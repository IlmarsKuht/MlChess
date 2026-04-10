use std::{collections::{HashMap, VecDeque}, sync::Arc};

use arena_core::{
    GameResult, GameTermination, LiveEventEnvelope, LiveEventType, LiveGameState, LiveMatchSnapshot,
    LiveResult, LiveRuntimeCheckpoint, LiveStatus, LiveTermination, MoveCommittedEvent, ProtocolLiveSide,
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::{ApiError, storage::{insert_live_runtime_event, load_live_runtime_checkpoint, load_live_runtime_events_since, upsert_live_runtime_checkpoint}};

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
        let replay = load_live_runtime_events_since(db, match_id, checkpoint.seq.saturating_sub(REPLAY_LIMIT as u64)).await?;
        let (sender, _) = tokio::sync::broadcast::channel(256);
        self.entries.write().await.insert(match_id, LiveMatchEntry {
            checkpoint,
            replay: replay.into(),
            sender,
        });
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
    ) -> Option<(LiveMatchSnapshot, tokio::sync::broadcast::Receiver<LiveEventEnvelope>)> {
        let entries = self.entries.read().await;
        let entry = entries.get(&match_id)?;
        Some((snapshot_from_checkpoint(&entry.checkpoint), entry.sender.subscribe()))
    }

    pub(crate) async fn replay_since(
        &self,
        match_id: Uuid,
        seq: u64,
    ) -> Option<Vec<LiveEventEnvelope>> {
        self.entries.read().await.get(&match_id).map(|entry| {
            entry
                .replay
                .iter()
                .filter(|event| event_seq(event) > seq)
                .cloned()
                .collect()
        })
    }

    pub(crate) async fn record_legacy_state(
        &self,
        db: &sqlx::SqlitePool,
        state: &LiveGameState,
    ) -> Result<(), ApiError> {
        let previous = self
            .entries
            .read()
            .await
            .get(&state.match_id)
            .map(|entry| entry.checkpoint.clone());
        let next_seq = previous.as_ref().map(|value| value.seq + 1).unwrap_or(1);
        let checkpoint = checkpoint_from_legacy_state(state, next_seq);
        let event = if previous.is_none() {
            LiveEventEnvelope::Snapshot(snapshot_from_checkpoint(&checkpoint))
        } else if checkpoint.status != LiveStatus::Running {
            LiveEventEnvelope::GameFinished(game_finished_from_checkpoint(&checkpoint))
        } else if checkpoint.moves.len() > previous.as_ref().map(|value| value.moves.len()).unwrap_or(0) {
            LiveEventEnvelope::MoveCommitted(move_committed_from_checkpoint(&checkpoint))
        } else {
            LiveEventEnvelope::Snapshot(snapshot_from_checkpoint(&checkpoint))
        };
        self.publish(db, checkpoint, event).await
    }

    pub(crate) async fn publish(
        &self,
        db: &sqlx::SqlitePool,
        checkpoint: LiveRuntimeCheckpoint,
        event: LiveEventEnvelope,
    ) -> Result<(), ApiError> {
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
        Ok(())
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
        fen: checkpoint.fen.clone(),
        moves: checkpoint.moves.clone(),
        white_remaining_ms: checkpoint.white_remaining_ms,
        black_remaining_ms: checkpoint.black_remaining_ms,
        side_to_move: checkpoint.side_to_move,
        turn_started_server_unix_ms: checkpoint.turn_started_server_unix_ms,
    }
}

pub(crate) fn checkpoint_from_legacy_state(
    state: &LiveGameState,
    seq: u64,
) -> LiveRuntimeCheckpoint {
    LiveRuntimeCheckpoint {
        match_id: state.match_id,
        seq,
        status: live_status_from_match_status(state.status),
        result: state.result.map(live_result_from_game_result).unwrap_or(LiveResult::None),
        termination: state
            .termination
            .map(live_termination_from_game_termination)
            .unwrap_or(LiveTermination::None),
        fen: state.current_fen.clone(),
        moves: state.moves_uci.clone(),
        white_remaining_ms: state.white_time_left_ms,
        black_remaining_ms: state.black_time_left_ms,
        side_to_move: if matches!(state.status, arena_core::MatchStatus::Completed | arena_core::MatchStatus::Failed | arena_core::MatchStatus::Skipped) {
            ProtocolLiveSide::None
        } else {
            side_from_fen(&state.current_fen)
        },
        turn_started_server_unix_ms: state.updated_at.timestamp_millis(),
        updated_at: state.updated_at,
    }
}

pub(crate) fn move_committed_from_checkpoint(checkpoint: &LiveRuntimeCheckpoint) -> MoveCommittedEvent {
    MoveCommittedEvent {
        protocol_version: LIVE_PROTOCOL_VERSION,
        event_type: LiveEventType::MoveCommitted,
        match_id: checkpoint.match_id,
        seq: checkpoint.seq,
        server_now_unix_ms: checkpoint.updated_at.timestamp_millis(),
        status: checkpoint.status,
        move_uci: checkpoint.moves.last().cloned().unwrap_or_default(),
        fen: checkpoint.fen.clone(),
        moves: checkpoint.moves.clone(),
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

pub(crate) fn live_termination_from_game_termination(termination: GameTermination) -> arena_core::LiveTermination {
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

pub(crate) fn checkpoint_from_protocol(
    snapshot: &LiveMatchSnapshot,
    updated_at: DateTime<Utc>,
) -> LiveRuntimeCheckpoint {
    LiveRuntimeCheckpoint {
        match_id: snapshot.match_id,
        seq: snapshot.seq,
        status: snapshot.status,
        result: snapshot.result,
        termination: snapshot.termination,
        fen: snapshot.fen.clone(),
        moves: snapshot.moves.clone(),
        white_remaining_ms: snapshot.white_remaining_ms,
        black_remaining_ms: snapshot.black_remaining_ms,
        side_to_move: snapshot.side_to_move,
        turn_started_server_unix_ms: snapshot.turn_started_server_unix_ms,
        updated_at,
    }
}
