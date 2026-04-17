use std::path::PathBuf;

use anyhow::{Context, Result};
use axum::{Json, Router, middleware, routing::get};
use serde_json::{Value, json};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::info;

use crate::{
    api,
    bootstrap::{
        middleware::request_context_middleware, reconciliation::reconcile_history_statuses,
        restore::restore_live_runtime,
    },
    db::init_db,
    live::LiveMatchStore,
    registry::{SetupRegistryCache, sync_setup_registry_if_changed},
    state::{AppState, HumanGameStore, LiveMetricsStore, TournamentCoordinator},
};
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
        debug_reports_dir: std::env::current_dir()?.join("debug-reports"),
        frontend_dist: frontend_dist.clone(),
        setup_registry,
    };
    restore_live_runtime(&state).await?;
    let reconciled = reconcile_history_statuses(&state.db).await?;
    if reconciled > 0 {
        info!(
            updated_rows = reconciled,
            "reconciled stale tournament history rows"
        );
    }
    let app = build_app(state);
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    info!("arena server listening on http://{bind_addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn build_app(state: AppState) -> Router {
    let api = api::router().layer(middleware::from_fn_with_state(
        state.clone(),
        request_context_middleware,
    ));
    let mut app = Router::new()
        .nest("/api", api)
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

pub(crate) fn workspace_root() -> PathBuf {
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
