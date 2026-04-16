use std::sync::atomic::Ordering;

use arena_core::{GameResult, MatchStatus};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    ApiError,
    gameplay::fen_for_variant,
    state::{AppState, MoveDebugContext},
};

use super::{
    logs::{match_runtime_log, match_runtime_source, push_runtime_log},
    publish::{
        CLOCK_SYNC_INTERVAL_MS, elapsed_since_turn_start_ms, emit_match_clock_sync,
        finalize_timeout, publish_match_runtime, remaining_turn_time_ms, update_terminal_state,
    },
    types::{HumanGameCommand, HumanMoveAck, MatchRuntime, MatchSeatController, MatchSession},
};

pub(crate) async fn process_human_move(
    state: &AppState,
    session: &MatchSession,
    runtime: &mut MatchRuntime,
    side: cozy_chess::Color,
    move_context: MoveDebugContext,
) -> HumanMoveAck {
    let source = match_runtime_source(session);
    let intent_id = move_context.intent_id;
    let move_uci = move_context.move_uci.clone();
    let submitted_log = match_runtime_log(
        session,
        runtime,
        source,
        "move.submitted",
        format!("human submitted {move_uci}"),
    )
    .with_move_uci(move_uci.clone())
    .with_fields(json!({
        "intent_id": intent_id,
        "client_action_id": move_context.client_action_id,
        "ws_connection_id": move_context.ws_connection_id,
        "request_id": move_context.request_id,
    }));
    push_runtime_log(&mut runtime.logs, submitted_log);
    {
        let seen_intents = match side {
            cozy_chess::Color::White => match &mut runtime.white_seat {
                MatchSeatController::Human(controller) => &mut controller.seen_intents,
                MatchSeatController::Engine(_) => return HumanMoveAck::RejectedNotYourTurn,
            },
            cozy_chess::Color::Black => match &mut runtime.black_seat {
                MatchSeatController::Human(controller) => &mut controller.seen_intents,
                MatchSeatController::Engine(_) => return HumanMoveAck::RejectedNotYourTurn,
            },
        };
        if let Some(previous) = seen_intents.get(&intent_id).copied() {
            return previous;
        }
    }
    if runtime.status != MatchStatus::Running {
        let rejected_log = match_runtime_log(
            session,
            runtime,
            source,
            "move.rejected_finished",
            "human move rejected because game is finished",
        )
        .with_move_uci(move_uci)
        .with_fields(json!({ "intent_id": intent_id }));
        push_runtime_log(&mut runtime.logs, rejected_log);
        insert_human_ack(runtime, side, intent_id, HumanMoveAck::RejectedGameFinished);
        return HumanMoveAck::RejectedGameFinished;
    }
    if runtime.board.side_to_move() != side {
        let rejected_log = match_runtime_log(
            session,
            runtime,
            source,
            "move.rejected_wrong_turn",
            "human move rejected because it is not the human turn",
        )
        .with_move_uci(move_uci)
        .with_fields(json!({ "intent_id": intent_id }));
        push_runtime_log(&mut runtime.logs, rejected_log);
        insert_human_ack(runtime, side, intent_id, HumanMoveAck::RejectedNotYourTurn);
        return HumanMoveAck::RejectedNotYourTurn;
    }
    if runtime.move_history.len() as u16 >= runtime.max_plies {
        runtime.result = Some(GameResult::Draw);
        runtime.termination = Some(arena_core::GameTermination::MoveLimit);
        runtime.status = MatchStatus::Completed;
        let _ = publish_match_runtime(state, session, runtime, false).await;
        insert_human_ack(runtime, side, intent_id, HumanMoveAck::RejectedGameFinished);
        return HumanMoveAck::RejectedGameFinished;
    }
    let handled_at = Utc::now();
    let elapsed_ms = elapsed_since_turn_start_ms(runtime);
    let increment_ms = runtime.time_control.increment_ms;
    let clock = if side == cozy_chess::Color::White {
        &mut runtime.white_time_left_ms
    } else {
        &mut runtime.black_time_left_ms
    };
    let remaining_before = *clock;
    *clock = clock.saturating_sub(elapsed_ms);
    if elapsed_ms >= remaining_before {
        runtime.result = Some(if side == cozy_chess::Color::White {
            GameResult::BlackWin
        } else {
            GameResult::WhiteWin
        });
        runtime.termination = Some(arena_core::GameTermination::Timeout);
        runtime.status = MatchStatus::Completed;
        *clock = 0;
        let timeout_log = match_runtime_log(
            session,
            runtime,
            source,
            "timeout.fired",
            "human move arrived after remaining clock expired",
        )
        .with_fields(json!({ "intent_id": intent_id }));
        push_runtime_log(&mut runtime.logs, timeout_log);
        let _ = publish_match_runtime(state, session, runtime, false).await;
        insert_human_ack(runtime, side, intent_id, HumanMoveAck::RejectedGameFinished);
        return HumanMoveAck::RejectedGameFinished;
    }
    let Ok(mv) = cozy_chess::util::parse_uci_move(&runtime.board, &move_uci) else {
        let rejected_log = match_runtime_log(
            session,
            runtime,
            source,
            "move.rejected_illegal",
            "human move rejected because UCI parsing failed",
        )
        .with_move_uci(move_uci)
        .with_fields(json!({ "intent_id": intent_id }));
        push_runtime_log(&mut runtime.logs, rejected_log);
        insert_human_ack(runtime, side, intent_id, HumanMoveAck::RejectedIllegal);
        return HumanMoveAck::RejectedIllegal;
    };
    if runtime.board.try_play(mv).is_err() {
        let rejected_log = match_runtime_log(
            session,
            runtime,
            source,
            "move.rejected_illegal",
            "human move rejected because it is illegal in the current position",
        )
        .with_move_uci(move_uci)
        .with_fields(json!({ "intent_id": intent_id }));
        push_runtime_log(&mut runtime.logs, rejected_log);
        insert_human_ack(runtime, side, intent_id, HumanMoveAck::RejectedIllegal);
        return HumanMoveAck::RejectedIllegal;
    }
    runtime.move_history.push(move_uci.clone());
    runtime.current_fen = fen_for_variant(&runtime.board, runtime.variant);
    let board_hash = runtime.board.hash_without_ep();
    *runtime.repetitions.entry(board_hash).or_insert(0) += 1;
    if side == cozy_chess::Color::White {
        runtime.white_time_left_ms = runtime.white_time_left_ms.saturating_add(increment_ms);
    } else {
        runtime.black_time_left_ms = runtime.black_time_left_ms.saturating_add(increment_ms);
    }
    runtime.turn_started_server_unix_ms = handled_at.timestamp_millis();
    let accepted_log = match_runtime_log(
        session,
        runtime,
        source,
        "move.accepted",
        "human move accepted",
    )
    .with_move_uci(move_uci)
    .with_fields(json!({
        "intent_id": intent_id,
        "client_action_id": move_context.client_action_id,
        "ws_connection_id": move_context.ws_connection_id,
    }));
    push_runtime_log(&mut runtime.logs, accepted_log);
    update_terminal_state(runtime);
    let _ = publish_match_runtime(state, session, runtime, false).await;
    insert_human_ack(runtime, side, intent_id, HumanMoveAck::Accepted);
    HumanMoveAck::Accepted
}

