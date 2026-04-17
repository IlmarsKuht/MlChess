// Legacy transition module. Do not add new behavior here. Move code into feature/bootstrap/storage/state submodules instead.
mod api;
mod bootstrap;
mod debug;
mod db;
mod gameplay;
mod human_games;
mod live;
mod match_runtime;
mod presentation;
mod rating;
mod registry;
mod registry_loader;
mod registry_simple_toml;
mod registry_sync;
mod state;
mod storage;
mod tournaments;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use tracing::error;

#[derive(Debug, thiserror::Error)]
enum ApiError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    BadRequest(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::NotFound(message) => (StatusCode::NOT_FOUND, message),
            Self::Conflict(message) => (StatusCode::CONFLICT, message),
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::Internal(err) => {
                error!("internal error: {err:#}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_string(),
                )
            }
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(value: sqlx::Error) -> Self {
        Self::Internal(value.into())
    }
}

pub use bootstrap::reconciliation::cleanup_stale_match_statuses;
pub use bootstrap::server::run_server;
#[cfg(test)]
pub(crate) use db::init_db;
#[cfg(test)]
pub(crate) use bootstrap::server::build_app;
pub(crate) use bootstrap::server::workspace_root;
#[cfg(test)]
pub(crate) use bootstrap::restore::restore_live_runtime;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        live::LiveMatchStore,
        registry::{sync_setup_registry_if_changed, SetupRegistryCache},
        state::{AppState, HumanGameStore, LiveMetricsStore, TournamentCoordinator},
    };
    use crate::storage::{
        insert_game, insert_match_series, insert_tournament, list_agent_versions, list_agents,
        list_games, list_match_series, list_pools, list_tournaments, load_aggregate_leaderboard,
    };
    use arena_core::{
        GameRecord, GameResult, GameTermination, LiveStatus, MatchSeries, MatchStatus, Tournament,
        TournamentKind, TournamentStatus,
    };
    use axum::Router;
    use axum::body::Body;
    use chrono::Utc;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use tower::ServiceExt;
    use uuid::Uuid;

    async fn setup_app() -> Router {
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
        build_app(AppState {
            db,
            coordinator: TournamentCoordinator::default(),
            live_matches: LiveMatchStore::default(),
            live_metrics: LiveMetricsStore::default(),
            human_games: HumanGameStore::default(),
            debug_reports_dir: std::env::temp_dir().join("mlchess-debug-reports"),
            frontend_dist: None,
            setup_registry,
        })
    }

    #[tokio::test]
    async fn healthcheck_works() {
        let app = setup_app().await;
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn setup_post_routes_are_read_only() {
        let app = setup_app().await;
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/agents")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"name":"bot","tags":["baseline"]}"#))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn registry_sync_registers_starter_defaults() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        init_db(&db).await.unwrap();
        sync_setup_registry_if_changed(&db, &SetupRegistryCache::default())
            .await
            .unwrap();

        let agents = list_agents(&db).await.unwrap();
        let versions = list_agent_versions(&db, None).await.unwrap();
        let pools = list_pools(&db).await.unwrap();
        let leaderboard = load_aggregate_leaderboard(&db).await.unwrap();

        assert!(agents.len() >= 1);
        assert!(versions.len() >= 1);
        assert!(
            pools
                .iter()
                .any(|pool| pool.registry_key.as_deref() == Some("starter-standard-pool"))
        );
        assert!(
            pools
                .iter()
                .any(|pool| pool.registry_key.as_deref() == Some("starter-chess960-pool"))
        );
        assert!(leaderboard.len() >= 1);
    }

    #[tokio::test]
    async fn create_tournament_run_requires_two_entrants() {
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

        let pool_id = list_pools(&db).await.unwrap().remove(0).id;
        let version_id = list_agent_versions(&db, None).await.unwrap().remove(0).id;

        let result = crate::tournaments::service::create_tournament_run(
            &db,
            "Too Small".to_string(),
            TournamentKind::RoundRobin,
            pool_id,
            vec![version_id],
            1,
            1,
        )
        .await;

        assert!(matches!(result, Err(ApiError::BadRequest(_))));
    }

    #[tokio::test]
    async fn event_presets_are_backend_defined() {
        let app = setup_app().await;

        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/event-presets")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/event-presets")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{
                            "name":"Nope",
                            "kind":"round_robin",
                            "pool_id":"00000000-0000-0000-0000-000000000000"
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn build_app_accepts_frontend_fallback() {
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

        let frontend_dist = std::env::current_dir().unwrap();
        let _app = build_app(AppState {
            db,
            coordinator: TournamentCoordinator::default(),
            live_matches: LiveMatchStore::default(),
            live_metrics: LiveMetricsStore::default(),
            human_games: HumanGameStore::default(),
            debug_reports_dir: std::env::temp_dir().join("mlchess-debug-reports"),
            frontend_dist: Some(frontend_dist),
            setup_registry,
        });
    }

    #[tokio::test]
    async fn build_app_preserves_existing_session_rows() {
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

        let pool = list_pools(&db).await.unwrap().remove(0);
        let mut versions = list_agent_versions(&db, None).await.unwrap();
        if versions.len() < 2 {
            panic!("expected starter setup to provide at least two engine versions");
        }
        let white = versions.remove(0);
        let black = versions.remove(0);
        let tournament = Tournament {
            id: Uuid::new_v4(),
            name: "Preserve".to_string(),
            kind: TournamentKind::RoundRobin,
            pool_id: pool.id,
            participant_version_ids: vec![white.id, black.id],
            worker_count: 1,
            games_per_pairing: 1,
            status: TournamentStatus::Running,
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            completed_at: None,
        };
        insert_tournament(&db, &tournament).await.unwrap();

        let series = arena_core::MatchSeries {
            id: Uuid::new_v4(),
            tournament_id: tournament.id,
            pool_id: pool.id,
            round_index: 0,
            white_version_id: white.id,
            black_version_id: black.id,
            opening_id: None,
            game_index: 0,
            status: MatchStatus::Running,
            created_at: Utc::now(),
        };
        insert_match_series(&db, &series).await.unwrap();

        let game = GameRecord {
            id: Uuid::new_v4(),
            tournament_id: tournament.id,
            match_id: series.id,
            pool_id: pool.id,
            variant: pool.variant,
            opening_id: None,
            white_version_id: white.id,
            black_version_id: black.id,
            result: GameResult::Draw,
            termination: GameTermination::MoveLimit,
            start_fen: "startpos".to_string(),
            pgn: String::new(),
            moves_uci: Vec::new(),
            white_time_left_ms: pool.time_control.initial_ms,
            black_time_left_ms: pool.time_control.initial_ms,
            logs: Vec::new(),
            started_at: Utc::now(),
            completed_at: Utc::now(),
        };
        insert_game(&db, &game).await.unwrap();

        let _app = build_app(AppState {
            db: db.clone(),
            coordinator: TournamentCoordinator::default(),
            live_matches: LiveMatchStore::default(),
            live_metrics: LiveMetricsStore::default(),
            human_games: HumanGameStore::default(),
            debug_reports_dir: std::env::temp_dir().join("mlchess-debug-reports"),
            frontend_dist: None,
            setup_registry,
        });

        assert_eq!(list_tournaments(&db).await.unwrap().len(), 1);
        assert_eq!(list_match_series(&db, None).await.unwrap().len(), 1);
        assert_eq!(list_games(&db, None, None).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn restore_live_runtime_rehydrates_running_match_state() {
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
        let state = AppState {
            db: db.clone(),
            coordinator: TournamentCoordinator::default(),
            live_matches: LiveMatchStore::default(),
            live_metrics: LiveMetricsStore::default(),
            human_games: HumanGameStore::default(),
            debug_reports_dir: std::env::temp_dir().join("mlchess-debug-reports"),
            frontend_dist: None,
            setup_registry,
        };

        let match_id = Uuid::new_v4();
        let pool_id = list_pools(&db).await.unwrap().remove(0).id;
        let mut versions = list_agent_versions(&db, None).await.unwrap();
        let white = versions.remove(0);
        let black = versions.remove(0);
        let tournament = Tournament {
            id: Uuid::new_v4(),
            name: "restore".to_string(),
            kind: TournamentKind::RoundRobin,
            pool_id,
            participant_version_ids: vec![white.id, black.id],
            worker_count: 1,
            games_per_pairing: 1,
            status: TournamentStatus::Running,
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            completed_at: None,
        };
        crate::storage::insert_tournament(&db, &tournament)
            .await
            .unwrap();
        crate::storage::insert_match_series(
            &db,
            &MatchSeries {
                id: match_id,
                tournament_id: tournament.id,
                pool_id,
                round_index: 0,
                white_version_id: white.id,
                black_version_id: black.id,
                opening_id: None,
                game_index: 0,
                status: MatchStatus::Running,
                created_at: Utc::now(),
            },
        )
        .await
        .unwrap();
        let checkpoint = arena_core::LiveRuntimeCheckpoint {
            match_id,
            seq: 2,
            status: LiveStatus::Running,
            result: arena_core::LiveResult::None,
            termination: arena_core::LiveTermination::None,
            start_fen: cozy_chess::Board::default().to_string(),
            fen: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1".to_string(),
            moves: vec!["e2e4".to_string()],
            white_remaining_ms: 60_000,
            black_remaining_ms: 60_000,
            side_to_move: arena_core::ProtocolLiveSide::Black,
            turn_started_server_unix_ms: Utc::now().timestamp_millis(),
            updated_at: Utc::now(),
        };
        crate::storage::upsert_live_runtime_checkpoint(&db, &checkpoint)
            .await
            .unwrap();
        crate::storage::insert_live_runtime_event(
            &db,
            &arena_core::LiveEventEnvelope::Snapshot(crate::live::snapshot_from_checkpoint(
                &arena_core::LiveRuntimeCheckpoint {
                    seq: 1,
                    moves: Vec::new(),
                    ..checkpoint.clone()
                },
            )),
        )
        .await
        .unwrap();
        crate::storage::insert_live_runtime_event(
            &db,
            &arena_core::LiveEventEnvelope::MoveCommitted(
                crate::live::move_committed_from_checkpoint(&checkpoint),
            ),
        )
        .await
        .unwrap();

        restore_live_runtime(&state).await.unwrap();

        let snapshot = state.live_matches.get_snapshot(match_id).await.unwrap();
        assert_eq!(snapshot.seq, 2);
        assert_eq!(snapshot.moves, vec!["e2e4".to_string()]);
        let crate::live::ReplayResult::Replay(replay) =
            state.live_matches.replay_since(match_id, 0).await.unwrap()
        else {
            panic!("expected replay events");
        };
        assert_eq!(replay.len(), 2);
    }

    #[tokio::test]
    async fn cleanup_stale_match_statuses_updates_finished_match_rows() {
        let temp_path = std::env::temp_dir().join(format!("mlchess-cleanup-{}.db", Uuid::new_v4()));
        let db_path = temp_path.to_string_lossy().replace('\\', "/");
        let db_url = if db_path.starts_with('/') {
            format!("sqlite://{db_path}")
        } else {
            format!("sqlite:///{db_path}")
        };
        let db_options = db_url
            .parse::<SqliteConnectOptions>()
            .unwrap()
            .create_if_missing(true)
            .foreign_keys(true);
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(db_options)
            .await
            .unwrap();
        init_db(&db).await.unwrap();
        let setup_registry = SetupRegistryCache::default();
        sync_setup_registry_if_changed(&db, &setup_registry)
            .await
            .unwrap();

        let pool = list_pools(&db).await.unwrap().remove(0);
        let mut versions = list_agent_versions(&db, None).await.unwrap();
        let white = versions.remove(0);
        let black = versions.remove(0);
        let tournament = Tournament {
            id: Uuid::new_v4(),
            name: "cleanup".to_string(),
            kind: TournamentKind::RoundRobin,
            pool_id: pool.id,
            participant_version_ids: vec![white.id, black.id],
            worker_count: 1,
            games_per_pairing: 1,
            status: TournamentStatus::Completed,
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            completed_at: Some(Utc::now()),
        };
        insert_tournament(&db, &tournament).await.unwrap();

        let game_only_match_id = Uuid::new_v4();
        insert_match_series(
            &db,
            &MatchSeries {
                id: game_only_match_id,
                tournament_id: tournament.id,
                pool_id: pool.id,
                round_index: 2,
                white_version_id: white.id,
                black_version_id: black.id,
                opening_id: None,
                game_index: 0,
                status: MatchStatus::Running,
                created_at: Utc::now(),
            },
        )
        .await
        .unwrap();
        insert_game(
            &db,
            &GameRecord {
                id: Uuid::new_v4(),
                tournament_id: tournament.id,
                match_id: game_only_match_id,
                pool_id: pool.id,
                variant: pool.variant,
                opening_id: None,
                white_version_id: white.id,
                black_version_id: black.id,
                result: GameResult::WhiteWin,
                termination: arena_core::GameTermination::EngineFailure,
                start_fen: "startpos".to_string(),
                pgn: String::new(),
                moves_uci: vec!["e2e4".to_string()],
                white_time_left_ms: pool.time_control.initial_ms,
                black_time_left_ms: pool.time_control.initial_ms,
                logs: Vec::new(),
                started_at: Utc::now(),
                completed_at: Utc::now(),
            },
        )
        .await
        .unwrap();

        let checkpoint_match_id = Uuid::new_v4();
        insert_match_series(
            &db,
            &MatchSeries {
                id: checkpoint_match_id,
                tournament_id: tournament.id,
                pool_id: pool.id,
                round_index: 3,
                white_version_id: white.id,
                black_version_id: black.id,
                opening_id: None,
                game_index: 1,
                status: MatchStatus::Running,
                created_at: Utc::now(),
            },
        )
        .await
        .unwrap();
        crate::storage::upsert_live_runtime_checkpoint(
            &db,
            &arena_core::LiveRuntimeCheckpoint {
                match_id: checkpoint_match_id,
                seq: 57,
                status: LiveStatus::Finished,
                result: arena_core::LiveResult::WhiteWin,
                termination: arena_core::LiveTermination::EngineFailure,
                start_fen: "4r1k1/pp2rp1p/2pbn2q/Q2p4/3P4/1R1BP2P/PPP2PP1/1R5K w - - 0 29"
                    .to_string(),
                fen: "4r1k1/pp2rp1p/2pbn2q/Q2p4/3P4/1R1BP2P/PPP2PP1/1R5K w - - 0 29".to_string(),
                moves: vec!["e1a5".to_string()],
                white_remaining_ms: 22_000,
                black_remaining_ms: 22_000,
                side_to_move: arena_core::ProtocolLiveSide::None,
                turn_started_server_unix_ms: Utc::now().timestamp_millis(),
                updated_at: Utc::now(),
            },
        )
        .await
        .unwrap();

        let updated = cleanup_stale_match_statuses(&db_url).await.unwrap();
        assert_eq!(updated, 2);
        assert_eq!(
            crate::storage::get_match_series(&db, game_only_match_id)
                .await
                .unwrap()
                .status,
            MatchStatus::Completed
        );
        assert_eq!(
            crate::storage::get_match_series(&db, checkpoint_match_id)
                .await
                .unwrap()
                .status,
            MatchStatus::Completed
        );

        let _ = std::fs::remove_file(temp_path);
    }

    #[tokio::test]
    async fn cleanup_stale_match_statuses_terminalizes_orphaned_rows() {
        let temp_path = std::env::temp_dir().join(format!("mlchess-cleanup-{}.db", Uuid::new_v4()));
        let db_path = temp_path.to_string_lossy().replace('\\', "/");
        let db_url = if db_path.starts_with('/') {
            format!("sqlite://{db_path}")
        } else {
            format!("sqlite:///{db_path}")
        };
        let db_options = db_url
            .parse::<SqliteConnectOptions>()
            .unwrap()
            .create_if_missing(true)
            .foreign_keys(true);
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(db_options)
            .await
            .unwrap();
        init_db(&db).await.unwrap();
        let setup_registry = SetupRegistryCache::default();
        sync_setup_registry_if_changed(&db, &setup_registry)
            .await
            .unwrap();

        let pool = list_pools(&db).await.unwrap().remove(0);
        let mut versions = list_agent_versions(&db, None).await.unwrap();
        let white = versions.remove(0);
        let black = versions.remove(0);
        let created_at = Utc::now() - chrono::Duration::seconds(31);
        let tournament = Tournament {
            id: Uuid::new_v4(),
            name: "orphaned".to_string(),
            kind: TournamentKind::RoundRobin,
            pool_id: pool.id,
            participant_version_ids: vec![white.id, black.id],
            worker_count: 1,
            games_per_pairing: 1,
            status: TournamentStatus::Running,
            created_at,
            started_at: Some(created_at),
            completed_at: None,
        };
        insert_tournament(&db, &tournament).await.unwrap();

        let running_match_id = Uuid::new_v4();
        insert_match_series(
            &db,
            &MatchSeries {
                id: running_match_id,
                tournament_id: tournament.id,
                pool_id: pool.id,
                round_index: 0,
                white_version_id: white.id,
                black_version_id: black.id,
                opening_id: None,
                game_index: 0,
                status: MatchStatus::Running,
                created_at,
            },
        )
        .await
        .unwrap();
        let pending_match_id = Uuid::new_v4();
        insert_match_series(
            &db,
            &MatchSeries {
                id: pending_match_id,
                tournament_id: tournament.id,
                pool_id: pool.id,
                round_index: 0,
                white_version_id: black.id,
                black_version_id: white.id,
                opening_id: None,
                game_index: 1,
                status: MatchStatus::Pending,
                created_at,
            },
        )
        .await
        .unwrap();

        let updated = cleanup_stale_match_statuses(&db_url).await.unwrap();
        assert_eq!(updated, 3);
        assert_eq!(
            crate::storage::get_match_series(&db, running_match_id)
                .await
                .unwrap()
                .status,
            MatchStatus::Failed
        );
        assert_eq!(
            crate::storage::get_match_series(&db, pending_match_id)
                .await
                .unwrap()
                .status,
            MatchStatus::Skipped
        );
        assert_eq!(
            crate::storage::get_tournament(&db, tournament.id)
                .await
                .unwrap()
                .status,
            TournamentStatus::Failed
        );

        let _ = std::fs::remove_file(temp_path);
    }

    #[tokio::test]
    async fn cleanup_stale_match_statuses_fails_stale_zero_match_tournaments() {
        let temp_path = std::env::temp_dir().join(format!("mlchess-cleanup-{}.db", Uuid::new_v4()));
        let db_path = temp_path.to_string_lossy().replace('\\', "/");
        let db_url = if db_path.starts_with('/') {
            format!("sqlite://{db_path}")
        } else {
            format!("sqlite:///{db_path}")
        };
        let db_options = db_url
            .parse::<SqliteConnectOptions>()
            .unwrap()
            .create_if_missing(true)
            .foreign_keys(true);
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(db_options)
            .await
            .unwrap();
        init_db(&db).await.unwrap();
        let setup_registry = SetupRegistryCache::default();
        sync_setup_registry_if_changed(&db, &setup_registry)
            .await
            .unwrap();

        let pool = list_pools(&db).await.unwrap().remove(0);
        let mut versions = list_agent_versions(&db, None).await.unwrap();
        let white = versions.remove(0);
        let black = versions.remove(0);
        let created_at = Utc::now() - chrono::Duration::seconds(31);
        let tournament = Tournament {
            id: Uuid::new_v4(),
            name: "ghost".to_string(),
            kind: TournamentKind::RoundRobin,
            pool_id: pool.id,
            participant_version_ids: vec![white.id, black.id],
            worker_count: 1,
            games_per_pairing: 1,
            status: TournamentStatus::Running,
            created_at,
            started_at: Some(created_at),
            completed_at: None,
        };
        insert_tournament(&db, &tournament).await.unwrap();

        let updated = cleanup_stale_match_statuses(&db_url).await.unwrap();
        assert_eq!(updated, 1);
        let refreshed = crate::storage::get_tournament(&db, tournament.id)
            .await
            .unwrap();
        assert_eq!(refreshed.status, TournamentStatus::Failed);

        let _ = std::fs::remove_file(temp_path);
    }
}
