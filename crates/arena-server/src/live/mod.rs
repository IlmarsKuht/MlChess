pub(crate) mod stream_bootstrap;

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use arena_core::{
    ClockSyncEvent, GameResult, GameTermination, LiveEventEnvelope, LiveEventType,
    LiveMatchSnapshot, LiveResult, LiveRuntimeCheckpoint, LiveStatus, MoveCommittedEvent,
    ProtocolLiveSide,
};
use tracing::info;
use uuid::Uuid;

use crate::{
    ApiError,
    state::LiveMetricsStore,
    storage::{
        insert_live_runtime_event, load_live_runtime_checkpoint, load_live_runtime_events_since,
        upsert_live_runtime_checkpoint,
    },
};

const LIVE_PROTOCOL_VERSION: u32 = 1;
const REPLAY_LIMIT: usize = 128;

#[derive(Clone, Default)]
pub(crate) struct LiveMatchStore {
    entries: Arc<tokio::sync::RwLock<HashMap<Uuid, LiveMatchEntry>>>,
}

#[derive(Clone)]
struct LiveMatchEntry {
    checkpoint: LiveRuntimeCheckpoint,
    replay: VecDeque<LiveEventEnvelope>,
    sender: tokio::sync::broadcast::Sender<LiveEventEnvelope>,
}

pub(crate) enum ReplayResult {
    Replay(Vec<LiveEventEnvelope>),
    SnapshotRequired,
}

impl LiveMatchStore {
    pub(crate) async fn bootstrap_from_db(
        &self,
        db: &sqlx::SqlitePool,
        match_id: Uuid,
    ) -> Result<(), ApiError> {
        if self.entries.read().await.contains_key(&match_id) {
            return Ok(());
        }
        let Some(checkpoint) = load_live_runtime_checkpoint(db, match_id).await? else {
            return Ok(());
        };
        let replay = load_live_runtime_events_since(
            db,
            match_id,
            checkpoint.seq.saturating_sub(REPLAY_LIMIT as u64),
        )
        .await?;
        let (sender, _) = tokio::sync::broadcast::channel(256);
        self.entries.write().await.insert(
            match_id,
            LiveMatchEntry {
                checkpoint,
                replay: replay.into(),
                sender,
            },
        );
        Ok(())
    }

    pub(crate) async fn get_snapshot(&self, match_id: Uuid) -> Option<LiveMatchSnapshot> {
        self.entries
            .read()
            .await
            .get(&match_id)
            .map(|entry| snapshot_from_checkpoint(&entry.checkpoint))
    }

    pub(crate) async fn subscribe(
        &self,
        match_id: Uuid,
    ) -> Option<(
        LiveMatchSnapshot,
        tokio::sync::broadcast::Receiver<LiveEventEnvelope>,
    )> {
        let entries = self.entries.read().await;
        let entry = entries.get(&match_id)?;
        Some((
            snapshot_from_checkpoint(&entry.checkpoint),
            entry.sender.subscribe(),
        ))
    }

    pub(crate) async fn replay_since(&self, match_id: Uuid, seq: u64) -> Option<ReplayResult> {
        self.entries.read().await.get(&match_id).map(|entry| {
            let oldest_seq = entry.replay.front().map(event_seq);
            if let Some(oldest_seq) = oldest_seq
                && seq.saturating_add(1) < oldest_seq
            {
                return ReplayResult::SnapshotRequired;
            }

            ReplayResult::Replay(
                entry
                    .replay
                    .iter()
                    .filter(|event| event_seq(event) > seq)
                    .cloned()
                    .collect(),
            )
        })
    }

