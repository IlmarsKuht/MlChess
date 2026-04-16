use anyhow::Result;
use arena_core::{GameRecord, GameResult, MatchStatus, TournamentStatus};
use chrono::Utc;
use sqlx::{Sqlite, Transaction};
use uuid::Uuid;

use crate::{
    ApiError,
    human_games::service::apply_human_pool_rating_update,
    live::game_finished_from_checkpoint,
    state::AppState,
    storage::{
        insert_game_tx, insert_human_game_tx, insert_live_runtime_event_tx,
        update_match_series_status_tx, update_tournament_status_tx,
        upsert_live_runtime_checkpoint_tx,
    },
};

use super::{
    logs::{match_runtime_log, match_runtime_source, push_runtime_log},
    publish::match_checkpoint,
    types::{CompletedGameTable, MatchRuntime, MatchSeatController, MatchSession},
};

pub(crate) async fn finalize_match_game(
    state: &AppState,
    session: MatchSession,
    mut runtime: MatchRuntime,
) -> Result<GameRecord, ApiError> {
    let human_player = match (&runtime.white_seat, &runtime.black_seat) {
        (MatchSeatController::Human(controller), _) => Some(controller.player.clone()),
        (_, MatchSeatController::Human(controller)) => Some(controller.player.clone()),
        _ => None,
    };
    shutdown_match_seats(&mut runtime).await;
    if runtime.has_human_seat() {
        state.human_games.remove(session.match_series.id).await;
    }
    let final_log = match_runtime_log(
        &session,
        &runtime,
        match_runtime_source(&session),
        "game.finalized",
        "game finalized",
    );
    push_runtime_log(&mut runtime.logs, final_log);
    runtime.seq += 1;
    let result = runtime.result.unwrap_or(GameResult::Draw);
    let termination = runtime
        .termination
        .unwrap_or(arena_core::GameTermination::Unknown);
    let completed_at = Utc::now();
    let game = GameRecord {
        id: Uuid::new_v4(),
        tournament_id: runtime.tournament_id,
        match_id: session.match_series.id,
        pool_id: session.match_series.pool_id,
        variant: runtime.variant,
        opening_id: session.match_series.opening_id,
        white_version_id: session.match_series.white_version_id,
        black_version_id: session.match_series.black_version_id,
        result,
        termination,
        start_fen: runtime.start_fen.clone(),
        pgn: arena_runner::pgn_from_moves(
            &session.name,
            runtime.variant,
            &runtime.start_fen,
            &runtime.move_history,
            result,
        ),
        moves_uci: runtime.move_history.clone(),
        white_time_left_ms: runtime.white_time_left_ms,
        black_time_left_ms: runtime.black_time_left_ms,
        logs: runtime.logs.clone(),
        started_at: runtime.started_at,
        completed_at,
    };
    let checkpoint = match_checkpoint(&session, &runtime);
    let event =
        arena_core::LiveEventEnvelope::GameFinished(game_finished_from_checkpoint(&checkpoint));
    persist_terminal_match_state(
        state,
        session.completed_game_table,
        &game,
        MatchStatus::Completed,
        TournamentStatus::Completed,
        runtime.started_at,
        checkpoint,
        event,
    )
    .await?;
    if let Some(human_player) = human_player {
        apply_human_pool_rating_update(
            &state.db,
            session.match_series.pool_id,
            &game,
            &human_player,
        )
        .await?;
    }
    Ok(game)
}

pub(crate) async fn shutdown_match_seats(runtime: &mut MatchRuntime) {
    for seat in [&mut runtime.white_seat, &mut runtime.black_seat] {
        if let MatchSeatController::Engine(engine) = seat {
            if let Some(mut adapter) = engine.adapter.take() {
                adapter.shutdown(&mut runtime.logs).await.ok();
                engine.adapter = Some(adapter);
            }
        }
    }
}

async fn persist_terminal_match_state(
    state: &AppState,
    completed_game_table: CompletedGameTable,
    game: &GameRecord,
    match_status: MatchStatus,
    tournament_status: TournamentStatus,
    tournament_started_at: chrono::DateTime<Utc>,
    checkpoint: arena_core::LiveRuntimeCheckpoint,
    event: arena_core::LiveEventEnvelope,
) -> Result<(), ApiError> {
    let mut tx: Transaction<'_, Sqlite> = state.db.begin().await?;
    match completed_game_table {
        CompletedGameTable::Engine => insert_game_tx(&mut tx, game).await?,
        CompletedGameTable::Human => insert_human_game_tx(&mut tx, game).await?,
    }
    update_match_series_status_tx(&mut tx, game.match_id, match_status).await?;
    update_tournament_status_tx(
        &mut tx,
        game.tournament_id,
        tournament_status,
        Some(tournament_started_at),
        Some(game.completed_at),
    )
    .await?;
    upsert_live_runtime_checkpoint_tx(&mut tx, &checkpoint).await?;
    insert_live_runtime_event_tx(&mut tx, &event).await?;
    tx.commit().await?;
    crate::live::publish_transient_with_metrics(
        &state.live_matches,
        Some(&state.live_metrics),
        checkpoint,
        event,
    )
    .await;
    Ok(())
}
