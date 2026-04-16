use std::collections::HashMap;

use anyhow::Result;
use arena_core::{
    GameRecord, GameResult, LiveRuntimeCheckpoint, MatchSeries, MatchStatus, Tournament,
    TournamentKind, TournamentStatus, snapshot_from_entry,
};
use arena_runner::build_adapter;
use chrono::Utc;
use serde_json::json;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    ApiError,
    gameplay::{
        MatchConfig, ensure_engine_supports_variant, parse_saved_board, resolve_start_state,
    },
    match_runtime::{
        logs::{human_runtime_log, push_runtime_log},
        owner::run_match_owner,
        types::{
            CompletedGameTable, EngineSeatController, HumanGameCommand, HumanGameHandle,
            HumanMoveAck, HumanPlayer, HumanSeatController, MatchRuntime, MatchSeatController,
            MatchSession,
        },
    },
    presentation::HumanPlayerProfile,
    rating::build_pair_rating_update,
    state::{AppState, MoveDebugContext},
    storage::{
        ensure_human_player, get_agent_version, get_match_series, get_pool, get_tournament,
        insert_human_rating_snapshot, insert_match_series_tx, insert_rating_snapshot,
        insert_tournament_tx, load_human_profile, load_pool_leaderboard, load_pool_openings,
    },
};

pub(crate) async fn create_human_game(
    state: &AppState,
    name: String,
    pool_id: Uuid,
    engine_version_id: Uuid,
    human_plays_white: bool,
) -> Result<(Uuid, Uuid), ApiError> {
    let pool = get_pool(&state.db, pool_id).await?;
    let engine_version = get_agent_version(&state.db, engine_version_id).await?;
    ensure_engine_supports_variant(&engine_version, pool.variant)?;
    let human_player = ensure_human_player(&state.db).await?;
    let openings = load_pool_openings(&state.db, &pool).await?;
    let opening = openings.first().cloned();
    let (board, start_fen) = resolve_start_state(MatchConfig {
        variant: pool.variant,
        opening: opening.as_ref(),
        opening_seed: None,
    })?;
    let match_id = Uuid::new_v4();
    let tournament_id = Uuid::new_v4();
    let created_at = Utc::now();
    let tournament = Tournament {
        id: tournament_id,
        name: name.clone(),
        kind: TournamentKind::RoundRobin,
        pool_id: pool.id,
        participant_version_ids: vec![human_player.id, engine_version.id],
        worker_count: 1,
        games_per_pairing: 1,
        status: TournamentStatus::Running,
        created_at,
        started_at: Some(created_at),
        completed_at: None,
    };
    let match_series = MatchSeries {
        id: match_id,
        tournament_id,
        pool_id: pool.id,
        round_index: 0,
        white_version_id: if human_plays_white {
            human_player.id
        } else {
            engine_version.id
        },
        black_version_id: if human_plays_white {
            engine_version.id
        } else {
            human_player.id
        },
        opening_id: opening.as_ref().map(|value| value.id),
        game_index: 0,
        status: MatchStatus::Running,
        created_at,
    };
    let mut tx = state.db.begin().await?;
    insert_tournament_tx(&mut tx, &tournament).await?;
    insert_match_series_tx(&mut tx, &match_series).await?;
    tx.commit().await?;
    let mut logs = Vec::new();
    let mut engine = build_adapter(engine_version);
    engine.prepare(pool.variant, &mut logs).await?;
    engine.begin_game(&mut logs).await?;
    let initial_hash = board.hash_without_ep();
    let (command_tx, command_rx) = tokio::sync::mpsc::channel(32);
    let human_side = if human_plays_white {
        cozy_chess::Color::White
    } else {
        cozy_chess::Color::Black
    };
    let (white_seat, black_seat) = if human_side == cozy_chess::Color::White {
        (
            MatchSeatController::Human(HumanSeatController {
                player: human_player.clone(),
                command_rx,
                seen_intents: HashMap::new(),
            }),
            MatchSeatController::Engine(EngineSeatController {
                adapter: Some(engine),
            }),
        )
    } else {
        (
            MatchSeatController::Engine(EngineSeatController {
                adapter: Some(engine),
            }),
            MatchSeatController::Human(HumanSeatController {
                player: human_player.clone(),
                command_rx,
                seen_intents: HashMap::new(),
            }),
        )
    };
    let mut runtime = MatchRuntime {
        tournament_id,
        variant: pool.variant,
        time_control: pool.time_control.clone(),
        start_fen: start_fen.clone(),
        current_fen: start_fen.clone(),
        board,
        repetitions: HashMap::from([(initial_hash, 1)]),
        move_history: Vec::new(),
        white_time_left_ms: pool.time_control.initial_ms,
        black_time_left_ms: pool.time_control.initial_ms,
        max_plies: 300,
        white_seat,
        black_seat,
        logs,
        started_at: created_at,
        turn_started_server_unix_ms: created_at.timestamp_millis(),
        seq: 0,
        result: None,
        termination: None,
        status: MatchStatus::Running,
    };
    let session = MatchSession {
        name,
        match_series: match_series.clone(),
        completed_game_table: CompletedGameTable::Human,
    };
    let created_log = human_runtime_log(
        &session,
        &runtime,
        "human_game.created",
        "human game created",
    )
    .with_fields(json!({
        "human_player_id": human_player.id,
        "white_version_id": session.match_series.white_version_id,
        "black_version_id": session.match_series.black_version_id,
    }));
    push_runtime_log(&mut runtime.logs, created_log);
    state
        .human_games
        .insert(match_series.id, HumanGameHandle { command_tx })
        .await;
    tokio::spawn(run_match_owner(state.clone(), session, runtime, true));

    Ok((match_id, tournament_id))
}

