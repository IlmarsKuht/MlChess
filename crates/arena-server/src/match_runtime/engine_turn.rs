use anyhow::Result;
use arena_core::{GameResult, MatchStatus};
use arena_runner::{AgentAdapter, calculate_move_budget};
use chrono::Utc;

use crate::{ApiError, gameplay::fen_for_variant, state::AppState};

use super::{
    logs::{match_runtime_log, match_runtime_source, push_runtime_log},
    publish::{
        CLOCK_SYNC_INTERVAL_MS, elapsed_since_turn_start_ms, emit_match_clock_sync,
        publish_match_runtime, remaining_turn_time_ms, update_terminal_state,
    },
    types::{MatchRuntime, MatchSeatController, MatchSession},
};

pub(crate) async fn process_engine_turn(
    state: &AppState,
    session: &MatchSession,
    runtime: &mut MatchRuntime,
    side: cozy_chess::Color,
) -> Result<(), ApiError> {
    let source = match_runtime_source(session);
    if runtime.status != MatchStatus::Running || runtime.board.side_to_move() != side {
        return Ok(());
    }
    if runtime.move_history.len() as u16 >= runtime.max_plies {
        runtime.result = Some(GameResult::Draw);
        runtime.termination = Some(arena_core::GameTermination::MoveLimit);
        runtime.status = MatchStatus::Completed;
        publish_match_runtime(state, session, runtime, false).await?;
        return Ok(());
    }
    let increment_ms = runtime.time_control.increment_ms;
    let movetime_ms = calculate_move_budget(
        if side == cozy_chess::Color::White {
            runtime.white_time_left_ms
        } else {
            runtime.black_time_left_ms
        },
        increment_ms,
    );
    let start_fen = runtime.start_fen.clone();
    let move_history = runtime.move_history.clone();
    let board = runtime.board.clone();
    let handled_at = Utc::now();
    let started_log = match_runtime_log(
        session,
        runtime,
        source,
        "engine.turn_started",
        "engine turn started",
    );
    push_runtime_log(&mut runtime.logs, started_log);
    let remaining = if side == cozy_chess::Color::White {
        runtime.white_time_left_ms
    } else {
        runtime.black_time_left_ms
    };
    let mut logs = std::mem::take(&mut runtime.logs);
    let mut adapter = take_engine_adapter(runtime, side)?;
    let timeout = tokio::time::sleep(std::time::Duration::from_millis(remaining.max(1)));
    tokio::pin!(timeout);
    let sync = tokio::time::sleep(std::time::Duration::from_millis(
        CLOCK_SYNC_INTERVAL_MS.min(remaining.max(1)),
    ));
    tokio::pin!(sync);
    enum EngineTurnOutcome {
        Move(String),
        Timeout,
        Error(anyhow::Error),
    }

    let selected = {
        let choose = adapter.choose_move(&board, &start_fen, &move_history, movetime_ms, &mut logs);
        tokio::pin!(choose);
        loop {
            tokio::select! {
                result = &mut choose => break match result {
                    Ok(selected) => EngineTurnOutcome::Move(selected),
                    Err(err) => EngineTurnOutcome::Error(err),
                },
                _ = &mut timeout => break EngineTurnOutcome::Timeout,
                _ = &mut sync => {
                    emit_match_clock_sync(state, session, runtime, source).await?;
                    let next_delay = CLOCK_SYNC_INTERVAL_MS.min(remaining_turn_time_ms(runtime).max(1));
                    sync.as_mut().reset(tokio::time::Instant::now() + std::time::Duration::from_millis(next_delay));
                }
            }
        }
    };
    restore_engine_adapter(runtime, side, adapter);
    runtime.logs = logs;
    let elapsed_ms = elapsed_since_turn_start_ms(runtime);
    let clock = if side == cozy_chess::Color::White {
        &mut runtime.white_time_left_ms
    } else {
        &mut runtime.black_time_left_ms
    };
    *clock = clock.saturating_sub(elapsed_ms);
    match selected {
        EngineTurnOutcome::Move(selected) => {
            let returned_log = match_runtime_log(
                session,
                runtime,
                source,
                "engine.move_returned",
                format!("engine returned {selected}"),
            )
            .with_move_uci(selected.clone());
            push_runtime_log(&mut runtime.logs, returned_log);
            if elapsed_ms >= remaining {
                runtime.result = Some(if side == cozy_chess::Color::White {
                    GameResult::BlackWin
                } else {
                    GameResult::WhiteWin
                });
                runtime.termination = Some(arena_core::GameTermination::Timeout);
                runtime.status = MatchStatus::Completed;
                let timeout_log = match_runtime_log(
                    session,
                    runtime,
                    source,
                    "timeout.fired",
                    "engine exceeded remaining time",
                );
                push_runtime_log(&mut runtime.logs, timeout_log);
            } else if selected == "0000" {
                runtime.result = Some(GameResult::Draw);
                runtime.termination = Some(arena_core::GameTermination::EngineFailure);
                runtime.status = MatchStatus::Completed;
            } else if let Ok(mv) = cozy_chess::util::parse_uci_move(&runtime.board, &selected) {
                if runtime.board.try_play(mv).is_err() {
                    runtime.result = Some(if side == cozy_chess::Color::White {
                        GameResult::BlackWin
                    } else {
                        GameResult::WhiteWin
                    });
                    runtime.termination = Some(arena_core::GameTermination::IllegalMove);
                    runtime.status = MatchStatus::Completed;
                } else {
                    runtime.move_history.push(selected);
                    runtime.current_fen = fen_for_variant(&runtime.board, runtime.variant);
                    let board_hash = runtime.board.hash_without_ep();
                    *runtime.repetitions.entry(board_hash).or_insert(0) += 1;
                    if side == cozy_chess::Color::White {
                        runtime.white_time_left_ms =
                            runtime.white_time_left_ms.saturating_add(increment_ms);
                    } else {
                        runtime.black_time_left_ms =
                            runtime.black_time_left_ms.saturating_add(increment_ms);
                    }
                    runtime.turn_started_server_unix_ms = handled_at.timestamp_millis();
                    update_terminal_state(runtime);
                }
            } else {
                runtime.result = Some(if side == cozy_chess::Color::White {
                    GameResult::BlackWin
                } else {
                    GameResult::WhiteWin
                });
                runtime.termination = Some(arena_core::GameTermination::IllegalMove);
                runtime.status = MatchStatus::Completed;
            }
        }
        EngineTurnOutcome::Timeout => {
            runtime.result = Some(if side == cozy_chess::Color::White {
                GameResult::BlackWin
            } else {
                GameResult::WhiteWin
            });
            runtime.termination = Some(arena_core::GameTermination::Timeout);
            runtime.status = MatchStatus::Completed;
        }
        EngineTurnOutcome::Error(err) => {
            runtime.result = Some(if side == cozy_chess::Color::White {
                GameResult::BlackWin
            } else {
                GameResult::WhiteWin
            });
            runtime.termination = Some(arena_core::GameTermination::EngineFailure);
            runtime.status = MatchStatus::Completed;
            let failure_log = match_runtime_log(
                session,
                runtime,
                source,
                "engine.move_failed",
                format!("engine move selection failed: {err}"),
            );
            push_runtime_log(&mut runtime.logs, failure_log);
        }
    }
    publish_match_runtime(state, session, runtime, false).await
}

