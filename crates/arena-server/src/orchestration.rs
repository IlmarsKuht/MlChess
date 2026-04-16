use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Result, anyhow, bail};
use arena_core::{
    AgentVersion, EventPreset, EventPresetSelectionMode, GameLogEntry, GameRecord, GameResult,
    LeaderboardEntry, LiveRuntimeCheckpoint, MatchSeries, MatchStatus, RoundRobinScheduler,
    ScheduledPair, StabilityConfig, StabilityTracker, Tournament, TournamentKind, TournamentStatus,
    snapshot_from_entry,
};
use arena_runner::{
    AgentAdapter, build_adapter, calculate_move_budget, classify_position, classify_terminal_board,
    pgn_from_moves,
};
use chrono::Utc;
use serde_json::json;
use sqlx::{Sqlite, SqlitePool, Transaction};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    ApiError,
    gameplay::{
        MatchConfig, ensure_engine_supports_variant, fen_for_variant, parse_saved_board,
        resolve_start_state,
    },
    live::{
        clock_sync_from_checkpoint, game_finished_from_checkpoint, live_result_from_game_result,
        live_status_from_match_status, live_termination_from_game_termination,
        move_committed_from_checkpoint, publish_transient_with_metrics, publish_with_metrics,
        side_from_fen, snapshot_from_checkpoint,
    },
    presentation::HumanPlayerProfile,
    rating::build_pair_rating_update,
    state::{
        AppState, CompletedGameTable, EngineSeatController, HumanGameCommand, HumanGameHandle,
        HumanMoveAck, HumanPlayer, HumanSeatController, MatchRuntime, MatchSeatController,
        MatchSession, MoveDebugContext,
    },
    storage::{
        ensure_human_player, ensure_leaderboard_seed, get_agent_version, get_match_series,
        get_pool, get_tournament, insert_game_tx, insert_human_game_tx,
        insert_human_rating_snapshot, insert_live_runtime_event_tx, insert_match_series,
        insert_match_series_tx, insert_rating_snapshot, insert_tournament, insert_tournament_tx,
        list_agent_versions, list_agent_versions_by_ids, load_human_profile, load_pool_leaderboard,
        load_pool_openings, update_match_series_status, update_match_series_status_tx,
        update_tournament_status, update_tournament_status_tx, upsert_live_runtime_checkpoint_tx,
    },
};

fn push_runtime_log(logs: &mut Vec<GameLogEntry>, entry: GameLogEntry) {
    logs.push(entry);
}

fn runtime_log(
    runtime: &MatchRuntime,
    source: &str,
    event: &str,
    message: impl Into<String>,
) -> GameLogEntry {
    GameLogEntry::new(event, "info", source, message.into())
        .with_tournament_id(runtime.tournament_id)
        .with_seq(runtime.seq)
        .with_clocks(runtime.white_time_left_ms, runtime.black_time_left_ms)
}

fn match_runtime_log(
    session: &MatchSession,
    runtime: &MatchRuntime,
    source: &str,
    event: &str,
    message: impl Into<String>,
) -> GameLogEntry {
    runtime_log(runtime, source, event, message).with_match_id(session.match_series.id)
}

fn human_runtime_log(
    session: &MatchSession,
    runtime: &MatchRuntime,
    event: &str,
    message: impl Into<String>,
) -> GameLogEntry {
    match_runtime_log(session, runtime, "server.human_runtime", event, message)
}

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

async fn run_match_owner(
    state: AppState,
    session: MatchSession,
    runtime: MatchRuntime,
    publish_initial_snapshot: bool,
) {
    let _ = run_match_to_completion(&state, session, runtime, publish_initial_snapshot).await;
}