    pub(crate) async fn publish(
        &self,
        db: &sqlx::SqlitePool,
        checkpoint: LiveRuntimeCheckpoint,
        event: LiveEventEnvelope,
    ) -> Result<(), ApiError> {
        let match_id = checkpoint.match_id;
        let seq = checkpoint.seq;
        let status = checkpoint.status;
        let event_name = match &event {
            LiveEventEnvelope::Snapshot(_) => "snapshot",
            LiveEventEnvelope::MoveCommitted(_) => "move_committed",
            LiveEventEnvelope::ClockSync(_) => "clock_sync",
            LiveEventEnvelope::GameFinished(_) => "game_finished",
        };
        upsert_live_runtime_checkpoint(db, &checkpoint).await?;
        insert_live_runtime_event(db, &event).await?;

        let sender = {
            let mut entries = self.entries.write().await;
            let entry = entries.entry(checkpoint.match_id).or_insert_with(|| {
                let (sender, _) = tokio::sync::broadcast::channel(256);
                LiveMatchEntry {
                    checkpoint: checkpoint.clone(),
                    replay: VecDeque::new(),
                    sender,
                }
            });
            entry.checkpoint = checkpoint;
            entry.replay.push_back(event.clone());
            while entry.replay.len() > REPLAY_LIMIT {
                entry.replay.pop_front();
            }
            entry.sender.clone()
        };
        let _ = sender.send(event);
        info!(
            match_id = %match_id,
            seq = seq,
            status = ?status,
            event_type = event_name,
            "published live runtime event"
        );
        Ok(())
    }

    pub(crate) async fn publish_transient(
        &self,
        checkpoint: LiveRuntimeCheckpoint,
        event: LiveEventEnvelope,
    ) {
        let sender = {
            let mut entries = self.entries.write().await;
            let entry = entries.entry(checkpoint.match_id).or_insert_with(|| {
                let (sender, _) = tokio::sync::broadcast::channel(256);
                LiveMatchEntry {
                    checkpoint: checkpoint.clone(),
                    replay: VecDeque::new(),
                    sender,
                }
            });
            entry.checkpoint = checkpoint;
            entry.replay.push_back(event.clone());
            while entry.replay.len() > REPLAY_LIMIT {
                entry.replay.pop_front();
            }
            entry.sender.clone()
        };
        let _ = sender.send(event);
    }
}