fn insert_human_ack(
    runtime: &mut MatchRuntime,
    side: cozy_chess::Color,
    intent_id: Uuid,
    ack: HumanMoveAck,
) {
    let seen_intents = match side {
        cozy_chess::Color::White => match &mut runtime.white_seat {
            MatchSeatController::Human(controller) => &mut controller.seen_intents,
            MatchSeatController::Engine(_) => return,
        },
        cozy_chess::Color::Black => match &mut runtime.black_seat {
            MatchSeatController::Human(controller) => &mut controller.seen_intents,
            MatchSeatController::Engine(_) => return,
        },
    };
    seen_intents.insert(intent_id, ack);
}

pub(crate) async fn process_human_turn(
    state: &AppState,
    session: &MatchSession,
    runtime: &mut MatchRuntime,
    side: cozy_chess::Color,
) -> Result<(), ApiError> {
    let source = match_runtime_source(session);
    let timeout_delay = remaining_turn_time_ms(runtime);
    if timeout_delay == 0 {
        state
            .live_metrics
            .timeout_fires
            .fetch_add(1, Ordering::Relaxed);
        return finalize_timeout(state, session, runtime, source).await;
    }
    let sleep = tokio::time::sleep(std::time::Duration::from_millis(timeout_delay));
    tokio::pin!(sleep);
    let sync = tokio::time::sleep(std::time::Duration::from_millis(
        CLOCK_SYNC_INTERVAL_MS.min(timeout_delay),
    ));
    tokio::pin!(sync);
    loop {
        let command_opt = match side {
            cozy_chess::Color::White => match &mut runtime.white_seat {
                MatchSeatController::Human(controller) => {
                    tokio::select! {
                        maybe_command = controller.command_rx.recv() => maybe_command,
                        _ = &mut sleep => {
                            state.live_metrics.timeout_fires.fetch_add(1, Ordering::Relaxed);
                            finalize_timeout(state, session, runtime, source).await?;
                            return Ok(());
                        }
                        _ = &mut sync => {
                            emit_match_clock_sync(state, session, runtime, source).await?;
                            let next_delay = CLOCK_SYNC_INTERVAL_MS.min(remaining_turn_time_ms(runtime).max(1));
                            sync.as_mut().reset(tokio::time::Instant::now() + std::time::Duration::from_millis(next_delay));
                            continue;
                        }
                    }
                }
                MatchSeatController::Engine(_) => return Ok(()),
            },
            cozy_chess::Color::Black => match &mut runtime.black_seat {
                MatchSeatController::Human(controller) => {
                    tokio::select! {
                        maybe_command = controller.command_rx.recv() => maybe_command,
                        _ = &mut sleep => {
                            state.live_metrics.timeout_fires.fetch_add(1, Ordering::Relaxed);
                            finalize_timeout(state, session, runtime, source).await?;
                            return Ok(());
                        }
                        _ = &mut sync => {
                            emit_match_clock_sync(state, session, runtime, source).await?;
                            let next_delay = CLOCK_SYNC_INTERVAL_MS.min(remaining_turn_time_ms(runtime).max(1));
                            sync.as_mut().reset(tokio::time::Instant::now() + std::time::Duration::from_millis(next_delay));
                            continue;
                        }
                    }
                }
                MatchSeatController::Engine(_) => return Ok(()),
            },
        };
        let Some(command) = command_opt else {
            return Ok(());
        };
        match command {
            HumanGameCommand::SubmitMove {
                intent_id,
                client_action_id,
                ws_connection_id,
                move_uci,
                respond_to,
            } => {
                let ack = process_human_move(
                    state,
                    session,
                    runtime,
                    side,
                    MoveDebugContext {
                        request_id: None,
                        client_action_id,
                        ws_connection_id,
                        intent_id,
                        move_uci,
                    },
                )
                .await;
                let _ = respond_to.send(ack);
                return Ok(());
            }
        }
    }
}
