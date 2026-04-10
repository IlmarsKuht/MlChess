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
    LiveGameFrame, LiveGameState, LiveRuntimeCheckpoint, LiveSide, MatchSeries, MatchStatus,
    RoundRobinScheduler, ScheduledPair, StabilityConfig, StabilityTracker, Tournament,
    TournamentKind, TournamentStatus, Variant, snapshot_from_entry,
};
use arena_runner::{
    MatchPairRequest, MatchRequest, build_adapter, calculate_move_budget, classify_position,
    classify_terminal_board, pgn_from_moves, play_match_pair, starting_board,
};
use chrono::Utc;
use sqlx::SqlitePool;
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    ApiError,
    gameplay::starting_board_for_human_game,
    live::{
        game_finished_from_checkpoint, live_result_from_game_result, live_status_from_match_status,
        live_termination_from_game_termination, move_committed_from_checkpoint, side_from_fen,
        snapshot_from_checkpoint,
    },
    presentation::HumanPlayerProfile,
    rating::build_pair_rating_update,
    state::{AppState, HumanGameCommand, HumanGameRuntime, HumanGameSession, HumanMoveAck, HumanPlayer},
    storage::{
        ensure_agent_version_exists, ensure_human_player, ensure_leaderboard_seed, ensure_pool_exists,
        get_agent_version, get_pool, get_tournament, insert_game, insert_human_game,
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
        created_at: Utc::now(),
    };
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
        started_at: Utc::now(),
        turn_started_at: Instant::now(),
        turn_started_server_unix_ms: Utc::now().timestamp_millis(),
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
        HumanMoveAck::Duplicate => return Ok("duplicate"),
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

async fn run_human_game_owner(
    state: AppState,
    session: HumanGameSession,
    mut runtime: HumanGameRuntime,
    mut command_rx: tokio::sync::mpsc::Receiver<HumanGameCommand>,
) {
    let _ = publish_human_runtime(&state, &session, &mut runtime, true).await;
    loop {
        if runtime.status != MatchStatus::Running {
            let _ = finalize_human_game(state.clone(), session.clone(), runtime).await;
            return;
        }
        if runtime.board.side_to_move() == runtime.engine_side {
            let _ = process_engine_turn(&state, &session, &mut runtime).await;
            continue;
        }
        let Some(command) = command_rx.recv().await else {
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
    state
        .live_matches
        .record_legacy_state(
            &state.db,
            &build_pending_live_state(tournament_id, pool, &first_series, opening.as_ref(), pair_index),
        )
        .await?;

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

    let progress_sink = Arc::new({
        let live_matches = state.live_matches.clone();
        let db = state.db.clone();
        move |live_state: LiveGameState| {
            let live_matches = live_matches.clone();
            let db = db.clone();
            tokio::spawn(async move {
                let status = if matches!(live_state.status, MatchStatus::Completed) {
                    MatchStatus::Completed
                } else {
                    MatchStatus::Running
                };
                let _ = update_match_series_status(&db, live_state.match_id, status).await;
                let _ = live_matches.record_legacy_state(&db, &live_state).await;
            });
        }
    });

    let mut requests = vec![MatchRequest {
        tournament_id,
        match_series: first_series.clone(),
        variant: pool.variant,
        white: engine_a.clone(),
        black: engine_b.clone(),
        opening: opening.clone(),
        time_control: pool.time_control.clone(),
        max_plies: 300,
        opening_seed: pool.fairness.opening_seed.or(Some(pair_index as u64)),
        progress_sink: Some(progress_sink.clone()),
    }];

    if let Some(second_series) = second_series.clone() {
        requests.push(MatchRequest {
            tournament_id,
            match_series: second_series,
            variant: pool.variant,
            white: engine_b.clone(),
            black: engine_a.clone(),
            opening,
            time_control: pool.time_control.clone(),
            max_plies: 300,
            opening_seed: pool.fairness.opening_seed.or(Some(pair_index as u64)),
            progress_sink: Some(progress_sink),
        });
    }

    match play_match_pair(MatchPairRequest {
        engine_a_id: engine_a.id,
        engine_b_id: engine_b.id,
        games: requests,
    })
    .await
    {
        Ok(pair) => {
            update_match_series_status(&state.db, first_series.id, MatchStatus::Completed).await?;
            if let Some(second_series) = second_series {
                update_match_series_status(&state.db, second_series.id, MatchStatus::Completed)
                    .await?;
            }
            for game in &pair.games {
                insert_game(&state.db, game).await?;
            }
            Ok(pair)
        }
        Err(err) => {
            update_match_series_status(&state.db, first_series.id, MatchStatus::Failed).await?;
            if let Some(second_series) = second_series {
                update_match_series_status(&state.db, second_series.id, MatchStatus::Failed)
                    .await?;
            }
            Err(err)
        }
    }
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
    state.live_matches.publish(&state.db, checkpoint, event).await
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
    let elapsed_ms = runtime.turn_started_at.elapsed().as_millis() as u64;
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
    runtime.turn_started_at = Instant::now();
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
                    runtime.turn_started_at = Instant::now();
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

fn build_pending_live_state(
    tournament_id: Uuid,
    pool: &arena_core::BenchmarkPool,
    series: &MatchSeries,
    opening: Option<&arena_core::OpeningPosition>,
    pair_index: u32,
) -> LiveGameState {
    let board = starting_board(
        pool.variant,
        opening,
        pool.fairness.opening_seed.or(Some(pair_index as u64)),
    );
    let fen = board.to_string();
    let updated_at = Utc::now();

    LiveGameState {
        match_id: series.id,
        tournament_id,
        pool_id: pool.id,
        variant: pool.variant,
        white_version_id: series.white_version_id,
        black_version_id: series.black_version_id,
        start_fen: fen.clone(),
        current_fen: fen.clone(),
        moves_uci: Vec::new(),
        white_time_left_ms: pool.time_control.initial_ms,
        black_time_left_ms: pool.time_control.initial_ms,
        status: MatchStatus::Running,
        result: None,
        termination: None,
        updated_at,
        live_frames: vec![LiveGameFrame {
            ply: 0,
            fen,
            move_uci: None,
            white_time_left_ms: pool.time_control.initial_ms,
            black_time_left_ms: pool.time_control.initial_ms,
            updated_at,
            side_to_move: if board.side_to_move() == cozy_chess::Color::Black {
                LiveSide::Black
            } else {
                LiveSide::White
            },
            status: MatchStatus::Running,
            result: None,
            termination: None,
        }],
    }
}
