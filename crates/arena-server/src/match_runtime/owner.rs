use anyhow::Result;
use arena_core::{GameRecord, MatchStatus};

use crate::{ApiError, state::AppState};

use super::{
    engine_turn::process_engine_turn,
    finalize::finalize_match_game,
    human_turn::process_human_turn,
    publish::publish_match_runtime,
    types::{MatchRuntime, MatchSeatController, MatchSession},
};

pub(crate) async fn run_match_owner(
    state: AppState,
    session: MatchSession,
    runtime: MatchRuntime,
    publish_initial_snapshot: bool,
) {
    let _ = run_match_to_completion(&state, session, runtime, publish_initial_snapshot).await;
}

pub(crate) async fn run_match_to_completion(
    state: &AppState,
    session: MatchSession,
    mut runtime: MatchRuntime,
    publish_initial_snapshot: bool,
) -> Result<GameRecord, ApiError> {
    if publish_initial_snapshot {
        publish_match_runtime(state, &session, &mut runtime, true).await?;
    }

    loop {
        if runtime.status != MatchStatus::Running {
            return finalize_match_game(state, session, runtime).await;
        }
        process_active_turn(state, &session, &mut runtime).await?;
    }
}

pub(crate) async fn process_active_turn(
    state: &AppState,
    session: &MatchSession,
    runtime: &mut MatchRuntime,
) -> Result<(), ApiError> {
    let side = runtime.active_side();
    let is_engine_turn = matches!(runtime.active_seat(), MatchSeatController::Engine(_));
    if is_engine_turn {
        process_engine_turn(state, session, runtime, side).await
    } else {
        process_human_turn(state, session, runtime, side).await
    }
}
