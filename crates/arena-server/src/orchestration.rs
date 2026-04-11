use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use anyhow::{Result, anyhow, bail};
use arena_core::{
    AgentVersion, EventPreset, EventPresetSelectionMode, GameRecord, GameResult, LeaderboardEntry,
    LiveRuntimeCheckpoint, MatchSeries, MatchStatus, RoundRobinScheduler, ScheduledPair,
    StabilityConfig, StabilityTracker, Tournament, TournamentKind, TournamentStatus, Variant,
    snapshot_from_entry,
};
use arena_runner::{
    AgentAdapter, build_adapter, calculate_move_budget, classify_position, classify_terminal_board,
    pgn_from_moves,
};
use chrono::Utc;
use sqlx::SqlitePool;
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    ApiError,
    gameplay::starting_board_for_human_game,
    live::{
        clock_sync_from_checkpoint, game_finished_from_checkpoint, live_result_from_game_result,
        live_status_from_match_status, live_termination_from_game_termination,
        move_committed_from_checkpoint, publish_transient_with_metrics, publish_with_metrics,
        side_from_fen, snapshot_from_checkpoint,
    },
    presentation::HumanPlayerProfile,
    rating::build_pair_rating_update,
    state::{AppState, HumanGameCommand, HumanGameRuntime, HumanGameSession, HumanMoveAck, HumanPlayer},
    storage::{
        ensure_agent_version_exists, ensure_human_player, ensure_leaderboard_seed, ensure_pool_exists,
        get_agent_version, get_match_series, get_pool, get_tournament, insert_game, insert_human_game,
        insert_human_rating_snapshot, insert_match_series, insert_rating_snapshot, insert_tournament,
        list_agent_versions, list_agent_versions_by_ids, load_human_profile, load_pool_leaderboard,
        load_pool_openings, update_match_series_status, update_tournament_status,
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
    if pool.variant != Variant::Standard {
        return Err(ApiError::BadRequest(
            "human play currently supports standard pools only".to_string(),
        ));
    }

    let engine_version = get_agent_version(&state.db, engine_version_id).await?;
    let human_player = ensure_human_player(&state.db).await?;
    let openings = load_pool_openings(&state.db, &pool).await?;
    let opening = openings.first().cloned();
    let board = starting_board_for_human_game(pool.variant, opening.as_ref(), None)?;
    let start_fen = format!("{board}");
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
    insert_tournament(&state.db, &tournament).await?;
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
    insert_match_series(&state.db, &match_series).await?;
    let mut logs = Vec::new();
    let mut engine = build_adapter(engine_version);
    engine.prepare(pool.variant, &mut logs).await?;
    engine.begin_game(&mut logs).await?;
    let initial_hash = board.hash_without_ep();
    let runtime = HumanGameRuntime {
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
        engine_side: if human_plays_white {
            cozy_chess::Color::Black
        } else {
            cozy_chess::Color::White
        },
        engine,
        logs,
        started_at: created_at,
        turn_started_server_unix_ms: created_at.timestamp_millis(),
        seq: 0,
        seen_intents: HashMap::new(),
        result: None,
        termination: None,
        status: MatchStatus::Running,
    };
    let (command_tx, command_rx) = tokio::sync::mpsc::channel(32);
    let session = HumanGameSession {
        name,
        match_series: match_series.clone(),
        human_player,
        command_tx,
    };
    state.human_games.insert(session.clone()).await;
    tokio::spawn(run_human_game_owner(
        state.clone(),
        session,
        runtime,
        command_rx,
        true,
    ));

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
    intent_id: Uuid,
    uci: String,
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
            intent_id,
            move_uci: uci,
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
    let openings = load_pool_openings(&state.db, &pool).await?;
    let opening = openings
        .iter()
        .find(|candidate| Some(candidate.id) == match_series.opening_id)
        .cloned();
    let mut board = starting_board_for_human_game(pool.variant, opening.as_ref(), None)?;
    let start_fen = format!("{board}");
    let mut repetitions = HashMap::from([(board.hash_without_ep(), 1_u8)]);
    for uci in &checkpoint.moves {
        let mv = cozy_chess::util::parse_uci_move(&board, uci)
            .map_err(|err| ApiError::Conflict(format!("failed to restore move {uci}: {err}")))?;
        board.try_play(mv)
            .map_err(|_| ApiError::Conflict(format!("failed to replay restored move {uci}")))?;
        *repetitions.entry(board.hash_without_ep()).or_insert(0) += 1;
    }

    let mut logs = Vec::new();
    let mut engine = build_adapter(engine_version);
    engine.prepare(pool.variant, &mut logs).await?;
    engine.begin_game(&mut logs).await?;

    let runtime = HumanGameRuntime {
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
        engine_side: if match_series.white_version_id == human_player.id {
            cozy_chess::Color::Black
        } else {
            cozy_chess::Color::White
        },
        engine,
        logs,
        started_at: tournament.started_at.unwrap_or(match_series.created_at),
        turn_started_server_unix_ms: checkpoint.turn_started_server_unix_ms,
        seq: checkpoint.seq,
        seen_intents: HashMap::new(),
        result: match checkpoint.result {
            arena_core::LiveResult::WhiteWin => Some(GameResult::WhiteWin),
            arena_core::LiveResult::BlackWin => Some(GameResult::BlackWin),
            arena_core::LiveResult::Draw => Some(GameResult::Draw),
            arena_core::LiveResult::None => None,
        },
        termination: match checkpoint.termination {
            arena_core::LiveTermination::Checkmate => Some(arena_core::GameTermination::Checkmate),
            arena_core::LiveTermination::Timeout => Some(arena_core::GameTermination::Timeout),
            arena_core::LiveTermination::Resignation => Some(arena_core::GameTermination::Resignation),
            arena_core::LiveTermination::Abort => Some(arena_core::GameTermination::Unknown),
            arena_core::LiveTermination::Stalemate => Some(arena_core::GameTermination::Stalemate),
            arena_core::LiveTermination::Repetition => Some(arena_core::GameTermination::Repetition),
            arena_core::LiveTermination::InsufficientMaterial => Some(arena_core::GameTermination::InsufficientMaterial),
            arena_core::LiveTermination::FiftyMoveRule => Some(arena_core::GameTermination::FiftyMoveRule),
            arena_core::LiveTermination::IllegalMove => Some(arena_core::GameTermination::IllegalMove),
            arena_core::LiveTermination::MoveLimit => Some(arena_core::GameTermination::MoveLimit),
            arena_core::LiveTermination::EngineFailure => Some(arena_core::GameTermination::EngineFailure),
            arena_core::LiveTermination::None => None,
        },
        status: match checkpoint.status {
            arena_core::LiveStatus::Running => MatchStatus::Running,
            arena_core::LiveStatus::Finished => MatchStatus::Completed,
            arena_core::LiveStatus::Aborted => MatchStatus::Failed,
        },
    };
    let (command_tx, command_rx) = tokio::sync::mpsc::channel(32);
    let session = HumanGameSession {
        name: tournament.name,
        match_series,
        human_player,
        command_tx,
    };
    state.human_games.insert(session.clone()).await;
    tokio::spawn(run_human_game_owner(
        state.clone(),
        session,
        runtime,
        command_rx,
        false,
    ));
    Ok(())
}