async fn run_match_to_completion(
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

async fn process_active_turn(
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

pub(crate) async fn run_tournament(
    state: AppState,
    tournament_id: Uuid,
    stop_flag: Arc<AtomicBool>,
) -> Result<()> {
    let tournament = get_tournament(&state.db, tournament_id).await?;
    let pool = get_pool(&state.db, tournament.pool_id).await?;
    let participants =
        list_agent_versions_by_ids(&state.db, &tournament.participant_version_ids).await?;
    if participants.len() < 2 {
        bail!("tournament requires at least two participant versions");
    }

    let participant_map: Arc<HashMap<Uuid, AgentVersion>> = Arc::new(
        participants
            .into_iter()
            .map(|version| (version.id, version))
            .collect(),
    );
    let rating_ids: Vec<_> = participant_map.keys().copied().collect();
    let openings = load_pool_openings(&state.db, &pool).await?;
    let ratings = Arc::new(tokio::sync::Mutex::new(
        ensure_leaderboard_seed(&state.db, pool.id, &rating_ids).await?,
    ));

    update_tournament_status(
        &state.db,
        tournament.id,
        TournamentStatus::Running,
        Some(Utc::now()),
        None,
    )
    .await?;

    let mut had_error = false;
    let mut pair_index = 0_u32;
    let mut scheduler = build_scheduler(&tournament);
    let mut stability = StabilityTracker::new(StabilityConfig::default());

    loop {
        if stop_flag.load(Ordering::SeqCst) {
            break;
        }

        if let Some(reason) = stability.should_stop(&tournament.participant_version_ids) {
            info!(
                "stopping tournament {tournament_id}: stable after {} pairs (min_pairs={}, ranking_stable={}, max_delta_ok={})",
                stability.total_pairs(),
                reason.min_pairs_reached,
                reason.ranking_stable,
                reason.max_rating_delta_below_threshold
            );
            break;
        }

        let Some(scheduled_pair) = scheduler.next_pair() else {
            break;
        };

        let white = participant_map
            .get(&scheduled_pair.engine_a)
            .cloned()
            .ok_or_else(|| anyhow!("missing first participant"))?;
        let black = participant_map
            .get(&scheduled_pair.engine_b)
            .cloned()
            .ok_or_else(|| anyhow!("missing second participant"))?;
        let opening = if openings.is_empty() {
            None
        } else {
            Some(openings[pair_index as usize % openings.len()].clone())
        };

        match play_engine_match_pair(
            &state,
            tournament_id,
            &pool,
            &white,
            &black,
            opening,
            pair_index,
            pool.fairness.paired_games && pool.fairness.swap_colors,
        )
        .await
        {
            Ok(pair) => {
                apply_pool_rating_update(&state.db, pool.id, &pair, &ratings).await?;
                {
                    let ratings_guard = ratings.lock().await;
                    stability.observe_pair(&pair, &ratings_guard);
                }
                pair_index += 1;
            }
            Err(err) => {
                had_error = true;
                warn!("match pair failed in tournament {tournament_id}: {err:#}");
            }
        }
    }

    let status = if stop_flag.load(Ordering::SeqCst) {
        TournamentStatus::Stopped
    } else if had_error {
        TournamentStatus::Failed
    } else {
        TournamentStatus::Completed
    };
    update_tournament_status(
        &state.db,
        tournament.id,
        status,
        tournament.started_at,
        Some(Utc::now()),
    )
    .await?;
    Ok(())
}

fn build_scheduler(tournament: &Tournament) -> RoundRobinScheduler {
    match tournament.kind {
        TournamentKind::RoundRobin => RoundRobinScheduler::new(
            &tournament.participant_version_ids,
            tournament.games_per_pairing,
        ),
        TournamentKind::Ladder => RoundRobinScheduler::from_pairings(
            tournament
                .participant_version_ids
                .windows(2)
                .flat_map(|window| {
                    (0..tournament.games_per_pairing.max(1)).map(move |_| ScheduledPair {
                        engine_a: window[0],
                        engine_b: window[1],
                    })
                })
                .collect(),
        ),
    }
}

pub(crate) async fn resolve_preset_participants(
    db: &SqlitePool,
    preset: &EventPreset,
) -> Result<Vec<Uuid>, ApiError> {
    let pool = get_pool(db, preset.pool_id).await?;
    let versions = list_agent_versions(db, None).await?;
    let participants = match preset.selection_mode {
        EventPresetSelectionMode::AllActiveEngines => versions
            .into_iter()
            .filter(|version| version.active)
            .filter(|version| !version.tags.iter().any(|tag| tag == "hidden"))
            .filter(|version| version.capabilities.supports_variant(pool.variant))
            .map(|version| version.id)
            .collect(),
    };
    Ok(participants)
}

pub(crate) async fn create_tournament_run(
    db: &SqlitePool,
    name: String,
    kind: TournamentKind,
    pool_id: Uuid,
    participant_version_ids: Vec<Uuid>,
    worker_count: u16,
    games_per_pairing: u16,
) -> Result<Tournament, ApiError> {
    if participant_version_ids.len() < 2 {
        return Err(ApiError::BadRequest(
            "pick at least two engine versions before creating a tournament".to_string(),
        ));
    }

    let pool = get_pool(db, pool_id).await?;
    let versions = list_agent_versions_by_ids(db, &participant_version_ids).await?;
    if versions.len() != participant_version_ids.len() {
        return Err(ApiError::BadRequest(
            "one or more selected engine versions no longer exist".to_string(),
        ));
    }
    for version in &versions {
        ensure_engine_supports_variant(version, pool.variant)?;
    }

    let tournament = Tournament {
        id: Uuid::new_v4(),
        name,
        kind,
        pool_id,
        participant_version_ids,
        worker_count: worker_count.max(1),
        games_per_pairing: games_per_pairing.max(1),
        status: TournamentStatus::Draft,
        created_at: Utc::now(),
        started_at: None,
        completed_at: None,
    };
    insert_tournament(db, &tournament)
        .await
        .map_err(ApiError::Internal)?;
    info!(
        tournament_id = %tournament.id,
        pool_id = %tournament.pool_id,
        participant_count = tournament.participant_version_ids.len(),
        "tournament created"
    );
    Ok(tournament)
}

async fn persist_terminal_match_state(
    state: &AppState,
    completed_game_table: CompletedGameTable,
    game: &GameRecord,
    match_status: MatchStatus,
    tournament_status: TournamentStatus,
    tournament_started_at: chrono::DateTime<Utc>,
    checkpoint: LiveRuntimeCheckpoint,
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
    publish_transient_with_metrics(
        &state.live_matches,
        Some(&state.live_metrics),
        checkpoint,
        event,
    )
    .await;
    Ok(())
}

async fn apply_pool_rating_update(
    db: &SqlitePool,
    pool_id: Uuid,
    pair: &arena_core::MatchPair,
    ratings: &tokio::sync::Mutex<HashMap<Uuid, LeaderboardEntry>>,
) -> Result<()> {
    let (engine_a_snapshot, engine_b_snapshot) = {
        let mut ratings = ratings.lock().await;
        let update = build_pair_rating_update(&ratings, pair);

        ratings.insert(pair.engine_a, update.engine_a.clone());
        ratings.insert(pair.engine_b, update.engine_b.clone());

        (
            snapshot_from_entry(Some(pool_id), &update.engine_a),
            snapshot_from_entry(Some(pool_id), &update.engine_b),
        )
    };

    insert_rating_snapshot(db, &engine_a_snapshot).await?;
    insert_rating_snapshot(db, &engine_b_snapshot).await?;
    Ok(())
}

async fn apply_human_pool_rating_update(
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

async fn play_engine_match_pair(
    state: &AppState,
    tournament_id: Uuid,
    pool: &arena_core::BenchmarkPool,
    engine_a: &AgentVersion,
    engine_b: &AgentVersion,
    opening: Option<arena_core::OpeningPosition>,
    pair_index: u32,
    swap_colors: bool,
) -> Result<arena_core::MatchPair> {
    let first_series = MatchSeries {
        id: Uuid::new_v4(),
        tournament_id,
        pool_id: pool.id,
        round_index: pair_index,
        white_version_id: engine_a.id,
        black_version_id: engine_b.id,
        opening_id: opening.as_ref().map(|value| value.id),
        game_index: pair_index.saturating_mul(2),
        status: MatchStatus::Running,
        created_at: Utc::now(),
    };
    insert_match_series(&state.db, &first_series).await?;

    let second_series = if swap_colors {
        let series = MatchSeries {
            id: Uuid::new_v4(),
            tournament_id,
            pool_id: pool.id,
            round_index: pair_index,
            white_version_id: engine_b.id,
            black_version_id: engine_a.id,
            opening_id: opening.as_ref().map(|value| value.id),
            game_index: pair_index.saturating_mul(2).saturating_add(1),
            status: MatchStatus::Pending,
            created_at: Utc::now(),
        };
        insert_match_series(&state.db, &series).await?;
        Some(series)
    } else {
        None
    };

    let first_game = play_server_owned_engine_game(
        state,
        MatchSession {
            name: format!("{} vs {}", engine_a.version, engine_b.version),
            match_series: first_series.clone(),
            completed_game_table: CompletedGameTable::Engine,
        },
        build_engine_runtime(
            tournament_id,
            pool,
            engine_a.clone(),
            engine_b.clone(),
            opening.clone(),
            300,
            pool.fairness.opening_seed.or(Some(pair_index as u64)),
        )
        .await?,
        true,
    )
    .await?;

    let mut games = vec![first_game];
    if let Some(second_series) = second_series {
        update_match_series_status(&state.db, second_series.id, MatchStatus::Running).await?;
        let second_game = play_server_owned_engine_game(
            state,
            MatchSession {
                name: format!("{} vs {}", engine_b.version, engine_a.version),
                match_series: second_series,
                completed_game_table: CompletedGameTable::Engine,
            },
            build_engine_runtime(
                tournament_id,
                pool,
                engine_b.clone(),
                engine_a.clone(),
                opening,
                300,
                pool.fairness.opening_seed.or(Some(pair_index as u64)),
            )
            .await?,
            true,
        )
        .await?;
        games.push(second_game);
    }

    Ok(arena_core::MatchPair {
        engine_a: engine_a.id,
        engine_b: engine_b.id,
        games,
    })
}

const CLOCK_SYNC_INTERVAL_MS: u64 = 1_000;

async fn build_engine_runtime(
    tournament_id: Uuid,
    pool: &arena_core::BenchmarkPool,
    white: AgentVersion,
    black: AgentVersion,
    opening: Option<arena_core::OpeningPosition>,
    max_plies: u16,
    opening_seed: Option<u64>,
) -> Result<MatchRuntime, ApiError> {
    ensure_engine_supports_variant(&white, pool.variant)?;
    ensure_engine_supports_variant(&black, pool.variant)?;
    let (board, start_fen) = resolve_start_state(MatchConfig {
        variant: pool.variant,
        opening: opening.as_ref(),
        opening_seed,
    })?;
    let initial_hash = board.hash_without_ep();
    let started_at = Utc::now();
    let mut logs = Vec::new();
    let mut white_engine = build_adapter(white);
    let mut black_engine = build_adapter(black);
    white_engine.prepare(pool.variant, &mut logs).await?;
    black_engine.prepare(pool.variant, &mut logs).await?;
    white_engine.begin_game(&mut logs).await?;
    black_engine.begin_game(&mut logs).await?;
    Ok(MatchRuntime {
        tournament_id,
        variant: pool.variant,
        time_control: pool.time_control.clone(),
        start_fen: start_fen.clone(),
        current_fen: start_fen,
        board,
        repetitions: HashMap::from([(initial_hash, 1)]),
        move_history: Vec::new(),
        white_time_left_ms: pool.time_control.initial_ms,
        black_time_left_ms: pool.time_control.initial_ms,
        max_plies,
        white_seat: MatchSeatController::Engine(EngineSeatController {
            adapter: Some(white_engine),
        }),
        black_seat: MatchSeatController::Engine(EngineSeatController {
            adapter: Some(black_engine),
        }),
        logs,
        started_at,
        turn_started_server_unix_ms: started_at.timestamp_millis(),
        seq: 0,
        result: None,
        termination: None,
        status: MatchStatus::Running,
    })
}

async fn play_server_owned_engine_game(
    state: &AppState,
    session: MatchSession,
    runtime: MatchRuntime,
    publish_initial_snapshot: bool,
) -> Result<GameRecord, ApiError> {
    run_match_to_completion(state, session, runtime, publish_initial_snapshot).await
}

pub(crate) async fn restore_engine_game(
    state: &AppState,
    checkpoint: LiveRuntimeCheckpoint,
) -> Result<(), ApiError> {
    let match_series = get_match_series(&state.db, checkpoint.match_id).await?;
    let pool = get_pool(&state.db, match_series.pool_id).await?;
    let tournament = get_tournament(&state.db, match_series.tournament_id).await?;
    let white = get_agent_version(&state.db, match_series.white_version_id).await?;
    let black = get_agent_version(&state.db, match_series.black_version_id).await?;
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
    let mut white_engine = build_adapter(white);
    let mut black_engine = build_adapter(black);
    white_engine.prepare(pool.variant, &mut logs).await?;
    black_engine.prepare(pool.variant, &mut logs).await?;
    white_engine.begin_game(&mut logs).await?;
    black_engine.begin_game(&mut logs).await?;
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
        white_seat: MatchSeatController::Engine(EngineSeatController {
            adapter: Some(white_engine),
        }),
        black_seat: MatchSeatController::Engine(EngineSeatController {
            adapter: Some(black_engine),
        }),
        logs,
        started_at: tournament.started_at.unwrap_or(match_series.created_at),
        turn_started_server_unix_ms: checkpoint.turn_started_server_unix_ms,
        seq: checkpoint.seq,
        result: None,
        termination: None,
        status: MatchStatus::Running,
    };
    let session = MatchSession {
        name: tournament.name,
        match_series,
        completed_game_table: CompletedGameTable::Engine,
    };
    let state = state.clone();
    tokio::spawn(async move {
        let _ = run_match_to_completion(&state, session, runtime, false).await;
    });
    Ok(())
}

fn match_checkpoint(session: &MatchSession, runtime: &MatchRuntime) -> LiveRuntimeCheckpoint {
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

async fn finalize_match_game(
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
        pgn: pgn_from_moves(
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

fn match_runtime_source(session: &MatchSession) -> &'static str {
    match session.completed_game_table {
        CompletedGameTable::Engine => "server.engine_runtime",
        CompletedGameTable::Human => "server.human_runtime",
    }
}

async fn shutdown_match_seats(runtime: &mut MatchRuntime) {
    for seat in [&mut runtime.white_seat, &mut runtime.black_seat] {
        if let MatchSeatController::Engine(engine) = seat {
            if let Some(mut adapter) = engine.adapter.take() {
                adapter.shutdown(&mut runtime.logs).await.ok();
                engine.adapter = Some(adapter);
            }
        }
    }
}

fn elapsed_since_turn_start_ms(runtime: &MatchRuntime) -> u64 {
    Utc::now()
        .timestamp_millis()
        .saturating_sub(runtime.turn_started_server_unix_ms) as u64
}

fn remaining_turn_time_ms(runtime: &MatchRuntime) -> u64 {
    let remaining = if runtime.board.side_to_move() == cozy_chess::Color::White {
        runtime.white_time_left_ms
    } else {
        runtime.black_time_left_ms
    };
    remaining.saturating_sub(elapsed_since_turn_start_ms(runtime))
}

async fn emit_match_clock_sync(
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

async fn finalize_timeout(
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

async fn publish_match_runtime(
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

async fn process_human_move(
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

async fn process_engine_turn(
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

fn update_terminal_state(runtime: &mut MatchRuntime) {
    if let Some((result, termination)) = classify_position(&runtime.board, &runtime.repetitions) {
        runtime.result = Some(result);
        runtime.termination = Some(termination);
        runtime.status = MatchStatus::Completed;
    } else if runtime.board.status() != cozy_chess::GameStatus::Ongoing {
        let (result, termination) = classify_terminal_board(&runtime.board);
        runtime.result = Some(result);
        runtime.termination = Some(termination);
        runtime.status = MatchStatus::Completed;
    }
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

async fn process_human_turn(
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

#[cfg(test)]
mod tests {
    use super::*;
    use arena_core::{GameLogEntry, TimeControl, Variant};
    use arena_runner::AgentAdapter;
    use async_trait::async_trait;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::time::Duration;

    use crate::{
        db::init_db,
        registry::{SetupRegistryCache, sync_setup_registry_if_changed},
        state::{HumanGameStore, TournamentCoordinator},
    };

    struct SleepyAdapter {
        delay_ms: u64,
        move_uci: String,
    }

    #[async_trait]
    impl AgentAdapter for SleepyAdapter {
        async fn prepare(
            &mut self,
            _variant: Variant,
            _logs: &mut Vec<GameLogEntry>,
        ) -> Result<()> {
            Ok(())
        }

        async fn begin_game(&mut self, _logs: &mut Vec<GameLogEntry>) -> Result<()> {
            Ok(())
        }

        async fn choose_move(
            &mut self,
            _board: &cozy_chess::Board,
            _start_fen: &str,
            _moves: &[String],
            _movetime_ms: u64,
            _logs: &mut Vec<GameLogEntry>,
        ) -> Result<String> {
            tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
            Ok(self.move_uci.clone())
        }

        async fn shutdown(&mut self, _logs: &mut Vec<GameLogEntry>) -> Result<()> {
            Ok(())
        }
    }

    async fn test_state() -> AppState {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        init_db(&db).await.unwrap();
        let setup_registry = SetupRegistryCache::default();
        sync_setup_registry_if_changed(&db, &setup_registry)
            .await
            .unwrap();
        AppState {
            db,
            coordinator: TournamentCoordinator::default(),
            live_matches: crate::live::LiveMatchStore::default(),
            live_metrics: crate::state::LiveMetricsStore::default(),
            human_games: HumanGameStore::default(),
            debug_reports_dir: std::env::temp_dir().join("mlchess-debug-reports"),
            frontend_dist: None,
            setup_registry,
        }
    }

    async fn session_and_runtime(
        state: &AppState,
        _engine_side: cozy_chess::Color,
        human_plays_white: bool,
    ) -> (MatchSession, MatchRuntime) {
        let pool = crate::storage::list_pools(&state.db)
            .await
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let engine_version = list_agent_versions(&state.db, None)
            .await
            .unwrap()
            .into_iter()
            .find(|version| version.id != Uuid::from_u128(1))
            .unwrap();
        let human_player = ensure_human_player(&state.db).await.unwrap();
        let tournament_id = Uuid::new_v4();
        insert_tournament(
            &state.db,
            &Tournament {
                id: tournament_id,
                name: "test".to_string(),
                kind: TournamentKind::RoundRobin,
                pool_id: pool.id,
                participant_version_ids: vec![human_player.id, engine_version.id],
                worker_count: 1,
                games_per_pairing: 1,
                status: TournamentStatus::Running,
                created_at: Utc::now(),
                started_at: Some(Utc::now()),
                completed_at: None,
            },
        )
        .await
        .unwrap();
        let match_series = MatchSeries {
            id: Uuid::new_v4(),
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
            opening_id: None,
            game_index: 0,
            status: MatchStatus::Running,
            created_at: Utc::now(),
        };
        let (command_tx, command_rx) = tokio::sync::mpsc::channel(1);
        let session = MatchSession {
            name: "test".to_string(),
            match_series: match_series.clone(),
            completed_game_table: CompletedGameTable::Human,
        };
        let board = cozy_chess::Board::default();
        let human_seat = MatchSeatController::Human(HumanSeatController {
            player: human_player,
            command_rx,
            seen_intents: HashMap::new(),
        });
        let engine_seat = MatchSeatController::Engine(EngineSeatController {
            adapter: Some(Box::new(SleepyAdapter {
                delay_ms: 0,
                move_uci: "e2e4".to_string(),
            })),
        });
        let (white_seat, black_seat) = if human_plays_white {
            (human_seat, engine_seat)
        } else {
            (engine_seat, human_seat)
        };
        let runtime = MatchRuntime {
            tournament_id: match_series.tournament_id,
            variant: Variant::Standard,
            time_control: TimeControl {
                initial_ms: 50,
                increment_ms: 0,
            },
            start_fen: board.to_string(),
            current_fen: board.to_string(),
            board,
            repetitions: HashMap::from([(cozy_chess::Board::default().hash_without_ep(), 1)]),
            move_history: Vec::new(),
            white_time_left_ms: 50,
            black_time_left_ms: 50,
            max_plies: 300,
            white_seat,
            black_seat,
            logs: Vec::new(),
            started_at: Utc::now(),
            turn_started_server_unix_ms: Utc::now().timestamp_millis(),
            seq: 0,
            result: None,
            termination: None,
            status: MatchStatus::Running,
        };
        let handle = HumanGameHandle { command_tx };
        state.human_games.insert(match_series.id, handle).await;
        (session, runtime)
    }

    #[tokio::test]
    async fn human_move_times_out_when_elapsed_exceeds_remaining_clock() {
        let state = test_state().await;
        let (session, mut runtime) =
            session_and_runtime(&state, cozy_chess::Color::Black, true).await;
        runtime.turn_started_server_unix_ms -= 75;
        runtime.white_time_left_ms = 25;

        let ack = process_human_move(
            &state,
            &session,
            &mut runtime,
            cozy_chess::Color::White,
            MoveDebugContext {
                request_id: None,
                client_action_id: None,
                ws_connection_id: None,
                intent_id: Uuid::new_v4(),
                move_uci: "e2e4".to_string(),
            },
        )
        .await;

        assert!(matches!(ack, HumanMoveAck::RejectedGameFinished));
        assert_eq!(runtime.status, MatchStatus::Completed);
        assert_eq!(
            runtime.termination,
            Some(arena_core::GameTermination::Timeout)
        );
        finalize_match_game(&state, session.clone(), runtime)
            .await
            .unwrap();
        let snapshot = state
            .live_matches
            .get_snapshot(session.match_series.id)
            .await
            .unwrap();
        assert_eq!(snapshot.status, arena_core::LiveStatus::Finished);
        assert_eq!(snapshot.termination, arena_core::LiveTermination::Timeout);
    }

    #[tokio::test]
    async fn engine_turn_times_out_when_adapter_responds_too_late() {
        let state = test_state().await;
        let (session, mut runtime) =
            session_and_runtime(&state, cozy_chess::Color::White, false).await;
        runtime.white_time_left_ms = 20;
        runtime.white_seat = MatchSeatController::Engine(EngineSeatController {
            adapter: Some(Box::new(SleepyAdapter {
                delay_ms: 40,
                move_uci: "e2e4".to_string(),
            })),
        });

        process_engine_turn(&state, &session, &mut runtime, cozy_chess::Color::White)
            .await
            .unwrap();

        assert_eq!(runtime.status, MatchStatus::Completed);
        assert_eq!(
            runtime.termination,
            Some(arena_core::GameTermination::Timeout)
        );
        finalize_match_game(&state, session.clone(), runtime)
            .await
            .unwrap();
        let snapshot = state
            .live_matches
            .get_snapshot(session.match_series.id)
            .await
            .unwrap();
        assert_eq!(snapshot.status, arena_core::LiveStatus::Finished);
        assert_eq!(snapshot.termination, arena_core::LiveTermination::Timeout);
    }

    #[tokio::test]
    async fn human_owner_times_out_without_submitted_move() {
        let state = test_state().await;
        let (session, mut runtime) =
            session_and_runtime(&state, cozy_chess::Color::Black, true).await;
        runtime.white_time_left_ms = 10;
        runtime.turn_started_server_unix_ms -= 20;

        tokio::spawn(run_match_owner(
            state.clone(),
            session.clone(),
            runtime,
            true,
        ));

        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        let snapshot = loop {
            if let Some(snapshot) = state
                .live_matches
                .get_snapshot(session.match_series.id)
                .await
                && snapshot.status == arena_core::LiveStatus::Finished
            {
                break snapshot;
            }
            assert!(tokio::time::Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(10)).await;
        };
        assert_eq!(snapshot.status, arena_core::LiveStatus::Finished);
        assert_eq!(snapshot.termination, arena_core::LiveTermination::Timeout);
        assert_eq!(snapshot.white_remaining_ms, 0);
    }

    #[tokio::test]
    async fn create_human_game_persists_match_series_with_human_participant() {
        let state = test_state().await;
        let pool = crate::storage::list_pools(&state.db)
            .await
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let engine_version = list_agent_versions(&state.db, None)
            .await
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let human_player = ensure_human_player(&state.db).await.unwrap();

        let (match_id, _tournament_id) = create_human_game(
            &state,
            "test human game".to_string(),
            pool.id,
            engine_version.id,
            true,
        )
        .await
        .unwrap();

        let series = crate::storage::get_match_series(&state.db, match_id)
            .await
            .unwrap();
        assert_eq!(series.white_version_id, human_player.id);
        assert_eq!(series.black_version_id, engine_version.id);
    }
}
