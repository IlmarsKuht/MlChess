mod api;
mod registry;
mod registry_loader;
mod registry_sync;
mod registry_simple_toml;
mod db;
mod gameplay;
mod live;
mod orchestration;
mod presentation;
mod rating;
mod state;
mod storage;

use std::path::PathBuf;

use anyhow::{Context, Result};
use axum::{Json, Router, http::StatusCode, response::{IntoResponse, Response}, routing::get};
use db::init_db;
use registry::{SetupRegistryCache, sync_setup_registry_if_changed};
use serde_json::{Value, json};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::{error, info};
use live::LiveMatchStore;
use orchestration::{restore_engine_game, restore_human_game};
use state::{AppState, HumanGameStore, LiveMetricsStore, TournamentCoordinator};
use arena_core::{LiveResult, LiveStatus, LiveTermination, ProtocolLiveSide};

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

pub async fn run_server(
    db_url: &str,
    bind_addr: &str,
    frontend_dist: Option<PathBuf>,
) -> Result<()> {
    let db_options = db_url
        .parse::<SqliteConnectOptions>()
        .with_context(|| format!("failed to parse sqlite connection string {db_url}"))?
        .create_if_missing(true)
        .foreign_keys(true);
    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(db_options)
        .await
        .with_context(|| format!("failed to connect to {db_url}"))?;
    sqlx::query("PRAGMA foreign_keys = ON").execute(&db).await?;
    init_db(&db).await?;
    let setup_registry = SetupRegistryCache::default();
    sync_setup_registry_if_changed(&db, &setup_registry).await?;

    let state = AppState {
        db,
        coordinator: TournamentCoordinator::default(),
        live_matches: LiveMatchStore::default(),
        live_metrics: LiveMetricsStore::default(),
        human_games: HumanGameStore::default(),
        frontend_dist: frontend_dist.clone(),
        setup_registry,
    };
    restore_live_runtime(&state).await?;
    let app = build_app(state);
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    info!("arena server listening on http://{bind_addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn build_app(state: AppState) -> Router {
    let mut app = Router::new()
        .nest("/api", api::router())
        .layer(ServiceBuilder::new().layer(CorsLayer::permissive()))
        .with_state(state.clone());

    if let Some(frontend_dist) = &state.frontend_dist {
        app = app.fallback_service(ServeDir::new(frontend_dist));
    } else {
        app = app.route("/", get(root_message));
    }

    app
}

async fn restore_live_runtime(state: &AppState) -> Result<()> {
    let checkpoints =
        crate::storage::list_live_runtime_checkpoints(&state.db, Some(LiveStatus::Running)).await?;
    let human_player = crate::storage::ensure_human_player(&state.db).await?;
    state
        .live_metrics
        .restored_matches
        .store(checkpoints.len() as u64, std::sync::atomic::Ordering::Relaxed);
    info!(restored_matches = checkpoints.len(), "restoring live runtime checkpoints");
    for checkpoint in checkpoints {
        let match_series = match crate::storage::get_match_series(&state.db, checkpoint.match_id).await {
            Ok(value) => value,
            Err(err) => {
                error!("failed to load match series for restored live match {}: {err}", checkpoint.match_id);
                continue;
            }
        };
        state
            .live_matches
            .bootstrap_from_db(&state.db, checkpoint.match_id)
            .await?;
        if match_series.white_version_id == human_player.id || match_series.black_version_id == human_player.id {
            if let Err(err) = restore_human_game(state, checkpoint.clone()).await {
                error!("failed to restore human live match {}: {err}", checkpoint.match_id);
                fail_closed_live_match(state, &match_series, checkpoint.clone()).await?;
            }
        } else {
            if let Err(err) = restore_engine_game(state, checkpoint.clone()).await {
                error!("failed to restore engine live match {}: {err}", checkpoint.match_id);
                fail_closed_live_match(state, &match_series, checkpoint.clone()).await?;
            }
        }
    }
    Ok(())
}

async fn fail_closed_live_match(
    state: &AppState,
    match_series: &arena_core::MatchSeries,
    mut checkpoint: arena_core::LiveRuntimeCheckpoint,
) -> Result<()> {
    checkpoint.seq = checkpoint.seq.saturating_add(1);
    checkpoint.status = LiveStatus::Aborted;
    checkpoint.result = LiveResult::None;
    checkpoint.termination = LiveTermination::Abort;
    checkpoint.side_to_move = ProtocolLiveSide::None;
    checkpoint.updated_at = chrono::Utc::now();
    live::publish_with_metrics(
        &state.live_matches,
        &state.db,
        Some(&state.live_metrics),
        checkpoint.clone(),
        arena_core::LiveEventEnvelope::GameFinished(live::game_finished_from_checkpoint(&checkpoint)),
    )
    .await?;
    crate::storage::update_match_series_status(&state.db, match_series.id, arena_core::MatchStatus::Failed).await?;
    crate::storage::update_tournament_status(
        &state.db,
        match_series.tournament_id,
        arena_core::TournamentStatus::Failed,
        None,
        Some(chrono::Utc::now()),
    )
    .await?;
    Ok(())
}

async fn root_message() -> Json<Value> {
    Json(json!({
        "name": "rust-chess-arena",
        "message": "Frontend assets are not being served. Run the Vite app separately during development."
    }))
}

fn workspace_root() -> PathBuf {
    if let Ok(path) = std::env::var("ARENA_WORKSPACE_ROOT") {
        return PathBuf::from(path);
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .map(|path| path.to_path_buf())
        .unwrap_or(manifest_dir)
}
#[cfg(test)]
mod tests {
    use super::*;
    use arena_core::{GameRecord, GameResult, GameTermination, MatchSeries, MatchStatus, Tournament, TournamentKind, TournamentStatus};
    use axum::body::Body;
    use chrono::Utc;
    use tower::ServiceExt;
    use uuid::Uuid;
    use crate::storage::{insert_game, insert_match_series, insert_tournament, list_agent_versions, list_agents, list_games, list_match_series, list_pools, list_tournaments, load_aggregate_leaderboard};

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
                .any(|pool| pool.name == "Starter Standard Pool")
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

        let result = crate::orchestration::create_tournament_run(
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
        crate::storage::insert_tournament(&db, &tournament).await.unwrap();
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
            fen: "4k3/8/8/8/8/8/8/4K3 w - - 0 1".to_string(),
            moves: vec!["e2e4".to_string()],
            white_remaining_ms: 60_000,
            black_remaining_ms: 60_000,
            side_to_move: arena_core::ProtocolLiveSide::White,
            turn_started_server_unix_ms: Utc::now().timestamp_millis(),
            updated_at: Utc::now(),
        };
        crate::storage::upsert_live_runtime_checkpoint(&db, &checkpoint)
            .await
            .unwrap();
        crate::storage::insert_live_runtime_event(
            &db,
            &arena_core::LiveEventEnvelope::Snapshot(crate::live::snapshot_from_checkpoint(
                &arena_core::LiveRuntimeCheckpoint { seq: 1, moves: Vec::new(), ..checkpoint.clone() },
            )),
        )
        .await
        .unwrap();
        crate::storage::insert_live_runtime_event(
            &db,
            &arena_core::LiveEventEnvelope::MoveCommitted(crate::live::move_committed_from_checkpoint(&checkpoint)),
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
}
