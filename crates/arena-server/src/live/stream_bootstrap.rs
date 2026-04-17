use arena_core::{LiveEventEnvelope, LiveMatchSnapshot};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    ApiError,
    live::ReplayResult,
    state::AppState,
    storage::load_live_runtime_events_since,
};

pub(crate) async fn subscribe_live_socket(
    state: &AppState,
    match_id: Uuid,
    last_seq: Option<u64>,
) -> Result<
    (
        Vec<LiveEventEnvelope>,
        tokio::sync::broadcast::Receiver<LiveEventEnvelope>,
    ),
    ApiError,
> {
    state
        .live_matches
        .bootstrap_from_db(&state.db, match_id)
        .await?;
    let (snapshot, receiver) = state
        .live_matches
        .subscribe(match_id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("live state for match {match_id} not found")))?;
    let initial_events = initial_stream_events(state, match_id, last_seq, snapshot).await;
    Ok((initial_events, receiver))
}

pub(crate) async fn initial_stream_events(
    state: &AppState,
    match_id: Uuid,
    last_seq: Option<u64>,
    initial_snapshot: LiveMatchSnapshot,
) -> Vec<LiveEventEnvelope> {
    match last_seq {
        Some(seq) => match state.live_matches.replay_since(match_id, seq).await {
            Some(ReplayResult::Replay(events)) if !events.is_empty() => {
                state
                    .live_metrics
                    .replay_requests
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                state
                    .live_metrics
                    .replay_events_served
                    .fetch_add(events.len() as u64, std::sync::atomic::Ordering::Relaxed);
                info!(match_id = %match_id, from_seq = seq, replay_count = events.len(), "serving live replay events");
                events
            }
            Some(ReplayResult::Replay(_)) => {
                state
                    .live_metrics
                    .replay_requests
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                state
                    .live_metrics
                    .snapshot_fallbacks
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                info!(match_id = %match_id, from_seq = seq, "replay request already up to date, sending snapshot");
                vec![LiveEventEnvelope::Snapshot(initial_snapshot)]
            }
            Some(ReplayResult::SnapshotRequired) => {
                state
                    .live_metrics
                    .replay_requests
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                state
                    .live_metrics
                    .snapshot_fallbacks
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                warn!(match_id = %match_id, from_seq = seq, "live replay gap exceeded buffer, falling back to snapshot");
                vec![LiveEventEnvelope::Snapshot(initial_snapshot)]
            }
            None => {
                state
                    .live_metrics
                    .replay_requests
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                state
                    .live_metrics
                    .snapshot_fallbacks
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                warn!(match_id = %match_id, from_seq = seq, "live replay state missing, falling back to snapshot");
                vec![LiveEventEnvelope::Snapshot(initial_snapshot)]
            }
        },
        None => match load_live_runtime_events_since(&state.db, match_id, 0).await {
            Ok(events) if !events.is_empty() => {
                state
                    .live_metrics
                    .replay_events_served
                    .fetch_add(events.len() as u64, std::sync::atomic::Ordering::Relaxed);
                info!(match_id = %match_id, replay_count = events.len(), "serving full live history bootstrap");
                events
            }
            Ok(_) => {
                state
                    .live_metrics
                    .snapshot_fallbacks
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                vec![LiveEventEnvelope::Snapshot(initial_snapshot)]
            }
            Err(err) => {
                state
                    .live_metrics
                    .snapshot_fallbacks
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                warn!(match_id = %match_id, "failed to load live history bootstrap: {err:#}");
                vec![LiveEventEnvelope::Snapshot(initial_snapshot)]
            }
        },
    }
}