async fn run_human_game_owner(
    state: AppState,
    session: HumanGameSession,
    mut runtime: HumanGameRuntime,
    mut command_rx: tokio::sync::mpsc::Receiver<HumanGameCommand>,
    publish_initial_snapshot: bool,
) {
    if publish_initial_snapshot {
        let _ = publish_human_runtime(&state, &session, &mut runtime, true).await;
    }
    loop {
        if runtime.status != MatchStatus::Running {
            let _ = finalize_human_game(state.clone(), session.clone(), runtime).await;
            return;
        }
        if runtime.board.side_to_move() == runtime.engine_side {
            let _ = process_engine_turn(&state, &session, &mut runtime).await;
            continue;
        }
        let timeout_delay = remaining_turn_time_ms(&runtime);
        if timeout_delay == 0 {
            let _ = finalize_human_timeout(&state, &session, &mut runtime).await;
            continue;
        }
        let sleep = tokio::time::sleep(std::time::Duration::from_millis(timeout_delay));
        tokio::pin!(sleep);
        let sync = tokio::time::sleep(std::time::Duration::from_millis(CLOCK_SYNC_INTERVAL_MS.min(timeout_delay)));
        tokio::pin!(sync);
        let mut timed_out = false;
        let command_opt = loop {
            let maybe_command = tokio::select! {
                maybe_command = command_rx.recv() => maybe_command,
                _ = &mut sleep => {
                    state
                        .live_metrics
                        .timeout_fires
                        .fetch_add(1, Ordering::Relaxed);
                    let _ = finalize_human_timeout(&state, &session, &mut runtime).await;
                    timed_out = true;
                    break None;
                }
                _ = &mut sync => {
                    let _ = emit_human_clock_sync(&state, &session, &mut runtime).await;
                    let next_delay = CLOCK_SYNC_INTERVAL_MS.min(remaining_turn_time_ms(&runtime).max(1));
                    sync.as_mut().reset(tokio::time::Instant::now() + std::time::Duration::from_millis(next_delay));
                    continue;
                }
            };
            break maybe_command;
        };
        if timed_out {
            continue;
        }
        let Some(command) = command_opt else {
            return;
        };
        match command {
            HumanGameCommand::SubmitMove {
                intent_id,
                move_uci,
                respond_to,
            } => {
                let ack = process_human_move(&state, &session, &mut runtime, intent_id, move_uci).await;
                let _ = respond_to.send(ack);
            }
        }
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
        TournamentKind::RoundRobin => {
            RoundRobinScheduler::new(&tournament.participant_version_ids, tournament.games_per_pairing)
        }
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
    let versions = list_agent_versions(db, None).await?;
    let participants = match preset.selection_mode {
        EventPresetSelectionMode::AllActiveEngines => versions
            .into_iter()
            .filter(|version| version.active)
            .filter(|version| !version.tags.iter().any(|tag| tag == "hidden"))
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

    ensure_pool_exists(db, pool_id).await?;
    for participant in &participant_version_ids {
        ensure_agent_version_exists(db, *participant).await?;
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
    Ok(tournament)
}

async fn finalize_human_game(
    state: AppState,
    session: HumanGameSession,
    mut runtime: HumanGameRuntime,
) -> Result<(), ApiError> {
    state.human_games.remove(session.match_series.id).await;
    let mut shutdown_logs = std::mem::take(&mut runtime.logs);
    runtime.engine.shutdown(&mut shutdown_logs).await.ok();
    runtime.logs = shutdown_logs;
    let result = runtime.result.unwrap_or(GameResult::Draw);
    let termination = runtime.termination.unwrap_or(arena_core::GameTermination::Unknown);
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
        completed_at: Utc::now(),
    };
    insert_human_game(&state.db, &game).await?;
    apply_human_pool_rating_update(
        &state.db,
        session.match_series.pool_id,
        &game,
        &session.human_player,
    )
    .await?;
    update_match_series_status(&state.db, session.match_series.id, runtime.status).await?;
    update_tournament_status(
        &state.db,
        runtime.tournament_id,
        TournamentStatus::Completed,
        Some(runtime.started_at),
        Some(Utc::now()),
    )
    .await?;
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
        insert_human_rating_snapshot(db, &snapshot_from_entry(Some(pool_id), &update.engine_a)).await?;
        insert_rating_snapshot(db, &snapshot_from_entry(Some(pool_id), &update.engine_b)).await?;
    } else {
        insert_rating_snapshot(db, &snapshot_from_entry(Some(pool_id), &update.engine_a)).await?;
        insert_human_rating_snapshot(db, &snapshot_from_entry(Some(pool_id), &update.engine_b)).await?;
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
        EngineMatchSession {
            name: format!("{} vs {}", engine_a.version, engine_b.version),
            match_series: first_series.clone(),
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
            EngineMatchSession {
                name: format!("{} vs {}", engine_b.version, engine_a.version),
                match_series: second_series,
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

struct EngineMatchSession {
    name: String,
    match_series: MatchSeries,
}

struct EngineMatchRuntime {
    tournament_id: Uuid,
    variant: Variant,
    time_control: arena_core::TimeControl,
    start_fen: String,
    current_fen: String,
    board: cozy_chess::Board,
    repetitions: HashMap<u64, u8>,
    move_history: Vec<String>,
    white_time_left_ms: u64,
    black_time_left_ms: u64,
    max_plies: u16,
    white_engine: Box<dyn AgentAdapter>,
    black_engine: Box<dyn AgentAdapter>,
    logs: Vec<arena_core::GameLogEntry>,
    started_at: chrono::DateTime<Utc>,
    turn_started_server_unix_ms: i64,
    seq: u64,
    result: Option<GameResult>,
    termination: Option<arena_core::GameTermination>,
    status: MatchStatus,
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
) -> Result<EngineMatchRuntime, ApiError> {
    let board = arena_runner::starting_board(pool.variant, opening.as_ref(), opening_seed);
    let initial_hash = board.hash_without_ep();
    let started_at = Utc::now();
    let start_fen = board.to_string();
    let mut logs = Vec::new();
    let mut white_engine = build_adapter(white);
    let mut black_engine = build_adapter(black);
    white_engine.prepare(pool.variant, &mut logs).await?;
    black_engine.prepare(pool.variant, &mut logs).await?;
    white_engine.begin_game(&mut logs).await?;
    black_engine.begin_game(&mut logs).await?;
    Ok(EngineMatchRuntime {
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
        white_engine,
        black_engine,
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
    session: EngineMatchSession,
    mut runtime: EngineMatchRuntime,
    publish_initial_snapshot: bool,
) -> Result<GameRecord, ApiError> {
    let mut terminal_published = false;
    if publish_initial_snapshot {
        publish_engine_runtime(state, &session, &mut runtime, true).await?;
    }

    while runtime.status == MatchStatus::Running {
        if let Some((result, termination)) = classify_position(&runtime.board, &runtime.repetitions) {
            runtime.result = Some(result);
            runtime.termination = Some(termination);
            runtime.status = MatchStatus::Completed;
            break;
        }
        if runtime.move_history.len() as u16 >= runtime.max_plies {
            runtime.result = Some(GameResult::Draw);
            runtime.termination = Some(arena_core::GameTermination::MoveLimit);
            runtime.status = MatchStatus::Completed;
            break;
        }
        let side = runtime.board.side_to_move();
        let remaining = if side == cozy_chess::Color::White {
            runtime.white_time_left_ms
        } else {
            runtime.black_time_left_ms
        };
        let movetime_ms = calculate_move_budget(remaining, runtime.time_control.increment_ms);
        let mut logs = std::mem::take(&mut runtime.logs);
        let timeout = tokio::time::sleep(std::time::Duration::from_millis(remaining.max(1)));
        tokio::pin!(timeout);
        let sync = tokio::time::sleep(std::time::Duration::from_millis(CLOCK_SYNC_INTERVAL_MS.min(remaining.max(1))));
        tokio::pin!(sync);
        let sync_match_id = session.match_series.id;
        let sync_fen = runtime.current_fen.clone();
        let sync_moves = runtime.move_history.clone();
        let sync_side_to_move = side_from_fen(&runtime.current_fen);
        let selected = if side == cozy_chess::Color::White {
            let choose = runtime.white_engine.choose_move(
                &runtime.board,
                &runtime.start_fen,
                &runtime.move_history,
                movetime_ms,
                &mut logs,
            );
            tokio::pin!(choose);
            loop {
                tokio::select! {
                    result = &mut choose => break Ok(result),
                    _ = &mut timeout => break Err(()),
                    _ = &mut sync => {
                        emit_engine_clock_sync_during_turn(
                            state,
                            &mut runtime.seq,
                            &mut runtime.white_time_left_ms,
                            &mut runtime.black_time_left_ms,
                            &mut runtime.turn_started_server_unix_ms,
                            side,
                            sync_match_id,
                            &sync_fen,
                            &sync_moves,
                            sync_side_to_move,
                        ).await;
                        let next_remaining = if side == cozy_chess::Color::White {
                            runtime.white_time_left_ms
                        } else {
                            runtime.black_time_left_ms
                        }
                        .saturating_sub(
                            Utc::now()
                                .timestamp_millis()
                                .saturating_sub(runtime.turn_started_server_unix_ms) as u64,
                        );
                        let next_delay = CLOCK_SYNC_INTERVAL_MS.min(next_remaining.max(1));
                        sync.as_mut().reset(tokio::time::Instant::now() + std::time::Duration::from_millis(next_delay));
                    }
                }
            }
        } else {
            let choose = runtime.black_engine.choose_move(
                &runtime.board,
                &runtime.start_fen,
                &runtime.move_history,
                movetime_ms,
                &mut logs,
            );
            tokio::pin!(choose);
            loop {
                tokio::select! {
                    result = &mut choose => break Ok(result),
                    _ = &mut timeout => break Err(()),
                    _ = &mut sync => {
                        emit_engine_clock_sync_during_turn(
                            state,
                            &mut runtime.seq,
                            &mut runtime.white_time_left_ms,
                            &mut runtime.black_time_left_ms,
                            &mut runtime.turn_started_server_unix_ms,
                            side,
                            sync_match_id,
                            &sync_fen,
                            &sync_moves,
                            sync_side_to_move,
                        ).await;
                        let next_remaining = if side == cozy_chess::Color::White {
                            runtime.white_time_left_ms
                        } else {
                            runtime.black_time_left_ms
                        }
                        .saturating_sub(
                            Utc::now()
                                .timestamp_millis()
                                .saturating_sub(runtime.turn_started_server_unix_ms) as u64,
                        );
                        let next_delay = CLOCK_SYNC_INTERVAL_MS.min(next_remaining.max(1));
                        sync.as_mut().reset(tokio::time::Instant::now() + std::time::Duration::from_millis(next_delay));
                    }
                }
            }
        };
        runtime.logs = logs;
        let elapsed_ms = elapsed_since_turn_start_ms_engine(&runtime);
        let clock = if side == cozy_chess::Color::White {
            &mut runtime.white_time_left_ms
        } else {
            &mut runtime.black_time_left_ms
        };
        *clock = clock.saturating_sub(elapsed_ms);
        let selected = match selected {
            Ok(Ok(value)) if elapsed_ms < remaining => value,
            Ok(Ok(_)) | Err(_) => {
                if side == cozy_chess::Color::White {
                    runtime.white_time_left_ms = 0;
                    runtime.result = Some(GameResult::BlackWin);
                } else {
                    runtime.black_time_left_ms = 0;
                    runtime.result = Some(GameResult::WhiteWin);
                }
                runtime.termination = Some(arena_core::GameTermination::Timeout);
                runtime.status = MatchStatus::Completed;
                state.live_metrics.timeout_fires.fetch_add(1, Ordering::Relaxed);
                break;
            }
            Ok(Err(_)) => {
                runtime.result = Some(if side == cozy_chess::Color::White {
                    GameResult::BlackWin
                } else {
                    GameResult::WhiteWin
                });
                runtime.termination = Some(arena_core::GameTermination::EngineFailure);
                runtime.status = MatchStatus::Completed;
                break;
            }
        };

        if selected == "0000" {
            runtime.result = Some(GameResult::Draw);
            runtime.termination = Some(arena_core::GameTermination::EngineFailure);
            runtime.status = MatchStatus::Completed;
            break;
        }

        let mv = cozy_chess::util::parse_uci_move(&runtime.board, &selected)
            .map_err(|err| ApiError::Conflict(format!("engine returned invalid UCI move: {err}")))?;
        if runtime.board.try_play(mv).is_err() {
            runtime.result = Some(if side == cozy_chess::Color::White {
                GameResult::BlackWin
            } else {
                GameResult::WhiteWin
            });
            runtime.termination = Some(arena_core::GameTermination::IllegalMove);
            runtime.status = MatchStatus::Completed;
            break;
        }

        runtime.move_history.push(selected);
        runtime.current_fen = runtime.board.to_string();
        *runtime.repetitions.entry(runtime.board.hash_without_ep()).or_insert(0) += 1;
        if side == cozy_chess::Color::White {
            runtime.white_time_left_ms = runtime
                .white_time_left_ms
                .saturating_add(runtime.time_control.increment_ms);
        } else {
            runtime.black_time_left_ms = runtime
                .black_time_left_ms
                .saturating_add(runtime.time_control.increment_ms);
        }
        runtime.turn_started_server_unix_ms = Utc::now().timestamp_millis();
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
        publish_engine_runtime(state, &session, &mut runtime, false).await?;
        terminal_published = runtime.status != MatchStatus::Running;
    }

    if runtime.status != MatchStatus::Running && !terminal_published {
        publish_engine_runtime(state, &session, &mut runtime, false).await?;
    }
    finalize_engine_game(state, session, runtime).await
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
    let openings = load_pool_openings(&state.db, &pool).await?;
    let opening = openings
        .iter()
        .find(|candidate| Some(candidate.id) == match_series.opening_id)
        .cloned();
    let mut board = arena_runner::starting_board(pool.variant, opening.as_ref(), None);
    let start_fen = board.to_string();
    let mut repetitions = HashMap::from([(board.hash_without_ep(), 1_u8)]);
    for uci in &checkpoint.moves {
        let mv = cozy_chess::util::parse_uci_move(&board, uci)
            .map_err(|err| ApiError::Conflict(format!("failed to restore move {uci}: {err}")))?;
        board.try_play(mv)
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
    let runtime = EngineMatchRuntime {
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
        white_engine,
        black_engine,
        logs,
        started_at: tournament.started_at.unwrap_or(match_series.created_at),
        turn_started_server_unix_ms: checkpoint.turn_started_server_unix_ms,
        seq: checkpoint.seq,
        result: None,
        termination: None,
        status: MatchStatus::Running,
    };
    let session = EngineMatchSession {
        name: tournament.name,
        match_series,
    };
    let state = state.clone();
    tokio::spawn(async move {
        let _ = play_server_owned_engine_game(&state, session, runtime, false).await;
    });
    Ok(())
}

async fn publish_engine_runtime(
    state: &AppState,
    session: &EngineMatchSession,
    runtime: &mut EngineMatchRuntime,
    initial: bool,
) -> Result<(), ApiError> {
    runtime.seq += 1;
    let checkpoint = engine_checkpoint(session, runtime);
    let event = if initial {
        arena_core::LiveEventEnvelope::Snapshot(snapshot_from_checkpoint(&checkpoint))
    } else if runtime.status == MatchStatus::Running {
        arena_core::LiveEventEnvelope::MoveCommitted(move_committed_from_checkpoint(&checkpoint))
    } else {
        arena_core::LiveEventEnvelope::GameFinished(game_finished_from_checkpoint(&checkpoint))
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

fn engine_checkpoint(session: &EngineMatchSession, runtime: &EngineMatchRuntime) -> LiveRuntimeCheckpoint {
    let updated_at = Utc::now();
    LiveRuntimeCheckpoint {
        match_id: session.match_series.id,
        seq: runtime.seq,
        status: live_status_from_match_status(runtime.status),
        result: runtime.result.map(live_result_from_game_result).unwrap_or(arena_core::LiveResult::None),
        termination: runtime
            .termination
            .map(live_termination_from_game_termination)
            .unwrap_or(arena_core::LiveTermination::None),
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

async fn finalize_engine_game(
    state: &AppState,
    session: EngineMatchSession,
    mut runtime: EngineMatchRuntime,
) -> Result<GameRecord, ApiError> {
    runtime.white_engine.shutdown(&mut runtime.logs).await.ok();
    runtime.black_engine.shutdown(&mut runtime.logs).await.ok();
    let result = runtime.result.unwrap_or(GameResult::Draw);
    let termination = runtime.termination.unwrap_or(arena_core::GameTermination::Unknown);
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
        completed_at: Utc::now(),
    };
    insert_game(&state.db, &game).await?;
    update_match_series_status(&state.db, session.match_series.id, MatchStatus::Completed).await?;
    Ok(game)
}

fn elapsed_since_turn_start_ms(runtime: &HumanGameRuntime) -> u64 {
    Utc::now()
        .timestamp_millis()
        .saturating_sub(runtime.turn_started_server_unix_ms) as u64
}

fn remaining_turn_time_ms(runtime: &HumanGameRuntime) -> u64 {
    let remaining = if runtime.board.side_to_move() == cozy_chess::Color::White {
        runtime.white_time_left_ms
    } else {
        runtime.black_time_left_ms
    };
    remaining.saturating_sub(elapsed_since_turn_start_ms(runtime))
}

fn elapsed_since_turn_start_ms_engine(runtime: &EngineMatchRuntime) -> u64 {
    Utc::now()
        .timestamp_millis()
        .saturating_sub(runtime.turn_started_server_unix_ms) as u64
}

async fn emit_engine_clock_sync_during_turn(
    state: &AppState,
    seq: &mut u64,
    white_time_left_ms: &mut u64,
    black_time_left_ms: &mut u64,
    turn_started_server_unix_ms: &mut i64,
    side: cozy_chess::Color,
    match_id: Uuid,
    fen: &str,
    moves: &[String],
    side_to_move: arena_core::ProtocolLiveSide,
) {
    let elapsed_ms = Utc::now()
        .timestamp_millis()
        .saturating_sub(*turn_started_server_unix_ms) as u64;
    if elapsed_ms == 0 {
        return;
    }
    if side == cozy_chess::Color::White {
        *white_time_left_ms = (*white_time_left_ms).saturating_sub(elapsed_ms);
    } else {
        *black_time_left_ms = (*black_time_left_ms).saturating_sub(elapsed_ms);
    }
    *turn_started_server_unix_ms = Utc::now().timestamp_millis();
    *seq += 1;
    let checkpoint = LiveRuntimeCheckpoint {
        match_id,
        seq: *seq,
        status: live_status_from_match_status(MatchStatus::Running),
        result: arena_core::LiveResult::None,
        termination: arena_core::LiveTermination::None,
        fen: fen.to_string(),
        moves: moves.to_vec(),
        white_remaining_ms: *white_time_left_ms,
        black_remaining_ms: *black_time_left_ms,
        side_to_move,
        turn_started_server_unix_ms: *turn_started_server_unix_ms,
        updated_at: Utc::now(),
    };
    publish_transient_with_metrics(
        &state.live_matches,
        Some(&state.live_metrics),
        checkpoint.clone(),
        arena_core::LiveEventEnvelope::ClockSync(clock_sync_from_checkpoint(&checkpoint)),
    )
    .await;
}

async fn emit_human_clock_sync(
    state: &AppState,
    session: &HumanGameSession,
    runtime: &mut HumanGameRuntime,
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
    let checkpoint = human_checkpoint(session, runtime);
    publish_transient_with_metrics(
        &state.live_matches,
        Some(&state.live_metrics),
        checkpoint.clone(),
        arena_core::LiveEventEnvelope::ClockSync(clock_sync_from_checkpoint(&checkpoint)),
    )
    .await;
    Ok(())
}

async fn finalize_human_timeout(
    state: &AppState,
    session: &HumanGameSession,
    runtime: &mut HumanGameRuntime,
) -> Result<(), ApiError> {
    if runtime.status != MatchStatus::Running {
        return Ok(());
    }
    runtime.result = Some(if runtime.board.side_to_move() == cozy_chess::Color::White {
        GameResult::BlackWin
    } else {
        GameResult::WhiteWin
    });
    runtime.termination = Some(arena_core::GameTermination::Timeout);
    runtime.status = MatchStatus::Completed;
    if runtime.board.side_to_move() == cozy_chess::Color::White {
        runtime.white_time_left_ms = 0;
    } else {
        runtime.black_time_left_ms = 0;
    }
    publish_human_runtime(state, session, runtime, false).await
}

async fn publish_human_runtime(
    state: &AppState,
    session: &HumanGameSession,
    runtime: &mut HumanGameRuntime,
    initial: bool,
) -> Result<(), ApiError> {
    runtime.seq += 1;
    let checkpoint = human_checkpoint(session, runtime);
    let event = if initial {
        arena_core::LiveEventEnvelope::Snapshot(snapshot_from_checkpoint(&checkpoint))
    } else if runtime.status == MatchStatus::Running {
        arena_core::LiveEventEnvelope::MoveCommitted(move_committed_from_checkpoint(&checkpoint))
    } else {
        arena_core::LiveEventEnvelope::GameFinished(game_finished_from_checkpoint(&checkpoint))
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

fn human_checkpoint(session: &HumanGameSession, runtime: &HumanGameRuntime) -> LiveRuntimeCheckpoint {
    let updated_at = Utc::now();
    LiveRuntimeCheckpoint {
        match_id: session.match_series.id,
        seq: runtime.seq,
        status: live_status_from_match_status(runtime.status),
        result: runtime.result.map(live_result_from_game_result).unwrap_or(arena_core::LiveResult::None),
        termination: runtime
            .termination
            .map(live_termination_from_game_termination)
            .unwrap_or(arena_core::LiveTermination::None),
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

async fn process_human_move(
    state: &AppState,
    session: &HumanGameSession,
    runtime: &mut HumanGameRuntime,
    intent_id: Uuid,
    move_uci: String,
) -> HumanMoveAck {
    if let Some(previous) = runtime.seen_intents.get(&intent_id).copied() {
        return previous;
    }
    if runtime.status != MatchStatus::Running {
        runtime.seen_intents.insert(intent_id, HumanMoveAck::RejectedGameFinished);
        return HumanMoveAck::RejectedGameFinished;
    }
    let human_side = if session.match_series.white_version_id == session.human_player.id {
        cozy_chess::Color::White
    } else {
        cozy_chess::Color::Black
    };
    if runtime.board.side_to_move() != human_side {
        runtime.seen_intents.insert(intent_id, HumanMoveAck::RejectedNotYourTurn);
        return HumanMoveAck::RejectedNotYourTurn;
    }
    if runtime.move_history.len() as u16 >= runtime.max_plies {
        runtime.result = Some(GameResult::Draw);
        runtime.termination = Some(arena_core::GameTermination::MoveLimit);
        runtime.status = MatchStatus::Completed;
        let _ = publish_human_runtime(state, session, runtime, false).await;
        runtime.seen_intents.insert(intent_id, HumanMoveAck::RejectedGameFinished);
        return HumanMoveAck::RejectedGameFinished;
    }
    let handled_at = Utc::now();
    let elapsed_ms = elapsed_since_turn_start_ms(runtime);
    let increment_ms = runtime.time_control.increment_ms;
    let clock = if human_side == cozy_chess::Color::White {
        &mut runtime.white_time_left_ms
    } else {
        &mut runtime.black_time_left_ms
    };
    let remaining_before = *clock;
    *clock = clock.saturating_sub(elapsed_ms);
    if elapsed_ms >= remaining_before {
        runtime.result = Some(if human_side == cozy_chess::Color::White {
            GameResult::BlackWin
        } else {
            GameResult::WhiteWin
        });
        runtime.termination = Some(arena_core::GameTermination::Timeout);
        runtime.status = MatchStatus::Completed;
        *clock = 0;
        let _ = publish_human_runtime(state, session, runtime, false).await;
        runtime.seen_intents.insert(intent_id, HumanMoveAck::RejectedGameFinished);
        return HumanMoveAck::RejectedGameFinished;
    }
    let Ok(mv) = cozy_chess::util::parse_uci_move(&runtime.board, &move_uci) else {
        runtime.seen_intents.insert(intent_id, HumanMoveAck::RejectedIllegal);
        return HumanMoveAck::RejectedIllegal;
    };
    if runtime.board.try_play(mv).is_err() {
        runtime.seen_intents.insert(intent_id, HumanMoveAck::RejectedIllegal);
        return HumanMoveAck::RejectedIllegal;
    }
    runtime.move_history.push(move_uci);
    runtime.current_fen = format!("{}", runtime.board);
    let board_hash = runtime.board.hash_without_ep();
    *runtime.repetitions.entry(board_hash).or_insert(0) += 1;
    if human_side == cozy_chess::Color::White {
        runtime.white_time_left_ms = runtime.white_time_left_ms.saturating_add(increment_ms);
    } else {
        runtime.black_time_left_ms = runtime.black_time_left_ms.saturating_add(increment_ms);
    }
    runtime.turn_started_server_unix_ms = handled_at.timestamp_millis();
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
    let _ = publish_human_runtime(state, session, runtime, false).await;
    runtime.seen_intents.insert(intent_id, HumanMoveAck::Accepted);
    HumanMoveAck::Accepted
}

async fn process_engine_turn(
    state: &AppState,
    session: &HumanGameSession,
    runtime: &mut HumanGameRuntime,
) -> Result<(), ApiError> {
    if runtime.status != MatchStatus::Running || runtime.board.side_to_move() != runtime.engine_side {
        return Ok(());
    }
    if runtime.move_history.len() as u16 >= runtime.max_plies {
        runtime.result = Some(GameResult::Draw);
        runtime.termination = Some(arena_core::GameTermination::MoveLimit);
        runtime.status = MatchStatus::Completed;
        publish_human_runtime(state, session, runtime, false).await?;
        return Ok(());
    }
    let increment_ms = runtime.time_control.increment_ms;
    let movetime_ms = calculate_move_budget(
        if runtime.engine_side == cozy_chess::Color::White {
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
    let elapsed_started = Instant::now();
    let mut logs = std::mem::take(&mut runtime.logs);
    let selected = runtime
        .engine
        .choose_move(&board, &start_fen, &move_history, movetime_ms, &mut logs)
        .await;
    runtime.logs = logs;
    let elapsed_ms = elapsed_started.elapsed().as_millis() as u64;
    let clock = if runtime.engine_side == cozy_chess::Color::White {
        &mut runtime.white_time_left_ms
    } else {
        &mut runtime.black_time_left_ms
    };
    let remaining = *clock;
    *clock = clock.saturating_sub(elapsed_ms);
    match selected {
        Ok(selected) => {
            if elapsed_ms >= remaining {
                runtime.result = Some(if runtime.engine_side == cozy_chess::Color::White {
                    GameResult::BlackWin
                } else {
                    GameResult::WhiteWin
                });
                runtime.termination = Some(arena_core::GameTermination::Timeout);
                runtime.status = MatchStatus::Completed;
            } else if selected == "0000" {
                runtime.result = Some(GameResult::Draw);
                runtime.termination = Some(arena_core::GameTermination::EngineFailure);
                runtime.status = MatchStatus::Completed;
            } else if let Ok(mv) = cozy_chess::util::parse_uci_move(&runtime.board, &selected) {
                if runtime.board.try_play(mv).is_err() {
                    runtime.result = Some(if runtime.engine_side == cozy_chess::Color::White {
                        GameResult::BlackWin
                    } else {
                        GameResult::WhiteWin
                    });
                    runtime.termination = Some(arena_core::GameTermination::IllegalMove);
                    runtime.status = MatchStatus::Completed;
                } else {
                    runtime.move_history.push(selected);
                    runtime.current_fen = format!("{}", runtime.board);
                    let board_hash = runtime.board.hash_without_ep();
                    *runtime.repetitions.entry(board_hash).or_insert(0) += 1;
                    if runtime.engine_side == cozy_chess::Color::White {
                        runtime.white_time_left_ms = runtime.white_time_left_ms.saturating_add(increment_ms);
                    } else {
                        runtime.black_time_left_ms = runtime.black_time_left_ms.saturating_add(increment_ms);
                    }
                    runtime.turn_started_server_unix_ms = handled_at.timestamp_millis();
                    if let Some((result, termination)) =
                        classify_position(&runtime.board, &runtime.repetitions)
                    {
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
            } else {
                runtime.result = Some(if runtime.engine_side == cozy_chess::Color::White {
                    GameResult::BlackWin
                } else {
                    GameResult::WhiteWin
                });
                runtime.termination = Some(arena_core::GameTermination::IllegalMove);
                runtime.status = MatchStatus::Completed;
            }
        }
        Err(_) => {
            runtime.result = Some(if runtime.engine_side == cozy_chess::Color::White {
                GameResult::BlackWin
            } else {
                GameResult::WhiteWin
            });
            runtime.termination = Some(arena_core::GameTermination::EngineFailure);
            runtime.status = MatchStatus::Completed;
        }
    }
    publish_human_runtime(state, session, runtime, false).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_core::{GameLogEntry, TimeControl};
    use arena_runner::AgentAdapter;
    use async_trait::async_trait;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::time::Duration;

    use crate::{db::init_db, registry::{SetupRegistryCache, sync_setup_registry_if_changed}, state::{HumanGameStore, TournamentCoordinator}};

    struct SleepyAdapter {
        delay_ms: u64,
        move_uci: String,
    }

    #[async_trait]
    impl AgentAdapter for SleepyAdapter {
        async fn prepare(&mut self, _variant: Variant, _logs: &mut Vec<GameLogEntry>) -> Result<()> {
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
            frontend_dist: None,
            setup_registry,
        }
    }

    fn session_and_runtime(
        engine_side: cozy_chess::Color,
        human_plays_white: bool,
    ) -> (HumanGameSession, HumanGameRuntime) {
        let match_series = MatchSeries {
            id: Uuid::new_v4(),
            tournament_id: Uuid::new_v4(),
            pool_id: Uuid::new_v4(),
            round_index: 0,
            white_version_id: if human_plays_white { Uuid::from_u128(1) } else { Uuid::new_v4() },
            black_version_id: if human_plays_white { Uuid::new_v4() } else { Uuid::from_u128(1) },
            opening_id: None,
            game_index: 0,
            status: MatchStatus::Running,
            created_at: Utc::now(),
        };
        let (command_tx, _command_rx) = tokio::sync::mpsc::channel(1);
        let session = HumanGameSession {
            name: "test".to_string(),
            match_series: match_series.clone(),
            human_player: HumanPlayer {
                id: Uuid::from_u128(1),
                name: "You".to_string(),
                created_at: Utc::now(),
            },
            command_tx,
        };
        let board = cozy_chess::Board::default();
        let runtime = HumanGameRuntime {
            tournament_id: match_series.tournament_id,
            variant: Variant::Standard,
            time_control: TimeControl { initial_ms: 50, increment_ms: 0 },
            start_fen: board.to_string(),
            current_fen: board.to_string(),
            board,
            repetitions: HashMap::from([(cozy_chess::Board::default().hash_without_ep(), 1)]),
            move_history: Vec::new(),
            white_time_left_ms: 50,
            black_time_left_ms: 50,
            max_plies: 300,
            engine_side,
            engine: Box::new(SleepyAdapter { delay_ms: 0, move_uci: "e2e4".to_string() }),
            logs: Vec::new(),
            started_at: Utc::now(),
            turn_started_server_unix_ms: Utc::now().timestamp_millis(),
            seq: 0,
            seen_intents: HashMap::new(),
            result: None,
            termination: None,
            status: MatchStatus::Running,
        };
        (session, runtime)
    }

    #[tokio::test]
    async fn human_move_times_out_when_elapsed_exceeds_remaining_clock() {
        let state = test_state().await;
        let (session, mut runtime) = session_and_runtime(cozy_chess::Color::Black, true);
        runtime.turn_started_server_unix_ms -= 75;
        runtime.white_time_left_ms = 25;

        let ack = process_human_move(
            &state,
            &session,
            &mut runtime,
            Uuid::new_v4(),
            "e2e4".to_string(),
        )
        .await;

        assert!(matches!(ack, HumanMoveAck::RejectedGameFinished));
        assert_eq!(runtime.status, MatchStatus::Completed);
        assert_eq!(runtime.termination, Some(arena_core::GameTermination::Timeout));
        let snapshot = state.live_matches.get_snapshot(session.match_series.id).await.unwrap();
        assert_eq!(snapshot.status, arena_core::LiveStatus::Finished);
        assert_eq!(snapshot.termination, arena_core::LiveTermination::Timeout);
    }

    #[tokio::test]
    async fn engine_turn_times_out_when_adapter_responds_too_late() {
        let state = test_state().await;
        let (session, mut runtime) = session_and_runtime(cozy_chess::Color::White, false);
        runtime.white_time_left_ms = 20;
        runtime.engine = Box::new(SleepyAdapter {
            delay_ms: 40,
            move_uci: "e2e4".to_string(),
        });

        process_engine_turn(&state, &session, &mut runtime).await.unwrap();

        assert_eq!(runtime.status, MatchStatus::Completed);
        assert_eq!(runtime.termination, Some(arena_core::GameTermination::Timeout));
        let snapshot = state.live_matches.get_snapshot(session.match_series.id).await.unwrap();
        assert_eq!(snapshot.status, arena_core::LiveStatus::Finished);
        assert_eq!(snapshot.termination, arena_core::LiveTermination::Timeout);
    }

    #[tokio::test]
    async fn human_owner_times_out_without_submitted_move() {
        let state = test_state().await;
        let (session, mut runtime) = session_and_runtime(cozy_chess::Color::Black, true);
        runtime.white_time_left_ms = 10;
        runtime.turn_started_server_unix_ms -= 20;
        state.human_games.insert(session.clone()).await;
        let (_command_tx, command_rx) = tokio::sync::mpsc::channel(1);

        tokio::spawn(run_human_game_owner(
            state.clone(),
            session.clone(),
            runtime,
            command_rx,
            true,
        ));

        tokio::time::sleep(Duration::from_millis(50)).await;

        let snapshot = state.live_matches.get_snapshot(session.match_series.id).await.unwrap();
        assert_eq!(snapshot.status, arena_core::LiveStatus::Finished);
        assert_eq!(snapshot.termination, arena_core::LiveTermination::Timeout);
        assert_eq!(snapshot.white_remaining_ms, 0);
    }
}