pub(crate) async fn load_human_player_profile(
    db: &SqlitePool,
) -> Result<HumanPlayerProfile, ApiError> {
    let player = ensure_human_player(db).await?;
    load_human_profile(db, &player).await
}

pub(crate) async fn submit_human_move(
    state: AppState,
    match_id: Uuid,
    move_context: MoveDebugContext,
) -> Result<&'static str, ApiError> {
    let session = state
        .human_games
        .get(match_id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("human game {match_id} not found")))?;
    let (respond_to, receive_ack) = tokio::sync::oneshot::channel();
    session
        .command_tx
        .send(HumanGameCommand::SubmitMove {
            intent_id: move_context.intent_id,
            client_action_id: move_context.client_action_id,
            ws_connection_id: move_context.ws_connection_id,
            move_uci: move_context.move_uci.clone(),
            respond_to,
        })
        .await
        .map_err(|_| ApiError::Conflict("game owner is unavailable".to_string()))?;
    match receive_ack
        .await
        .map_err(|_| ApiError::Conflict("game owner is unavailable".to_string()))?
    {
        HumanMoveAck::Accepted => return Ok("accepted"),
        HumanMoveAck::RejectedIllegal => {
            return Err(ApiError::BadRequest("illegal move".to_string()));
        }
        HumanMoveAck::RejectedNotYourTurn => {
            return Err(ApiError::Conflict("it is not your turn".to_string()));
        }
        HumanMoveAck::RejectedGameFinished => {
            return Err(ApiError::Conflict("game is no longer running".to_string()));
        }
    }
}

