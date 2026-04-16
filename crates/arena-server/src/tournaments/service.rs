use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Result, anyhow, bail};
use arena_core::{
    AgentVersion, EventPreset, EventPresetSelectionMode, GameRecord, LeaderboardEntry,
    LiveRuntimeCheckpoint, MatchSeries, MatchStatus, RoundRobinScheduler, ScheduledPair,
    StabilityConfig, StabilityTracker, Tournament, TournamentKind, TournamentStatus,
    snapshot_from_entry,
};
use arena_runner::build_adapter;
use chrono::Utc;
use sqlx::SqlitePool;
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    ApiError,
    gameplay::{
        MatchConfig, ensure_engine_supports_variant, parse_saved_board, resolve_start_state,
    },
    match_runtime::{
        owner::run_match_to_completion,
        types::{
            CompletedGameTable, EngineSeatController, MatchRuntime, MatchSeatController,
            MatchSession,
        },
    },
    rating::build_pair_rating_update,
    state::AppState,
    storage::{
        ensure_leaderboard_seed, get_agent_version, get_match_series, get_pool, get_tournament,
        insert_match_series, insert_rating_snapshot, insert_tournament, list_agent_versions,
        list_agent_versions_by_ids, load_pool_openings, update_match_series_status,
        update_tournament_status,
    },
};

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
        human_games::service::create_human_game,
        match_runtime::{
            engine_turn::process_engine_turn,
            finalize::finalize_match_game,
            human_turn::process_human_move,
            owner::run_match_owner,
            types::{HumanGameHandle, HumanMoveAck, HumanSeatController},
        },
        registry::{SetupRegistryCache, sync_setup_registry_if_changed},
        state::MoveDebugContext,
        state::{HumanGameStore, TournamentCoordinator},
        storage::{ensure_human_player, insert_tournament},
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