fn take_engine_adapter(
    runtime: &mut MatchRuntime,
    side: cozy_chess::Color,
) -> Result<Box<dyn AgentAdapter>, ApiError> {
    match side {
        cozy_chess::Color::White => match &mut runtime.white_seat {
            MatchSeatController::Engine(controller) => controller
                .adapter
                .take()
                .ok_or_else(|| ApiError::Conflict("white engine seat unavailable".to_string())),
            MatchSeatController::Human(_) => Err(ApiError::Conflict(
                "white seat is not engine-controlled".to_string(),
            )),
        },
        cozy_chess::Color::Black => match &mut runtime.black_seat {
            MatchSeatController::Engine(controller) => controller
                .adapter
                .take()
                .ok_or_else(|| ApiError::Conflict("black engine seat unavailable".to_string())),
            MatchSeatController::Human(_) => Err(ApiError::Conflict(
                "black seat is not engine-controlled".to_string(),
            )),
        },
    }
}

fn restore_engine_adapter(
    runtime: &mut MatchRuntime,
    side: cozy_chess::Color,
    adapter: Box<dyn AgentAdapter>,
) {
    match side {
        cozy_chess::Color::White => {
            if let MatchSeatController::Engine(controller) = &mut runtime.white_seat {
                controller.adapter = Some(adapter);
            }
        }
        cozy_chess::Color::Black => {
            if let MatchSeatController::Engine(controller) = &mut runtime.black_seat {
                controller.adapter = Some(adapter);
            }
        }
    }
}