pub(crate) async fn publish_with_metrics(
    live_matches: &LiveMatchStore,
    db: &sqlx::SqlitePool,
    live_metrics: Option<&LiveMetricsStore>,
    checkpoint: LiveRuntimeCheckpoint,
    event: LiveEventEnvelope,
) -> Result<(), ApiError> {
    live_matches.publish(db, checkpoint, event).await?;
    if let Some(metrics) = live_metrics {
        metrics
            .published_events
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

pub(crate) async fn publish_transient_with_metrics(
    live_matches: &LiveMatchStore,
    live_metrics: Option<&LiveMetricsStore>,
    checkpoint: LiveRuntimeCheckpoint,
    event: LiveEventEnvelope,
) {
    live_matches.publish_transient(checkpoint, event).await;
    if let Some(metrics) = live_metrics {
        metrics
            .published_events
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

pub(crate) fn snapshot_from_checkpoint(checkpoint: &LiveRuntimeCheckpoint) -> LiveMatchSnapshot {
    LiveMatchSnapshot {
        protocol_version: LIVE_PROTOCOL_VERSION,
        event_type: LiveEventType::Snapshot,
        match_id: checkpoint.match_id,
        seq: checkpoint.seq,
        server_now_unix_ms: checkpoint.updated_at.timestamp_millis(),
        status: checkpoint.status,
        result: checkpoint.result,
        termination: checkpoint.termination,
        start_fen: checkpoint.start_fen.clone(),
        fen: checkpoint.fen.clone(),
        moves: checkpoint.moves.clone(),
        white_remaining_ms: checkpoint.white_remaining_ms,
        black_remaining_ms: checkpoint.black_remaining_ms,
        side_to_move: checkpoint.side_to_move,
        turn_started_server_unix_ms: checkpoint.turn_started_server_unix_ms,
    }
}

pub(crate) fn move_committed_from_checkpoint(
    checkpoint: &LiveRuntimeCheckpoint,
) -> MoveCommittedEvent {
    MoveCommittedEvent {
        protocol_version: LIVE_PROTOCOL_VERSION,
        event_type: LiveEventType::MoveCommitted,
        match_id: checkpoint.match_id,
        seq: checkpoint.seq,
        server_now_unix_ms: checkpoint.updated_at.timestamp_millis(),
        status: checkpoint.status,
        move_uci: checkpoint.moves.last().cloned().unwrap_or_default(),
        start_fen: checkpoint.start_fen.clone(),
        fen: checkpoint.fen.clone(),
        moves: checkpoint.moves.clone(),
        white_remaining_ms: checkpoint.white_remaining_ms,
        black_remaining_ms: checkpoint.black_remaining_ms,
        side_to_move: checkpoint.side_to_move,
        turn_started_server_unix_ms: checkpoint.turn_started_server_unix_ms,
    }
}

pub(crate) fn clock_sync_from_checkpoint(checkpoint: &LiveRuntimeCheckpoint) -> ClockSyncEvent {
    ClockSyncEvent {
        protocol_version: LIVE_PROTOCOL_VERSION,
        event_type: LiveEventType::ClockSync,
        match_id: checkpoint.match_id,
        seq: checkpoint.seq,
        server_now_unix_ms: checkpoint.updated_at.timestamp_millis(),
        status: checkpoint.status,
        white_remaining_ms: checkpoint.white_remaining_ms,
        black_remaining_ms: checkpoint.black_remaining_ms,
        side_to_move: checkpoint.side_to_move,
        turn_started_server_unix_ms: checkpoint.turn_started_server_unix_ms,
    }
}

pub(crate) fn game_finished_from_checkpoint(
    checkpoint: &LiveRuntimeCheckpoint,
) -> arena_core::GameFinishedEvent {
    arena_core::GameFinishedEvent {
        protocol_version: LIVE_PROTOCOL_VERSION,
        event_type: LiveEventType::GameFinished,
        match_id: checkpoint.match_id,
        seq: checkpoint.seq,
        server_now_unix_ms: checkpoint.updated_at.timestamp_millis(),
        status: checkpoint.status,
        result: checkpoint.result,
        termination: checkpoint.termination,
        start_fen: checkpoint.start_fen.clone(),
        fen: checkpoint.fen.clone(),
        moves: checkpoint.moves.clone(),
        white_remaining_ms: checkpoint.white_remaining_ms,
        black_remaining_ms: checkpoint.black_remaining_ms,
        side_to_move: ProtocolLiveSide::None,
        turn_started_server_unix_ms: checkpoint.turn_started_server_unix_ms,
    }
}

pub(crate) fn live_status_from_match_status(status: arena_core::MatchStatus) -> LiveStatus {
    match status {
        arena_core::MatchStatus::Running | arena_core::MatchStatus::Pending => LiveStatus::Running,
        arena_core::MatchStatus::Completed => LiveStatus::Finished,
        arena_core::MatchStatus::Failed | arena_core::MatchStatus::Skipped => LiveStatus::Aborted,
    }
}

pub(crate) fn live_result_from_game_result(result: GameResult) -> LiveResult {
    match result {
        GameResult::WhiteWin => LiveResult::WhiteWin,
        GameResult::BlackWin => LiveResult::BlackWin,
        GameResult::Draw => LiveResult::Draw,
    }
}

pub(crate) fn live_termination_from_game_termination(
    termination: GameTermination,
) -> arena_core::LiveTermination {
    match termination {
        GameTermination::Checkmate => arena_core::LiveTermination::Checkmate,
        GameTermination::Stalemate => arena_core::LiveTermination::Stalemate,
        GameTermination::FiftyMoveRule => arena_core::LiveTermination::FiftyMoveRule,
        GameTermination::Repetition => arena_core::LiveTermination::Repetition,
        GameTermination::InsufficientMaterial => arena_core::LiveTermination::InsufficientMaterial,
        GameTermination::Timeout => arena_core::LiveTermination::Timeout,
        GameTermination::Resignation => arena_core::LiveTermination::Resignation,
        GameTermination::IllegalMove => arena_core::LiveTermination::IllegalMove,
        GameTermination::MoveLimit => arena_core::LiveTermination::MoveLimit,
        GameTermination::EngineFailure => arena_core::LiveTermination::EngineFailure,
        GameTermination::Unknown => arena_core::LiveTermination::Abort,
    }
}

pub(crate) fn side_from_fen(fen: &str) -> ProtocolLiveSide {
    if fen.split_whitespace().nth(1) == Some("b") {
        ProtocolLiveSide::Black
    } else {
        ProtocolLiveSide::White
    }
}

fn event_seq(event: &LiveEventEnvelope) -> u64 {
    match event {
        LiveEventEnvelope::Snapshot(value) => value.seq,
        LiveEventEnvelope::MoveCommitted(value) => value.seq,
        LiveEventEnvelope::ClockSync(value) => value.seq,
        LiveEventEnvelope::GameFinished(value) => value.seq,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_core::LiveTermination;
    use chrono::Utc;
    use sqlx::sqlite::SqlitePoolOptions;

    use crate::db::init_db;

    fn checkpoint(
        match_id: Uuid,
        seq: u64,
        moves: &[&str],
        status: LiveStatus,
    ) -> LiveRuntimeCheckpoint {
        LiveRuntimeCheckpoint {
            match_id,
            seq,
            status,
            result: if status == LiveStatus::Finished {
                LiveResult::WhiteWin
            } else {
                LiveResult::None
            },
            termination: if status == LiveStatus::Finished {
                LiveTermination::Checkmate
            } else {
                LiveTermination::None
            },
            start_fen: "4k3/8/8/8/8/8/8/4K3 w - - 0 1".to_string(),
            fen: if moves.is_empty() {
                "4k3/8/8/8/8/8/8/4K3 w - - 0 1".to_string()
            } else {
                "4k3/8/8/8/8/8/8/4K3 b - - 0 1".to_string()
            },
            moves: moves.iter().map(|value| (*value).to_string()).collect(),
            white_remaining_ms: 60_000,
            black_remaining_ms: 60_000,
            side_to_move: if status == LiveStatus::Running {
                if moves.is_empty() {
                    ProtocolLiveSide::White
                } else {
                    ProtocolLiveSide::Black
                }
            } else {
                ProtocolLiveSide::None
            },
            turn_started_server_unix_ms: Utc::now().timestamp_millis(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn publish_emits_monotonic_snapshot_move_and_finish() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        init_db(&db).await.unwrap();
        let store = LiveMatchStore::default();
        let match_id = Uuid::new_v4();
        let first = checkpoint(match_id, 1, &[], LiveStatus::Running);
        let second = checkpoint(match_id, 2, &["e2e4"], LiveStatus::Running);
        let third = checkpoint(match_id, 3, &["e2e4"], LiveStatus::Finished);

        publish_with_metrics(
            &store,
            &db,
            None,
            first.clone(),
            LiveEventEnvelope::Snapshot(snapshot_from_checkpoint(&first)),
        )
        .await
        .unwrap();
        publish_with_metrics(
            &store,
            &db,
            None,
            second.clone(),
            LiveEventEnvelope::MoveCommitted(move_committed_from_checkpoint(&second)),
        )
        .await
        .unwrap();
        publish_with_metrics(
            &store,
            &db,
            None,
            third.clone(),
            LiveEventEnvelope::GameFinished(game_finished_from_checkpoint(&third)),
        )
        .await
        .unwrap();

        let ReplayResult::Replay(replay) = store.replay_since(match_id, 0).await.unwrap() else {
            panic!("expected replay events");
        };
        assert_eq!(replay.len(), 3);
        assert!(matches!(replay[0], LiveEventEnvelope::Snapshot(_)));
        assert!(matches!(replay[1], LiveEventEnvelope::MoveCommitted(_)));
        assert!(matches!(replay[2], LiveEventEnvelope::GameFinished(_)));
        assert_eq!(event_seq(&replay[0]), 1);
        assert_eq!(event_seq(&replay[1]), 2);
        assert_eq!(event_seq(&replay[2]), 3);

        let snapshot = store.get_snapshot(match_id).await.unwrap();
        assert_eq!(snapshot.seq, 3);
        assert_eq!(snapshot.status, LiveStatus::Finished);
    }

    #[tokio::test]
    async fn replay_since_returns_events_after_last_seen_seq() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        init_db(&db).await.unwrap();
        let store = LiveMatchStore::default();
        let match_id = Uuid::new_v4();
        let first = checkpoint(match_id, 1, &[], LiveStatus::Running);
        let second = checkpoint(match_id, 2, &["e2e4"], LiveStatus::Running);
        let third = checkpoint(match_id, 3, &["e2e4", "e7e5"], LiveStatus::Running);

        publish_with_metrics(
            &store,
            &db,
            None,
            first.clone(),
            LiveEventEnvelope::Snapshot(snapshot_from_checkpoint(&first)),
        )
        .await
        .unwrap();
        publish_with_metrics(
            &store,
            &db,
            None,
            second.clone(),
            LiveEventEnvelope::MoveCommitted(move_committed_from_checkpoint(&second)),
        )
        .await
        .unwrap();
        publish_with_metrics(
            &store,
            &db,
            None,
            third.clone(),
            LiveEventEnvelope::MoveCommitted(move_committed_from_checkpoint(&third)),
        )
        .await
        .unwrap();

        let ReplayResult::Replay(replay) = store.replay_since(match_id, 1).await.unwrap() else {
            panic!("expected replay events");
        };
        assert_eq!(replay.len(), 2);
        assert_eq!(event_seq(&replay[0]), 2);
        assert_eq!(event_seq(&replay[1]), 3);
    }

    #[tokio::test]
    async fn replay_since_requests_snapshot_when_gap_falls_outside_buffer() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        init_db(&db).await.unwrap();
        let store = LiveMatchStore::default();
        let match_id = Uuid::new_v4();

        for seq in 1..=(REPLAY_LIMIT as u64 + 3) {
            let moves = vec!["e2e4"; seq.saturating_sub(1) as usize];
            let checkpoint = LiveRuntimeCheckpoint {
                match_id,
                seq,
                status: LiveStatus::Running,
                result: LiveResult::None,
                termination: LiveTermination::None,
                start_fen: "4k3/8/8/8/8/8/8/4K3 w - - 0 1".to_string(),
                fen: if moves.is_empty() {
                    "4k3/8/8/8/8/8/8/4K3 w - - 0 1".to_string()
                } else {
                    "4k3/8/8/8/8/8/8/4K3 b - - 0 1".to_string()
                },
                moves: moves.iter().map(|value| (*value).to_string()).collect(),
                white_remaining_ms: 60_000,
                black_remaining_ms: 60_000,
                side_to_move: ProtocolLiveSide::White,
                turn_started_server_unix_ms: Utc::now().timestamp_millis(),
                updated_at: Utc::now(),
            };
            let event = if seq == 1 {
                LiveEventEnvelope::Snapshot(snapshot_from_checkpoint(&checkpoint))
            } else {
                LiveEventEnvelope::MoveCommitted(move_committed_from_checkpoint(&checkpoint))
            };
            publish_with_metrics(&store, &db, None, checkpoint, event)
                .await
                .unwrap();
        }

        assert!(matches!(
            store.replay_since(match_id, 1).await.unwrap(),
            ReplayResult::SnapshotRequired
        ));
    }
}
