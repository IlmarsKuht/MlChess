mod api;
mod db;
mod gameplay;
mod live;
mod orchestration;
mod presentation;
mod rating;
mod registry;
mod registry_loader;
mod registry_simple_toml;
mod registry_sync;
mod state;
mod storage;

use std::path::PathBuf;

use anyhow::{Context, Result};
use arena_core::{LiveResult, LiveStatus, LiveTermination, ProtocolLiveSide};
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{MatchedPath, Request},
    http::{HeaderName, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use db::init_db;
use live::LiveMatchStore;
use orchestration::{restore_engine_game, restore_human_game};
use presentation::resolve_match_lifecycle;
use registry::{SetupRegistryCache, sync_setup_registry_if_changed};
use serde_json::{Value, json};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use state::{
    AppState, HumanGameStore, LiveMetricsStore, RequestContext, RequestJournalEntry,
    TournamentCoordinator,
};
use tower::ServiceBuilder;
use tower_http::{cors::CorsLayer, services::ServeDir};
use tracing::{error, info, warn};
use uuid::Uuid;

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
        debug_reports_dir: std::env::current_dir()?.join("debug-reports"),
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

pub async fn cleanup_stale_match_statuses(db_url: &str) -> Result<u64> {
    let db_options = db_url
        .parse::<SqliteConnectOptions>()
        .with_context(|| format!("failed to parse sqlite connection string {db_url}"))?
        .create_if_missing(true)
        .foreign_keys(true);
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(db_options)
        .await
        .with_context(|| format!("failed to connect to {db_url}"))?;
    sqlx::query("PRAGMA foreign_keys = ON").execute(&db).await?;
    init_db(&db).await?;

    let match_series = crate::storage::list_match_series(&db, None).await?;
    let games = crate::storage::list_games(&db, None, None).await?;
    let checkpoints = crate::storage::list_live_runtime_checkpoints(&db, None).await?;
    let game_id_by_match_id = games
        .into_iter()
        .map(|game| (game.match_id, game.id))
        .collect::<std::collections::HashMap<_, _>>();
    let checkpoint_by_match_id = checkpoints
        .into_iter()
        .map(|checkpoint| (checkpoint.match_id, checkpoint))
        .collect::<std::collections::HashMap<_, _>>();

    let mut updated = 0_u64;
    for series in match_series {
        let (resolved_status, _, _) = resolve_match_lifecycle(
            &series,
            game_id_by_match_id.get(&series.id).copied(),
            checkpoint_by_match_id.get(&series.id),
        );
        if resolved_status == series.status {
            continue;
        }
        crate::storage::update_match_series_status(&db, series.id, resolved_status).await?;
        updated += 1;
    }
    Ok(updated)
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

async fn request_context_middleware(
    axum::extract::State(state): axum::extract::State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let started_at = chrono::Utc::now();
    let method = request.method().as_str().to_string();
    let matched_path = request
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str)
        .unwrap_or_else(|| request.uri().path())
        .to_string();
    let request_id =
        request_header_uuid(request.headers(), "x-request-id").unwrap_or_else(Uuid::new_v4);
    let context = RequestContext {
        request_id,
        client_action_id: request_header_uuid(request.headers(), "x-client-action-id"),
        client_route: request
            .headers()
            .get("x-client-route")
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned),
        client_ts: request
            .headers()
            .get("x-client-ts")
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned),
        method: method.clone(),
        route: matched_path.clone(),
    };
    request.extensions_mut().insert(context.clone());
    let (match_id, tournament_id, game_id) = infer_entity_ids(request.uri().path());

    let mut response = next.run(request).await;
    response.headers_mut().insert(
        HeaderName::from_static("x-request-id"),
        HeaderValue::from_str(&request_id.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("invalid-request-id")),
    );
    response = enrich_error_response(response, request_id).await;

    let completed_at = chrono::Utc::now();
    let status_code = response.status().as_u16();
    let error_text = response.extensions().get::<String>().cloned().or_else(|| {
        (status_code >= 400).then(|| format!("request failed with status {status_code}"))
    });
    let duration_ms = (completed_at - started_at).num_milliseconds();

    let journal = RequestJournalEntry {
        request_id,
        client_action_id: context.client_action_id,
        client_route: context.client_route.clone(),
        client_ts: context.client_ts.clone(),
        method,
        route: matched_path,
        status_code,
        match_id,
        tournament_id,
        game_id,
        started_at,
        completed_at,
        duration_ms,
        error_text,
    };
    if let Err(err) = crate::storage::insert_request_journal_entry(&state.db, &journal).await {
        warn!(request_id = %request_id, "failed to persist request journal entry: {err:#}");
    }
    info!(
        request_id = %request_id,
        client_action_id = ?context.client_action_id,
        client_route = ?context.client_route,
        status_code,
        route = %journal.route,
        match_id = ?journal.match_id,
        tournament_id = ?journal.tournament_id,
        game_id = ?journal.game_id,
        "handled api request"
    );
    response
}

