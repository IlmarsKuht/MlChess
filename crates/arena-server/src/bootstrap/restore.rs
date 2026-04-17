use anyhow::Result;
use arena_core::{LiveResult, LiveStatus, LiveTermination, ProtocolLiveSide};
use tracing::{error, info};

use crate::{
    human_games::service::restore_human_game,
    live,
    state::AppState,
    tournaments::service::restore_engine_game,
};
pub(crate) async fn restore_live_runtime(state: &AppState) -> Result<()> {
    let checkpoints =
        crate::storage::list_live_runtime_checkpoints(&state.db, Some(LiveStatus::Running)).await?;
    let human_player = crate::storage::ensure_human_player(&state.db).await?;
    state.live_metrics.restored_matches.store(
        checkpoints.len() as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
    info!(
        restored_matches = checkpoints.len(),
        "restoring live runtime checkpoints"
    );
    for checkpoint in checkpoints {
        let match_series =
            match crate::storage::get_match_series(&state.db, checkpoint.match_id).await {
                Ok(value) => value,
                Err(err) => {
                    error!(
                        "failed to load match series for restored live match {}: {err}",
                        checkpoint.match_id
                    );
                    continue;
                }
            };
        state
            .live_matches
            .bootstrap_from_db(&state.db, checkpoint.match_id)
            .await?;
        if match_series.white_version_id == human_player.id
            || match_series.black_version_id == human_player.id
        {
            if let Err(err) = restore_human_game(state, checkpoint.clone()).await {
                error!(
                    "failed to restore human live match {}: {err}",
                    checkpoint.match_id
                );
                fail_closed_live_match(state, &match_series, checkpoint.clone()).await?;
            }
        } else {
            if let Err(err) = restore_engine_game(state, checkpoint.clone()).await {
                error!(
                    "failed to restore engine live match {}: {err}",
                    checkpoint.match_id
                );
                fail_closed_live_match(state, &match_series, checkpoint.clone()).await?;
            }
        }
    }
    Ok(())
}

async fn fail_closed_live_match(
    state: &AppState,
    match_series: &arena_core::MatchSeries,
    mut checkpoint: arena_core::LiveRuntimeCheckpoint,
) -> Result<()> {
    checkpoint.seq = checkpoint.seq.saturating_add(1);
    checkpoint.status = LiveStatus::Aborted;
    checkpoint.result = LiveResult::None;
    checkpoint.termination = LiveTermination::Abort;
    checkpoint.side_to_move = ProtocolLiveSide::None;
    checkpoint.updated_at = chrono::Utc::now();
    let event = arena_core::LiveEventEnvelope::GameFinished(live::game_finished_from_checkpoint(
        &checkpoint,
    ));
    let mut tx = state.db.begin().await?;
    crate::storage::upsert_live_runtime_checkpoint_tx(&mut tx, &checkpoint).await?;
    crate::storage::insert_live_runtime_event_tx(&mut tx, &event).await?;
    crate::storage::update_match_series_status_tx(
        &mut tx,
        match_series.id,
        arena_core::MatchStatus::Failed,
    )
    .await?;
    crate::storage::update_tournament_status_tx(
        &mut tx,
        match_series.tournament_id,
        arena_core::TournamentStatus::Failed,
        None,
        Some(chrono::Utc::now()),
    )
    .await?;
    tx.commit().await?;
    live::publish_transient_with_metrics(
        &state.live_matches,
        Some(&state.live_metrics),
        checkpoint,
        event,
    )
    .await;
    Ok(())
}
