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
    LiveGameFrame, LiveGameState, LiveSide, MatchSeries, MatchStatus, RoundRobinScheduler,
    ScheduledPair, StabilityConfig, StabilityTracker, Tournament, TournamentKind,
    TournamentStatus, Variant, snapshot_from_entry,
};
use arena_runner::{
    MatchPairRequest, MatchRequest, build_adapter, calculate_move_budget, classify_position,
    classify_terminal_board, pgn_from_moves, play_match_pair,
};
use chrono::Utc;
use sqlx::SqlitePool;
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    ApiError,
    gameplay::starting_board_for_human_game,
    presentation::HumanPlayerProfile,
    rating::build_pair_rating_update,
    state::{AppState, HumanGameRuntime, HumanGameSession, HumanPlayer},
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
        result: None,
        termination: None,
        status: MatchStatus::Running,
    };
    let session = HumanGameSession {
        name,
        match_series: match_series.clone(),
        human_player,
        runtime: Arc::new(tokio::sync::Mutex::new(runtime)),
    };
    state.human_games.insert(session.clone()).await;
    publish_human_live_state(state, &session).await;
    if !human_plays_white {
        tokio::spawn(run_human_engine_turn(state.clone(), match_id));
    }

    Ok((match_id, tournament_id))
}

pub(crate) async fn load_human_player_profile(
    db: &SqlitePool,
) -> Result<HumanPlayerProfile, ApiError> {
    let player = ensure_human_player(db).await?;
    load_human_profile(db, &player).await
}

pub(crate) async fn publish_human_live_state(state: &AppState, session: &HumanGameSession) {
    let runtime = session.runtime.lock().await;
    state
        .live_games
        .upsert(LiveGameState {
            match_id: session.match_series.id,
            tournament_id: runtime.tournament_id,
            pool_id: session.match_series.pool_id,
            variant: runtime.variant,
            white_version_id: session.match_series.white_version_id,
            black_version_id: session.match_series.black_version_id,
            start_fen: runtime.start_fen.clone(),
            current_fen: runtime.current_fen.clone(),
            moves_uci: runtime.move_history.clone(),
            white_time_left_ms: runtime.white_time_left_ms,
            black_time_left_ms: runtime.black_time_left_ms,
            status: runtime.status,
            result: runtime.result,
            termination: runtime.termination,
            updated_at: Utc::now(),
            live_frames: vec![LiveGameFrame {
                ply: runtime.move_history.len() as u32,
                fen: runtime.current_fen.clone(),
                move_uci: runtime.move_history.last().cloned(),
                white_time_left_ms: runtime.white_time_left_ms,
                black_time_left_ms: runtime.black_time_left_ms,
                updated_at: Utc::now(),
                side_to_move: if runtime.board.side_to_move() == cozy_chess::Color::Black {
                    LiveSide::Black
                } else {
                    LiveSide::White
                },
                status: runtime.status,
                result: runtime.result,
                termination: runtime.termination,
            }],
        })
        .await;
}