async fn enrich_error_response(response: Response, request_id: Uuid) -> Response {
    if response.status().is_success() {
        return response;
    }
    let (parts, body) = response.into_parts();
    let bytes = match to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(_) => return Response::from_parts(parts, Body::empty()),
    };
    let mut payload = serde_json::from_slice::<Value>(&bytes)
        .unwrap_or_else(|_| json!({ "error": "request failed" }));
    if payload.get("request_id").is_none() {
        payload["request_id"] = json!(request_id);
    }
    let mut rebuilt = Response::from_parts(parts, Body::from(payload.to_string()));
    rebuilt.headers_mut().insert(
        HeaderName::from_static("content-type"),
        HeaderValue::from_static("application/json"),
    );
    if let Some(error_text) = payload.get("error").and_then(Value::as_str) {
        rebuilt.extensions_mut().insert(error_text.to_string());
    }
    rebuilt
}

fn request_header_uuid(headers: &axum::http::HeaderMap, name: &'static str) -> Option<Uuid> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
}

fn infer_entity_ids(path: &str) -> (Option<Uuid>, Option<Uuid>, Option<Uuid>) {
    let segments: Vec<_> = path.trim_matches('/').split('/').collect();
    let mut match_id = None;
    let mut tournament_id = None;
    let mut game_id = None;
    for window in segments.windows(2) {
        let Some(id) = Uuid::parse_str(window[1]).ok() else {
            continue;
        };
        match window[0] {
            "matches" => match_id = Some(id),
            "tournaments" => tournament_id = Some(id),
            "games" => game_id = Some(id),
            _ => {}
        }
    }
    (match_id, tournament_id, game_id)
}

async fn restore_live_runtime(state: &AppState) -> Result<()> {
    let checkpoints =
        crate::storage::list_live_runtime_checkpoints(&state.db, Some(LiveStatus::Running)).await?;
    let human_player = crate::storage::ensure_human_player(&state.db).await?;
    state.live_metrics.restored_matches.store(
        checkpoints.len() as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
    info!(
        restored_matches = checkpoints.len(),
        "restoring live runtime checkpoints"
    );
    for checkpoint in checkpoints {
        let match_series =
            match crate::storage::get_match_series(&state.db, checkpoint.match_id).await {
                Ok(value) => value,
                Err(err) => {
                    error!(
                        "failed to load match series for restored live match {}: {err}",
                        checkpoint.match_id
                    );
                    continue;
                }
            };
        state
            .live_matches
            .bootstrap_from_db(&state.db, checkpoint.match_id)
            .await?;
        if match_series.white_version_id == human_player.id
            || match_series.black_version_id == human_player.id
        {
            if let Err(err) = restore_human_game(state, checkpoint.clone()).await {
                error!(
                    "failed to restore human live match {}: {err}",
                    checkpoint.match_id
                );
                fail_closed_live_match(state, &match_series, checkpoint.clone()).await?;
            }
        } else {
            if let Err(err) = restore_engine_game(state, checkpoint.clone()).await {
                error!(
                    "failed to restore engine live match {}: {err}",
                    checkpoint.match_id
                );
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
    let event = arena_core::LiveEventEnvelope::GameFinished(live::game_finished_from_checkpoint(
        &checkpoint,
    ));
    let mut tx = state.db.begin().await?;
    crate::storage::upsert_live_runtime_checkpoint_tx(&mut tx, &checkpoint).await?;
    crate::storage::insert_live_runtime_event_tx(&mut tx, &event).await?;
    crate::storage::update_match_series_status_tx(
        &mut tx,
        match_series.id,
        arena_core::MatchStatus::Failed,
    )
    .await?;
    crate::storage::update_tournament_status_tx(
        &mut tx,
        match_series.tournament_id,
        arena_core::TournamentStatus::Failed,
        None,
        Some(chrono::Utc::now()),
    )
    .await?;
    tx.commit().await?;
    live::publish_transient_with_metrics(
        &state.live_matches,
        Some(&state.live_metrics),
        checkpoint,
        event,
    )
    .await;
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
    use crate::storage::{
        insert_game, insert_match_series, insert_tournament, list_agent_versions, list_agents,
        list_games, list_match_series, list_pools, list_tournaments, load_aggregate_leaderboard,
    };
    use arena_core::{
        GameRecord, GameResult, GameTermination, MatchSeries, MatchStatus, Tournament,
        TournamentKind, TournamentStatus,
    };
    use axum::body::Body;
    use chrono::Utc;
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
}
