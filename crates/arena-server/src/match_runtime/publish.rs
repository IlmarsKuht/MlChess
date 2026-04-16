use anyhow::Result;
use arena_core::{GameResult, LiveRuntimeCheckpoint, MatchStatus};
use chrono::Utc;

use crate::{
    ApiError,
    live::{
        clock_sync_from_checkpoint, live_result_from_game_result, live_status_from_match_status,
        live_termination_from_game_termination, move_committed_from_checkpoint,
        publish_transient_with_metrics, publish_with_metrics, side_from_fen,
        snapshot_from_checkpoint,
    },
    state::AppState,
};

use super::{
    logs::{match_runtime_log, match_runtime_source, push_runtime_log},
    types::{MatchRuntime, MatchSession},
};

pub(crate) const CLOCK_SYNC_INTERVAL_MS: u64 = 1_000;

pub(crate) fn match_checkpoint(
    session: &MatchSession,
    runtime: &MatchRuntime,
) -> LiveRuntimeCheckpoint {
    let updated_at = Utc::now();
    LiveRuntimeCheckpoint {
        match_id: session.match_series.id,
        seq: runtime.seq,
        status: live_status_from_match_status(runtime.status),
        result: runtime
            .result
            .map(live_result_from_game_result)
            .unwrap_or(arena_core::LiveResult::None),
        termination: runtime
            .termination
            .map(live_termination_from_game_termination)
            .unwrap_or(arena_core::LiveTermination::None),
        start_fen: runtime.start_fen.clone(),
        fen: runtime.current_fen.clone(),
        moves: runtime.move_history.clone(),
        white_remaining_ms: runtime.white_time_left_ms,
        black_remaining_ms: runtime.black_time_left_ms,
        side_to_move: if runtime.status == MatchStatus::Running {
            side_from_fen(&runtime.current_fen)
        } else {
            arena_core::ProtocolLiveSide::None
        },
        turn_started_server_unix_ms: runtime.turn_started_server_unix_ms,
        updated_at,
    }
}

pub(crate) fn elapsed_since_turn_start_ms(runtime: &MatchRuntime) -> u64 {
    Utc::now()
        .timestamp_millis()
        .saturating_sub(runtime.turn_started_server_unix_ms) as u64
}

pub(crate) fn remaining_turn_time_ms(runtime: &MatchRuntime) -> u64 {
    let remaining = if runtime.board.side_to_move() == cozy_chess::Color::White {
        runtime.white_time_left_ms
    } else {
        runtime.black_time_left_ms
    };
    remaining.saturating_sub(elapsed_since_turn_start_ms(runtime))
}

pub(crate) async fn emit_match_clock_sync(
    state: &AppState,
    session: &MatchSession,
    runtime: &mut MatchRuntime,
    source: &str,
) -> Result<(), ApiError> {
    if runtime.status != MatchStatus::Running {
        return Ok(());
    }
    let elapsed_ms = elapsed_since_turn_start_ms(runtime);
    if elapsed_ms == 0 {
        return Ok(());
    }
    if runtime.board.side_to_move() == cozy_chess::Color::White {
        runtime.white_time_left_ms = runtime.white_time_left_ms.saturating_sub(elapsed_ms);
    } else {
        runtime.black_time_left_ms = runtime.black_time_left_ms.saturating_sub(elapsed_ms);
    }
    runtime.turn_started_server_unix_ms = Utc::now().timestamp_millis();
    runtime.seq += 1;
    let checkpoint = match_checkpoint(session, runtime);
    let clock_log = match_runtime_log(
        session,
        runtime,
        source,
        "clock.sync_emitted",
        "clock sync emitted",
    );
    push_runtime_log(&mut runtime.logs, clock_log);
    publish_transient_with_metrics(
        &state.live_matches,
        Some(&state.live_metrics),
        checkpoint.clone(),
        arena_core::LiveEventEnvelope::ClockSync(clock_sync_from_checkpoint(&checkpoint)),
    )
    .await;
    Ok(())
}

pub(crate) async fn finalize_timeout(
    state: &AppState,
    session: &MatchSession,
    runtime: &mut MatchRuntime,
    source: &str,
) -> Result<(), ApiError> {
    if runtime.status != MatchStatus::Running {
        return Ok(());
    }
    runtime.result = Some(
        if runtime.board.side_to_move() == cozy_chess::Color::White {
            GameResult::BlackWin
        } else {
            GameResult::WhiteWin
        },
    );
    runtime.termination = Some(arena_core::GameTermination::Timeout);
    runtime.status = MatchStatus::Completed;
    if runtime.board.side_to_move() == cozy_chess::Color::White {
        runtime.white_time_left_ms = 0;
    } else {
        runtime.black_time_left_ms = 0;
    }
    let timeout_log = match_runtime_log(session, runtime, source, "timeout.fired", "timeout fired");
    push_runtime_log(&mut runtime.logs, timeout_log);
    publish_match_runtime(state, session, runtime, false).await
}

pub(crate) async fn publish_match_runtime(
    state: &AppState,
    session: &MatchSession,
    runtime: &mut MatchRuntime,
    initial: bool,
) -> Result<(), ApiError> {
    if !initial && runtime.status != MatchStatus::Running {
        return Ok(());
    }
    runtime.seq += 1;
    let checkpoint = match_checkpoint(session, runtime);
    let event_name = if initial {
        "live.snapshot"
    } else {
        "live.move_published"
    };
    let runtime_log_entry = match_runtime_log(
        session,
        runtime,
        match_runtime_source(session),
        event_name,
        format!("runtime event {}", runtime.seq),
    );
    push_runtime_log(&mut runtime.logs, runtime_log_entry);
    let event = if initial {
        arena_core::LiveEventEnvelope::Snapshot(snapshot_from_checkpoint(&checkpoint))
    } else {
        arena_core::LiveEventEnvelope::MoveCommitted(move_committed_from_checkpoint(&checkpoint))
    };
    publish_with_metrics(
        &state.live_matches,
        &state.db,
        Some(&state.live_metrics),
        checkpoint,
        event,
    )
    .await
}

pub(crate) fn update_terminal_state(runtime: &mut MatchRuntime) {
    if let Some((result, termination)) =
        arena_runner::classify_position(&runtime.board, &runtime.repetitions)
    {
        runtime.result = Some(result);
        runtime.termination = Some(termination);
        runtime.status = MatchStatus::Completed;
    } else if runtime.board.status() != cozy_chess::GameStatus::Ongoing {
        let (result, termination) = arena_runner::classify_terminal_board(&runtime.board);
        runtime.result = Some(result);
        runtime.termination = Some(termination);
        runtime.status = MatchStatus::Completed;
    }
}
