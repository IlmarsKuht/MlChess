mod api;
mod registry;
mod registry_loader;
mod registry_sync;
mod registry_simple_toml;
mod db;
mod gameplay;
mod orchestration;
mod presentation;
mod rating;
mod state;
mod storage;

use std::path::PathBuf;

use anyhow::{Context, Result};
use axum::{Json, Router, http::StatusCode, response::{IntoResponse, Response}, routing::get};
use db::{clear_session_event_history, init_db};
use registry::{SetupRegistryCache, sync_setup_registry_if_changed};
use serde_json::{Value, json};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::{error, info};
use state::{AppState, HumanGameStore, LiveGameStore, TournamentCoordinator};

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
    clear_session_event_history(&db).await?;
    let setup_registry = SetupRegistryCache::default();
    sync_setup_registry_if_changed(&db, &setup_registry).await?;

    let state = AppState {
        db,
        coordinator: TournamentCoordinator::default(),
        live_games: LiveGameStore::default(),
        human_games: HumanGameStore::default(),
        frontend_dist: frontend_dist.clone(),
        setup_registry,
    };
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
    use arena_core::{LiveGameState, MatchStatus, TournamentKind, Variant};
    use axum::body::Body;
    use chrono::Utc;
    use tower::ServiceExt;
    use uuid::Uuid;
    use crate::storage::{list_agent_versions, list_agents, list_pools, load_aggregate_leaderboard};

    fn sample_live_state(status: MatchStatus) -> LiveGameState {
        LiveGameState {
            match_id: Uuid::new_v4(),
            tournament_id: Uuid::new_v4(),
            pool_id: Uuid::new_v4(),
            variant: Variant::Standard,
            white_version_id: Uuid::new_v4(),
            black_version_id: Uuid::new_v4(),
            start_fen: "startpos".to_string(),
            current_fen: "startpos".to_string(),
            moves_uci: vec!["e2e4".to_string()],
            white_time_left_ms: 60_000,
            black_time_left_ms: 60_000,
            status,
            result: None,
            termination: None,
            updated_at: Utc::now(),
            live_frames: Vec::new(),
        }
    }

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
            live_games: LiveGameStore::default(),
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
            live_games: LiveGameStore::default(),
            human_games: HumanGameStore::default(),
            frontend_dist: Some(frontend_dist),
            setup_registry,
        });
    }

    #[tokio::test]
    async fn live_game_store_subscribe_receives_current_and_next_state() {
        let store = LiveGameStore::default();
        let running_state = sample_live_state(MatchStatus::Running);
        let match_id = running_state.match_id;
        store.upsert(running_state.clone()).await;

        let (initial, mut receiver) = store.subscribe(match_id).await.unwrap();
        assert_eq!(initial, running_state);

        let mut completed_state = running_state.clone();
        completed_state.status = MatchStatus::Completed;
        completed_state.updated_at = Utc::now();
        store.upsert(completed_state.clone()).await;

        let streamed = receiver.recv().await.unwrap();
        assert_eq!(streamed, completed_state);
    }

    #[tokio::test]
    async fn live_stream_returns_not_found_for_unknown_match() {
        let app = setup_app().await;
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/api/matches/{}/live/stream", Uuid::new_v4()))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