pub(crate) async fn restore_human_game(
    state: &AppState,
    checkpoint: LiveRuntimeCheckpoint,
) -> Result<(), ApiError> {
    let match_series = get_match_series(&state.db, checkpoint.match_id).await?;
    let pool = get_pool(&state.db, match_series.pool_id).await?;
    let tournament = get_tournament(&state.db, match_series.tournament_id).await?;
    let human_player = ensure_human_player(&state.db).await?;
    let engine_version_id = if match_series.white_version_id == human_player.id {
        match_series.black_version_id
    } else {
        match_series.white_version_id
    };
    let engine_version = get_agent_version(&state.db, engine_version_id).await?;
    ensure_engine_supports_variant(&engine_version, pool.variant)?;
    let human_side = if match_series.white_version_id == human_player.id {
        cozy_chess::Color::White
    } else {
        cozy_chess::Color::Black
    };
    let (mut board, start_fen) = parse_saved_board(pool.variant, &checkpoint.start_fen)
        .map_err(|err| ApiError::Conflict(format!("failed to restore start FEN: {err}")))?;
    let mut repetitions = HashMap::from([(board.hash_without_ep(), 1_u8)]);
    for uci in &checkpoint.moves {
        let mv = cozy_chess::util::parse_uci_move(&board, uci)
            .map_err(|err| ApiError::Conflict(format!("failed to restore move {uci}: {err}")))?;
        board
            .try_play(mv)
            .map_err(|_| ApiError::Conflict(format!("failed to replay restored move {uci}")))?;
        *repetitions.entry(board.hash_without_ep()).or_insert(0) += 1;
    }

    let mut logs = Vec::new();
    let mut engine = build_adapter(engine_version);
    engine.prepare(pool.variant, &mut logs).await?;
    engine.begin_game(&mut logs).await?;
    let (command_tx, command_rx) = tokio::sync::mpsc::channel(32);
    let (white_seat, black_seat) = if human_side == cozy_chess::Color::White {
        (
            MatchSeatController::Human(HumanSeatController {
                player: human_player.clone(),
                command_rx,
                seen_intents: HashMap::new(),
            }),
            MatchSeatController::Engine(EngineSeatController {
                adapter: Some(engine),
            }),
        )
    } else {
        (
            MatchSeatController::Engine(EngineSeatController {
                adapter: Some(engine),
            }),
            MatchSeatController::Human(HumanSeatController {
                player: human_player.clone(),
                command_rx,
                seen_intents: HashMap::new(),
            }),
        )
    };
    let runtime = MatchRuntime {
        tournament_id: match_series.tournament_id,
        variant: pool.variant,
        time_control: pool.time_control.clone(),
        start_fen,
        current_fen: checkpoint.fen.clone(),
        board,
        repetitions,
        move_history: checkpoint.moves.clone(),
        white_time_left_ms: checkpoint.white_remaining_ms,
        black_time_left_ms: checkpoint.black_remaining_ms,
        max_plies: 300,
        white_seat,
        black_seat,
        logs,
        started_at: tournament.started_at.unwrap_or(match_series.created_at),
        turn_started_server_unix_ms: checkpoint.turn_started_server_unix_ms,
        seq: checkpoint.seq,
        result: match checkpoint.result {
            arena_core::LiveResult::WhiteWin => Some(GameResult::WhiteWin),
            arena_core::LiveResult::BlackWin => Some(GameResult::BlackWin),
            arena_core::LiveResult::Draw => Some(GameResult::Draw),
            arena_core::LiveResult::None => None,
        },
        termination: match checkpoint.termination {
            arena_core::LiveTermination::Checkmate => Some(arena_core::GameTermination::Checkmate),
            arena_core::LiveTermination::Timeout => Some(arena_core::GameTermination::Timeout),
            arena_core::LiveTermination::Resignation => {
                Some(arena_core::GameTermination::Resignation)
            }
            arena_core::LiveTermination::Abort => Some(arena_core::GameTermination::Unknown),
            arena_core::LiveTermination::Stalemate => Some(arena_core::GameTermination::Stalemate),
            arena_core::LiveTermination::Repetition => {
                Some(arena_core::GameTermination::Repetition)
            }
            arena_core::LiveTermination::InsufficientMaterial => {
                Some(arena_core::GameTermination::InsufficientMaterial)
            }
            arena_core::LiveTermination::FiftyMoveRule => {
                Some(arena_core::GameTermination::FiftyMoveRule)
            }
            arena_core::LiveTermination::IllegalMove => {
                Some(arena_core::GameTermination::IllegalMove)
            }
            arena_core::LiveTermination::MoveLimit => Some(arena_core::GameTermination::MoveLimit),
            arena_core::LiveTermination::EngineFailure => {
                Some(arena_core::GameTermination::EngineFailure)
            }
            arena_core::LiveTermination::None => None,
        },
        status: match checkpoint.status {
            arena_core::LiveStatus::Running => MatchStatus::Running,
            arena_core::LiveStatus::Finished => MatchStatus::Completed,
            arena_core::LiveStatus::Aborted => MatchStatus::Failed,
        },
    };
    let session = MatchSession {
        name: tournament.name,
        match_series: match_series.clone(),
        completed_game_table: CompletedGameTable::Human,
    };
    state
        .human_games
        .insert(match_series.id, HumanGameHandle { command_tx })
        .await;
    tokio::spawn(run_match_owner(state.clone(), session, runtime, false));
    Ok(())
}

pub(crate) async fn apply_human_pool_rating_update(
    db: &SqlitePool,
    pool_id: Uuid,
    game: &GameRecord,
    human_player: &HumanPlayer,
) -> Result<()> {
    let entries = load_pool_leaderboard(db, pool_id)
        .await?
        .into_iter()
        .map(|entry| (entry.agent_version_id, entry))
        .collect::<HashMap<_, _>>();
    let pair = arena_core::MatchPair {
        engine_a: game.white_version_id,
        engine_b: game.black_version_id,
        games: vec![game.clone()],
    };
    let update = build_pair_rating_update(&entries, &pair);

    if game.white_version_id == human_player.id {
        insert_human_rating_snapshot(db, &snapshot_from_entry(Some(pool_id), &update.engine_a))
            .await?;
        insert_rating_snapshot(db, &snapshot_from_entry(Some(pool_id), &update.engine_b)).await?;
    } else {
        insert_rating_snapshot(db, &snapshot_from_entry(Some(pool_id), &update.engine_a)).await?;
        insert_human_rating_snapshot(db, &snapshot_from_entry(Some(pool_id), &update.engine_b))
            .await?;
    }
    Ok(())
}