pub(crate) async fn submit_human_move(
    state: AppState,
    match_id: Uuid,
    uci: String,
) -> Result<(), ApiError> {
    let session = state
        .human_games
        .get(match_id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("human game {match_id} not found")))?;
    {
        let mut runtime = session.runtime.lock().await;
        if runtime.status != MatchStatus::Running {
            return Err(ApiError::Conflict("game is no longer running".to_string()));
        }
        let human_side = if session.match_series.white_version_id == session.human_player.id {
            cozy_chess::Color::White
        } else {
            cozy_chess::Color::Black
        };
        if runtime.board.side_to_move() != human_side {
            return Err(ApiError::Conflict("it is not your turn".to_string()));
        }
        if runtime.move_history.len() as u16 >= runtime.max_plies {
            return Err(ApiError::Conflict("move limit reached".to_string()));
        }

        let elapsed_ms = runtime.turn_started_at.elapsed().as_millis() as u64;
        let increment_ms = runtime.time_control.increment_ms;
        let clock = if human_side == cozy_chess::Color::White {
            &mut runtime.white_time_left_ms
        } else {
            &mut runtime.black_time_left_ms
        };
        let remaining_before = *clock;
        *clock = clock
            .saturating_add(increment_ms)
            .saturating_sub(elapsed_ms);
        if elapsed_ms > remaining_before.saturating_add(increment_ms) {
            runtime.result = Some(if human_side == cozy_chess::Color::White {
                GameResult::BlackWin
            } else {
                GameResult::WhiteWin
            });
            runtime.termination = Some(arena_core::GameTermination::Timeout);
            runtime.status = MatchStatus::Completed;
        } else {
            let mv = cozy_chess::util::parse_uci_move(&runtime.board, &uci)
                .map_err(|_| ApiError::BadRequest("illegal move".to_string()))?;
            runtime
                .board
                .try_play(mv)
                .map_err(|_| ApiError::BadRequest("illegal move".to_string()))?;
            runtime.move_history.push(uci);
            runtime.current_fen = format!("{}", runtime.board);
            let board_hash = runtime.board.hash_without_ep();
            *runtime.repetitions.entry(board_hash).or_insert(0) += 1;
            runtime.turn_started_at = Instant::now();
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
    }
    publish_human_live_state(&state, &session).await;
    let status = { session.runtime.lock().await.status };
    if status == MatchStatus::Completed {
        finalize_human_game(state, match_id).await?;
    } else {
        tokio::spawn(run_human_engine_turn(state.clone(), match_id));
    }
    Ok(())
}

pub(crate) async fn run_human_engine_turn(state: AppState, match_id: Uuid) {
    let Some(session) = state.human_games.get(match_id).await else {
        return;
    };
    let finalize = {
        let mut runtime = session.runtime.lock().await;
        if runtime.status != MatchStatus::Running || runtime.board.side_to_move() != runtime.engine_side {
            false
        } else if runtime.move_history.len() as u16 >= runtime.max_plies {
            runtime.result = Some(GameResult::Draw);
            runtime.termination = Some(arena_core::GameTermination::MoveLimit);
            runtime.status = MatchStatus::Completed;
            true
        } else {
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
            *clock = clock
                .saturating_add(increment_ms)
                .saturating_sub(elapsed_ms);

            match selected {
                Ok(selected) => {
                    if elapsed_ms > remaining.saturating_add(increment_ms) {
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
                    } else if let Ok(mv) =
                        cozy_chess::util::parse_uci_move(&runtime.board, &selected)
                    {
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
                            runtime.turn_started_at = Instant::now();
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
            runtime.status == MatchStatus::Completed
        }
    };
    publish_human_live_state(&state, &session).await;
    if finalize {
        let _ = finalize_human_game(state, match_id).await;
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

async fn finalize_human_game(state: AppState, match_id: Uuid) -> Result<(), ApiError> {
    let Some(session) = state.human_games.remove(match_id).await else {
        return Ok(());
    };
    let mut runtime = session.runtime.lock().await;
    let mut shutdown_logs = std::mem::take(&mut runtime.logs);
    runtime.engine.shutdown(&mut shutdown_logs).await.ok();
    runtime.logs = shutdown_logs;
    let result = runtime.result.unwrap_or(GameResult::Draw);
    let termination = runtime
        .termination
        .unwrap_or(arena_core::GameTermination::Unknown);
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
    state.live_games.remove(match_id).await;
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
            status: MatchStatus::Running,
            created_at: Utc::now(),
        };
        insert_match_series(&state.db, &series).await?;
        Some(series)
    } else {
        None
    };

    let progress_sink = Arc::new({
        let live_games = state.live_games.clone();
        move |live_state| {
            let live_games = live_games.clone();
            tokio::spawn(async move {
                live_games.upsert(live_state).await;
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
                state.live_games.remove(game.match_id).await;
            }
            Ok(pair)
        }
        Err(err) => {
            update_match_series_status(&state.db, first_series.id, MatchStatus::Failed).await?;
            state.live_games.remove(first_series.id).await;
            if let Some(second_series) = second_series {
                update_match_series_status(&state.db, second_series.id, MatchStatus::Failed)
                    .await?;
                state.live_games.remove(second_series.id).await;
            }
            Err(err)
        }
    }
}
