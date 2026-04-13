use axum::{
    Json, Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{Extension, Path, Query, State},
    routing::{get, post},
};
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use tracing::{info, warn};
use uuid::Uuid;

use crate::live::ReplayResult;
use crate::{
    ApiError,
    orchestration::{
        create_human_game, create_tournament_run, load_human_player_profile,
        resolve_preset_participants, submit_human_move,
    },
    presentation::{
        ApiGameRecord, ApiLeaderboardEntry, ApiMatchSeries, HumanPlayerProfile, ReplayPayload,
        api_game_record, api_leaderboard_entry, api_match_series, participant_for_id,
        version_name_by_id,
    },
    registry::sync_setup_registry_if_changed,
    state::AppState,
    state::{MoveDebugContext, RequestContext},
    storage::{
        ensure_agent_version_exists, ensure_human_player, ensure_pool_exists, get_agent,
        get_agent_version, get_event_preset, get_game, get_match_series, get_opening_suite,
        get_pool, get_request_journal_entry, get_tournament, list_agent_versions, list_agents,
        list_event_presets, list_games, list_match_series, list_opening_suites, list_pools,
        list_recent_request_errors, list_request_journal_for_entities, list_tournaments,
        load_aggregate_leaderboard, load_live_runtime_checkpoint, load_live_runtime_events_since,
        load_pool_leaderboard, load_rating_history, update_tournament_status,
    },
};
use arena_core::{LiveEventEnvelope, LiveMatchSnapshot};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/agents", get(list_agents_handler))
        .route("/agents/{id}", get(get_agent_handler))
        .route("/agents/{id}/versions", get(list_agent_versions_handler))
        .route("/agent-versions/{id}", get(get_agent_version_handler))
        .route("/pools", get(list_pools_handler))
        .route("/pools/{id}", get(get_pool_handler))
        .route("/opening-suites", get(list_opening_suites_handler))
        .route("/opening-suites/{id}", get(get_opening_suite_handler))
        .route("/event-presets", get(list_event_presets_handler))
        .route("/event-presets/{id}", get(get_event_preset_handler))
        .route(
            "/event-presets/{id}/start",
            post(start_event_preset_handler),
        )
        .route("/duels", post(create_live_duel_handler))
        .route("/human-games", post(create_human_game_handler))
        .route("/human-player", get(get_human_player_handler))
        .route("/tournaments", get(list_tournaments_handler))
        .route("/tournaments/{id}", get(get_tournament_handler))
        .route("/tournaments/{id}/stop", post(stop_tournament_handler))
        .route("/matches", get(list_matches_handler))
        .route("/matches/{id}/live", get(get_live_match_handler))
        .route("/live/metrics", get(get_live_metrics_handler))
        .route("/matches/{id}/live/ws", get(websocket_live_match_handler))
        .route(
            "/debug/matches/{id}/bundle",
            get(get_match_debug_bundle_handler),
        )
        .route(
            "/debug/matches/{id}/events",
            get(get_match_debug_events_handler),
        )
        .route(
            "/debug/games/{id}/bundle",
            get(get_game_debug_bundle_handler),
        )
        .route(
            "/debug/tournaments/{id}/bundle",
            get(get_tournament_debug_bundle_handler),
        )
        .route(
            "/debug/request/{request_id}",
            get(get_request_debug_handler),
        )
        .route("/debug/recent-errors", get(get_recent_errors_handler))
        .route("/debug/reports", post(save_debug_report_handler))
        .route("/games", get(list_games_handler))
        .route("/games/{id}", get(get_game_handler))
        .route("/games/{id}/replay", get(get_game_replay_handler))
        .route("/games/{id}/logs", get(get_game_logs_handler))
        .route("/leaderboards", get(get_leaderboard_handler))
        .route("/ratings/history", get(get_rating_history_handler))
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}

#[derive(Debug, Deserialize)]
struct CreateLiveDuelRequest {
    name: String,
    pool_id: Uuid,
    white_version_id: Uuid,
    black_version_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum HumanSideChoice {
    White,
    Black,
    Random,
}

#[derive(Debug, Deserialize)]
struct CreateHumanGameRequest {
    name: String,
    pool_id: Uuid,
    engine_version_id: Uuid,
    human_side: HumanSideChoice,
}

#[derive(Debug, Deserialize)]
struct MatchesQuery {
    tournament_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct GamesQuery {
    tournament_id: Option<Uuid>,
    agent_version_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct LeaderboardQuery {
    pool_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct RatingHistoryQuery {
    pool_id: Option<Uuid>,
    agent_version_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct DebugEventsQuery {
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SaveDebugReportRequest {
    report: Value,
    preferred_filename: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "message_type", rename_all = "snake_case")]
enum LiveWsClientMessage {
    Subscribe {
        last_seq: Option<u64>,
        ws_connection_id: Option<Uuid>,
    },
    SubmitMove {
        intent_id: Option<Uuid>,
        client_action_id: Option<Uuid>,
        ws_connection_id: Option<Uuid>,
        move_uci: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "message_type", rename_all = "snake_case")]
enum LiveWsServerMessage {
    IntentAck {
        match_id: Uuid,
        intent_id: Uuid,
        client_action_id: Option<Uuid>,
        ws_connection_id: Option<Uuid>,
        request_id: Option<Uuid>,
        ack: &'static str,
    },
    Error {
        error: String,
        request_id: Option<Uuid>,
        client_action_id: Option<Uuid>,
        ws_connection_id: Option<Uuid>,
    },
}

async fn sync_registry(state: &AppState) -> Result<(), ApiError> {
    sync_setup_registry_if_changed(&state.db, &state.setup_registry)
        .await
        .map_err(ApiError::Internal)
}

async fn list_agents_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<arena_core::Agent>>, ApiError> {
    sync_registry(&state).await?;
    Ok(Json(list_agents(&state.db).await?))
}

async fn get_agent_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<arena_core::Agent>, ApiError> {
    sync_registry(&state).await?;
    get_agent(&state.db, id).await.map(Json)
}

async fn list_agent_versions_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Vec<arena_core::AgentVersion>>, ApiError> {
    sync_registry(&state).await?;
    Ok(Json(list_agent_versions(&state.db, Some(agent_id)).await?))
}

async fn get_agent_version_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<arena_core::AgentVersion>, ApiError> {
    sync_registry(&state).await?;
    get_agent_version(&state.db, id).await.map(Json)
}

async fn list_pools_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<arena_core::BenchmarkPool>>, ApiError> {
    sync_registry(&state).await?;
    Ok(Json(list_pools(&state.db).await?))
}

async fn get_pool_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<arena_core::BenchmarkPool>, ApiError> {
    sync_registry(&state).await?;
    get_pool(&state.db, id).await.map(Json)
}

async fn list_opening_suites_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<arena_core::OpeningSuite>>, ApiError> {
    sync_registry(&state).await?;
    Ok(Json(list_opening_suites(&state.db).await?))
}

async fn get_opening_suite_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<arena_core::OpeningSuite>, ApiError> {
    sync_registry(&state).await?;
    get_opening_suite(&state.db, id).await.map(Json)
}

async fn list_event_presets_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<arena_core::EventPreset>>, ApiError> {
    sync_registry(&state).await?;
    Ok(Json(list_event_presets(&state.db).await?))
}

async fn get_event_preset_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<arena_core::EventPreset>, ApiError> {
    sync_registry(&state).await?;
    get_event_preset(&state.db, id).await.map(Json)
}

async fn list_tournaments_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<arena_core::Tournament>>, ApiError> {
    Ok(Json(list_tournaments(&state.db).await?))
}

async fn get_tournament_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<arena_core::Tournament>, ApiError> {
    get_tournament(&state.db, id).await.map(Json)
}

async fn start_event_preset_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    sync_registry(&state).await?;
    let preset = get_event_preset(&state.db, id).await?;
    if !preset.active {
        return Err(ApiError::Conflict("event preset is inactive".to_string()));
    }

    let participant_version_ids = resolve_preset_participants(&state.db, &preset).await?;
    if participant_version_ids.len() < 2 {
        return Err(ApiError::BadRequest(
            "this event preset currently resolves to fewer than two engines".to_string(),
        ));
    }

    let tournament = create_tournament_run(
        &state.db,
        preset.name.clone(),
        preset.kind,
        preset.pool_id,
        participant_version_ids,
        preset.worker_count,
        preset.games_per_pairing,
    )
    .await?;

    let started = state
        .coordinator
        .start(state.clone(), tournament.id)
        .await?;
    if !started {
        return Err(ApiError::Conflict(
            "tournament is already running".to_string(),
        ));
    }
    Ok(Json(json!({
        "started": true,
        "tournament_id": tournament.id,
        "event_preset_id": preset.id
    })))
}

async fn create_live_duel_handler(
    State(state): State<AppState>,
    Json(payload): Json<CreateLiveDuelRequest>,
) -> Result<Json<Value>, ApiError> {
    ensure_pool_exists(&state.db, payload.pool_id).await?;
    ensure_agent_version_exists(&state.db, payload.white_version_id).await?;
    ensure_agent_version_exists(&state.db, payload.black_version_id).await?;

    if payload.white_version_id == payload.black_version_id {
        return Err(ApiError::BadRequest(
            "pick two different engines for the live duel".to_string(),
        ));
    }

    let tournament = create_tournament_run(
        &state.db,
        payload.name,
        arena_core::TournamentKind::RoundRobin,
        payload.pool_id,
        vec![payload.white_version_id, payload.black_version_id],
        1,
        1,
    )
    .await?;

    let started = state
        .coordinator
        .start(state.clone(), tournament.id)
        .await?;
    if !started {
        return Err(ApiError::Conflict(
            "tournament is already running".to_string(),
        ));
    }

    Ok(Json(json!({
        "started": true,
        "tournament_id": tournament.id,
    })))
}

async fn create_human_game_handler(
    State(state): State<AppState>,
    Json(payload): Json<CreateHumanGameRequest>,
) -> Result<Json<Value>, ApiError> {
    let human_plays_white = match payload.human_side {
        HumanSideChoice::White => true,
        HumanSideChoice::Black => false,
        HumanSideChoice::Random => Uuid::new_v4().as_u128() % 2 == 0,
    };
    let (match_id, tournament_id) = create_human_game(
        &state,
        payload.name,
        payload.pool_id,
        payload.engine_version_id,
        human_plays_white,
    )
    .await?;

    Ok(Json(json!({
        "started": true,
        "match_id": match_id,
        "tournament_id": tournament_id,
    })))
}

async fn get_human_player_handler(
    State(state): State<AppState>,
) -> Result<Json<HumanPlayerProfile>, ApiError> {
    Ok(Json(load_human_player_profile(&state.db).await?))
}

async fn stop_tournament_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let stopped = state.coordinator.stop(id).await;
    if !stopped {
        return Err(ApiError::Conflict("tournament is not running".to_string()));
    }
    update_tournament_status(
        &state.db,
        id,
        arena_core::TournamentStatus::Stopped,
        None,
        Some(Utc::now()),
    )
    .await?;
    Ok(Json(json!({ "stopped": true, "tournament_id": id })))
}

async fn list_matches_handler(
    State(state): State<AppState>,
    Query(query): Query<MatchesQuery>,
) -> Result<Json<Vec<ApiMatchSeries>>, ApiError> {
    let versions = list_agent_versions(&state.db, None).await?;
    let version_name_by_id = version_name_by_id(&versions);
    let human_player = ensure_human_player(&state.db).await?;
    let mut matches: Vec<_> = list_match_series(&state.db, query.tournament_id)
        .await?
        .into_iter()
        .map(|series| {
            let interactive = series.white_version_id == human_player.id
                || series.black_version_id == human_player.id;
            api_match_series(&series, &version_name_by_id, &human_player, interactive)
        })
        .collect();
    matches.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(Json(matches))
}

async fn get_live_match_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<LiveMatchSnapshot>, ApiError> {
    state.live_matches.bootstrap_from_db(&state.db, id).await?;
    let snapshot = state
        .live_matches
        .get_snapshot(id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("live state for match {id} not found")))?;
    Ok(Json(snapshot))
}

async fn get_live_metrics_handler(
    State(state): State<AppState>,
) -> Json<crate::state::LiveMetricsSnapshot> {
    Json(state.live_metrics.snapshot())
}

async fn get_match_debug_bundle_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let series = get_match_series(&state.db, id).await?;
    let tournament = get_tournament(&state.db, series.tournament_id).await?;
    let games = list_games(&state.db, Some(series.tournament_id), None).await?;
    let game = games.into_iter().find(|candidate| candidate.match_id == id);
    Ok(Json(
        build_debug_bundle(
            &state,
            Some(series.clone()),
            Some(tournament),
            game,
            Some(id),
            Some(series.tournament_id),
        )
        .await?,
    ))
}

async fn get_game_debug_bundle_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let game = get_game(&state.db, id).await?;
    let series = get_match_series(&state.db, game.match_id).await?;
    let tournament = get_tournament(&state.db, game.tournament_id).await?;
    Ok(Json(
        build_debug_bundle(
            &state,
            Some(series),
            Some(tournament),
            Some(game.clone()),
            Some(game.match_id),
            Some(game.tournament_id),
        )
        .await?,
    ))
}

async fn get_tournament_debug_bundle_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let tournament = get_tournament(&state.db, id).await?;
    let match_series = list_match_series(&state.db, Some(id)).await?;
    let primary_match = match_series.first().cloned();
    let games = list_games(&state.db, Some(id), None).await?;
    let primary_game = primary_match
        .as_ref()
        .and_then(|series| {
            games
                .iter()
                .find(|candidate| candidate.match_id == series.id)
                .cloned()
        })
        .or_else(|| games.first().cloned());
    Ok(Json(
        build_debug_bundle(
            &state,
            primary_match,
            Some(tournament.clone()),
            primary_game,
            None,
            Some(tournament.id),
        )
        .await?,
    ))
}

async fn get_match_debug_events_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<DebugEventsQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(20).min(100);
    let events = load_live_runtime_events_since(&state.db, id, 0).await?;
    let start = events.len().saturating_sub(limit);
    Ok(Json(json!({
        "match_id": id,
        "events": events.into_iter().skip(start).collect::<Vec<_>>(),
    })))
}

async fn get_request_debug_handler(
    State(state): State<AppState>,
    Path(request_id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let entry = get_request_journal_entry(&state.db, request_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("request {request_id} not found")))?;
    Ok(Json(json!({
        "summary": format!("{} {} -> {}", entry.method, entry.route, entry.status_code),
        "request": entry,
    })))
}

async fn get_recent_errors_handler(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let errors = list_recent_request_errors(&state.db, 20).await?;
    Ok(Json(json!({ "errors": errors })))
}

async fn save_debug_report_handler(
    State(state): State<AppState>,
    Json(payload): Json<SaveDebugReportRequest>,
) -> Result<Json<Value>, ApiError> {
    std::fs::create_dir_all(&state.debug_reports_dir)
        .map_err(|err| ApiError::Internal(err.into()))?;
    let report = payload.report;
    let entity_hint = report
        .get("correlation")
        .and_then(|value| {
            value
                .get("match_id")
                .or_else(|| value.get("game_id"))
                .or_else(|| value.get("tournament_id"))
        })
        .and_then(Value::as_str)
        .unwrap_or("general");
    let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%SZ");
    let filename = sanitize_report_filename(payload.preferred_filename.as_deref().unwrap_or(
        &format!("mlchess-bug-report-{timestamp}-{entity_hint}.json"),
    ));
    let path = state.debug_reports_dir.join(filename);
    let serialized =
        serde_json::to_string_pretty(&report).map_err(|err| ApiError::Internal(err.into()))?;
    std::fs::write(&path, serialized).map_err(|err| ApiError::Internal(err.into()))?;
    Ok(Json(json!({
        "saved": true,
        "path": path,
        "filename": path.file_name().and_then(|value| value.to_str()),
    })))
}

fn sanitize_report_filename(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => ch,
            _ => '-',
        })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    let candidate = if trimmed.is_empty() {
        "mlchess-bug-report.json"
    } else {
        trimmed
    };
    if candidate.ends_with(".json") {
        candidate.to_string()
    } else {
        format!("{candidate}.json")
    }
}

async fn websocket_live_match_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Extension(request_context): Extension<RequestContext>,
    Path(id): Path<Uuid>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    state.live_matches.bootstrap_from_db(&state.db, id).await?;
    Ok(ws.on_upgrade(move |socket| handle_live_socket(state, id, socket, request_context)))
}

async fn handle_live_socket(
    state: AppState,
    match_id: Uuid,
    mut socket: WebSocket,
    request_context: RequestContext,
) {
    state
        .live_metrics
        .websocket_connections
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut subscribed = false;
    let mut receiver: Option<tokio::sync::broadcast::Receiver<LiveEventEnvelope>> = None;
    let mut active_ws_connection_id: Option<Uuid> = None;

    loop {
        tokio::select! {
            maybe_message = socket.recv() => {
                let Some(Ok(message)) = maybe_message else {
                    break;
                };
                match message {
                    Message::Text(text) => {
                        let Ok(client_message) = serde_json::from_str::<LiveWsClientMessage>(&text) else {
                            let _ = send_ws_error(
                                &mut socket,
                                "Malformed live websocket message",
                                &request_context,
                                None,
                                active_ws_connection_id,
                            )
                            .await;
                            continue;
                        };
                        match client_message {
                            LiveWsClientMessage::Subscribe { last_seq, ws_connection_id } => {
                                active_ws_connection_id = ws_connection_id.or(active_ws_connection_id);
                                match subscribe_live_socket(&state, match_id, last_seq).await {
                                    Ok((initial_events, next_receiver)) => {
                                        receiver = Some(next_receiver);
                                        subscribed = true;
                                        info!(
                                            request_id = %request_context.request_id,
                                            ws_connection_id = ?active_ws_connection_id,
                                            match_id = %match_id,
                                            last_seq = ?last_seq,
                                            "websocket subscribed to live match"
                                        );
                                        for event in initial_events {
                                            if send_live_event(&mut socket, &event).await.is_err() {
                                                return;
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        state
                                            .live_metrics
                                            .move_intent_errors
                                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                        let _ = send_ws_error(&mut socket, &err.to_string(), &request_context, None, active_ws_connection_id).await;
                                    }
                                }
                            }
                            LiveWsClientMessage::SubmitMove { intent_id, client_action_id, ws_connection_id, move_uci } => {
                                active_ws_connection_id = ws_connection_id.or(active_ws_connection_id);
                                let intent_id = intent_id.unwrap_or_else(Uuid::new_v4);
                                let move_context = MoveDebugContext {
                                    request_id: Some(request_context.request_id),
                                    client_action_id,
                                    ws_connection_id: active_ws_connection_id,
                                    intent_id,
                                    move_uci: move_uci.clone(),
                                };
                                match submit_human_move(state.clone(), match_id, move_context).await {
                                    Ok(ack) => {
                                        if send_ws_json(
                                            &mut socket,
                                            &LiveWsServerMessage::IntentAck {
                                                match_id,
                                                intent_id,
                                                client_action_id,
                                                ws_connection_id: active_ws_connection_id,
                                                request_id: Some(request_context.request_id),
                                                ack,
                                            }
                                        ).await.is_err() {
                                            return;
                                        }
                                    }
                                    Err(err) => {
                                        let _ = send_ws_error(&mut socket, &err.to_string(), &request_context, client_action_id, active_ws_connection_id).await;
                                    }
                                }
                            }
                        }
                    }
                    Message::Ping(payload) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            received = async {
                match receiver.as_mut() {
                    Some(value) => value.recv().await.ok(),
                    None => None,
                }
            }, if subscribed => {
                let Some(event) = received else {
                    continue;
                };
                if send_live_event(&mut socket, &event).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn subscribe_live_socket(
    state: &AppState,
    match_id: Uuid,
    last_seq: Option<u64>,
) -> Result<
    (
        Vec<LiveEventEnvelope>,
        tokio::sync::broadcast::Receiver<LiveEventEnvelope>,
    ),
    ApiError,
> {
    state
        .live_matches
        .bootstrap_from_db(&state.db, match_id)
        .await?;
    let (snapshot, receiver) = state
        .live_matches
        .subscribe(match_id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("live state for match {match_id} not found")))?;
    let initial_events = initial_stream_events(state, match_id, last_seq, snapshot).await;
    Ok((initial_events, receiver))
}

async fn send_live_event(
    socket: &mut WebSocket,
    event: &LiveEventEnvelope,
) -> Result<(), axum::Error> {
    socket
        .send(Message::Text(
            serde_json::to_string(event)
                .expect("event should serialize")
                .into(),
        ))
        .await
}

async fn send_ws_json(
    socket: &mut WebSocket,
    message: &LiveWsServerMessage,
) -> Result<(), axum::Error> {
    socket
        .send(Message::Text(
            serde_json::to_string(message)
                .expect("message should serialize")
                .into(),
        ))
        .await
}

async fn send_ws_error(
    socket: &mut WebSocket,
    error: &str,
    request_context: &RequestContext,
    client_action_id: Option<Uuid>,
    ws_connection_id: Option<Uuid>,
) -> Result<(), axum::Error> {
    send_ws_json(
        socket,
        &LiveWsServerMessage::Error {
            error: error.to_string(),
            request_id: Some(request_context.request_id),
            client_action_id,
            ws_connection_id,
        },
    )
    .await
}

async fn initial_stream_events(
    state: &AppState,
    match_id: Uuid,
    last_seq: Option<u64>,
    initial_snapshot: LiveMatchSnapshot,
) -> Vec<LiveEventEnvelope> {
    match last_seq {
        Some(seq) => match state.live_matches.replay_since(match_id, seq).await {
            Some(ReplayResult::Replay(events)) if !events.is_empty() => {
                state
                    .live_metrics
                    .replay_requests
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                state
                    .live_metrics
                    .replay_events_served
                    .fetch_add(events.len() as u64, std::sync::atomic::Ordering::Relaxed);
                info!(match_id = %match_id, from_seq = seq, replay_count = events.len(), "serving live replay events");
                events
            }
            Some(ReplayResult::Replay(_)) => {
                state
                    .live_metrics
                    .replay_requests
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                state
                    .live_metrics
                    .snapshot_fallbacks
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                info!(match_id = %match_id, from_seq = seq, "replay request already up to date, sending snapshot");
                vec![LiveEventEnvelope::Snapshot(initial_snapshot)]
            }
            Some(ReplayResult::SnapshotRequired) => {
                state
                    .live_metrics
                    .replay_requests
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                state
                    .live_metrics
                    .snapshot_fallbacks
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                warn!(match_id = %match_id, from_seq = seq, "live replay gap exceeded buffer, falling back to snapshot");
                vec![LiveEventEnvelope::Snapshot(initial_snapshot)]
            }
            None => {
                state
                    .live_metrics
                    .replay_requests
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                state
                    .live_metrics
                    .snapshot_fallbacks
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                warn!(match_id = %match_id, from_seq = seq, "live replay state missing, falling back to snapshot");
                vec![LiveEventEnvelope::Snapshot(initial_snapshot)]
            }
        },
        None => match load_live_runtime_events_since(&state.db, match_id, 0).await {
            Ok(events) if !events.is_empty() => {
                state
                    .live_metrics
                    .replay_events_served
                    .fetch_add(events.len() as u64, std::sync::atomic::Ordering::Relaxed);
                info!(match_id = %match_id, replay_count = events.len(), "serving full live history bootstrap");
                events
            }
            Ok(_) => {
                state
                    .live_metrics
                    .snapshot_fallbacks
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                vec![LiveEventEnvelope::Snapshot(initial_snapshot)]
            }
            Err(err) => {
                state
                    .live_metrics
                    .snapshot_fallbacks
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                warn!(match_id = %match_id, "failed to load live history bootstrap: {err:#}");
                vec![LiveEventEnvelope::Snapshot(initial_snapshot)]
            }
        },
    }
}

async fn list_games_handler(
    State(state): State<AppState>,
    Query(query): Query<GamesQuery>,
) -> Result<Json<Vec<ApiGameRecord>>, ApiError> {
    let versions = list_agent_versions(&state.db, None).await?;
    let version_name_by_id = version_name_by_id(&versions);
    let human_player = ensure_human_player(&state.db).await?;
    let games = list_games(&state.db, query.tournament_id, query.agent_version_id).await?;
    Ok(Json(
        games
            .into_iter()
            .map(|game| api_game_record(&game, &version_name_by_id, &human_player))
            .collect(),
    ))
}

async fn get_game_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiGameRecord>, ApiError> {
    let versions = list_agent_versions(&state.db, None).await?;
    let version_name_by_id = version_name_by_id(&versions);
    let human_player = ensure_human_player(&state.db).await?;
    let game = get_game(&state.db, id).await?;
    Ok(Json(api_game_record(
        &game,
        &version_name_by_id,
        &human_player,
    )))
}

async fn get_game_replay_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ReplayPayload>, ApiError> {
    let game = get_game(&state.db, id).await?;
    Ok(Json(ReplayPayload {
        id: game.id,
        variant: game.variant,
        start_fen: game.start_fen,
        pgn: game.pgn,
        moves_uci: game.moves_uci,
        result: game.result,
        termination: game.termination,
    }))
}

async fn get_game_logs_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let game = get_game(&state.db, id).await?;
    Ok(Json(json!({ "id": game.id, "logs": game.logs })))
}

async fn get_leaderboard_handler(
    State(state): State<AppState>,
    Query(query): Query<LeaderboardQuery>,
) -> Result<Json<Vec<ApiLeaderboardEntry>>, ApiError> {
    let versions = list_agent_versions(&state.db, None).await?;
    let version_name_by_id = version_name_by_id(&versions);
    let human_player = ensure_human_player(&state.db).await?;
    let entries = if let Some(pool_id) = query.pool_id {
        load_pool_leaderboard(&state.db, pool_id).await?
    } else {
        load_aggregate_leaderboard(&state.db).await?
    };
    Ok(Json(
        entries
            .into_iter()
            .map(|entry| api_leaderboard_entry(entry, &version_name_by_id, &human_player))
            .collect(),
    ))
}

async fn get_rating_history_handler(
    State(state): State<AppState>,
    Query(query): Query<RatingHistoryQuery>,
) -> Result<Json<Vec<arena_core::RatingSnapshot>>, ApiError> {
    Ok(Json(
        load_rating_history(&state.db, query.pool_id, query.agent_version_id).await?,
    ))
}

async fn build_debug_bundle(
    state: &AppState,
    match_series: Option<arena_core::MatchSeries>,
    tournament: Option<arena_core::Tournament>,
    game: Option<arena_core::GameRecord>,
    match_id: Option<Uuid>,
    tournament_id: Option<Uuid>,
) -> Result<Value, ApiError> {
    let resolved_match_id = match_id
        .or(match_series.as_ref().map(|value| value.id))
        .or(game.as_ref().map(|value| value.match_id));
    let resolved_tournament_id = tournament_id
        .or(tournament.as_ref().map(|value| value.id))
        .or(match_series.as_ref().map(|value| value.tournament_id))
        .or(game.as_ref().map(|value| value.tournament_id));
    let resolved_game_id = game.as_ref().map(|value| value.id);

    let checkpoint = match resolved_match_id {
        Some(value) => load_live_runtime_checkpoint(&state.db, value).await?,
        None => None,
    };
    let recent_live_events = match resolved_match_id {
        Some(value) => {
            let events = load_live_runtime_events_since(&state.db, value, 0).await?;
            let start = events.len().saturating_sub(20);
            events.into_iter().skip(start).collect::<Vec<_>>()
        }
        None => Vec::new(),
    };
    let recent_requests = list_request_journal_for_entities(
        &state.db,
        resolved_match_id,
        resolved_tournament_id,
        resolved_game_id,
        20,
    )
    .await?;
    let recent_errors = list_recent_request_errors(&state.db, 20)
        .await?
        .into_iter()
        .filter(|entry| {
            resolved_match_id
                .map(|value| entry.match_id == Some(value))
                .unwrap_or(false)
                || resolved_tournament_id
                    .map(|value| entry.tournament_id == Some(value))
                    .unwrap_or(false)
                || resolved_game_id
                    .map(|value| entry.game_id == Some(value))
                    .unwrap_or(false)
        })
        .take(10)
        .collect::<Vec<_>>();

    let recent_persisted_logs = game
        .as_ref()
        .map(|value| {
            let start = value.logs.len().saturating_sub(30);
            value.logs.iter().skip(start).cloned().collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let versions = list_agent_versions(&state.db, None).await?;
    let version_names = version_name_by_id(&versions);
    let human_player = ensure_human_player(&state.db).await?;
    let participants = json!({
        "white": match_series.as_ref().map(|series| participant_for_id(series.white_version_id, &version_names, &human_player)),
        "black": match_series.as_ref().map(|series| participant_for_id(series.black_version_id, &version_names, &human_player)),
    });
    let summary = summarize_bundle(
        match_series.as_ref(),
        tournament.as_ref(),
        game.as_ref(),
        checkpoint.as_ref(),
        &recent_requests,
        &recent_live_events,
    );
    let failure_analysis = summarize_failures(
        checkpoint.as_ref(),
        &recent_requests,
        &recent_errors,
        &recent_live_events,
    );

    Ok(json!({
        "summary": summary,
        "failure_analysis": failure_analysis,
        "entity": {
            "match": match_series,
            "tournament": tournament,
            "game": game,
        },
        "related": {
            "match_id": resolved_match_id,
            "tournament_id": resolved_tournament_id,
            "game_id": resolved_game_id,
        },
        "checkpoint": checkpoint,
        "recent_live_events": recent_live_events,
        "recent_persisted_logs": recent_persisted_logs,
        "recent_requests": recent_requests,
        "recent_errors": recent_errors,
        "live_metrics": state.live_metrics.snapshot(),
        "participants": participants,
        "correlation": {
            "request_ids": recent_requests.iter().map(|entry| entry.request_id).collect::<Vec<_>>(),
            "client_action_ids": recent_requests.iter().filter_map(|entry| entry.client_action_id).collect::<Vec<_>>(),
        }
    }))
}

fn summarize_bundle(
    match_series: Option<&arena_core::MatchSeries>,
    tournament: Option<&arena_core::Tournament>,
    game: Option<&arena_core::GameRecord>,
    checkpoint: Option<&arena_core::LiveRuntimeCheckpoint>,
    requests: &[crate::state::RequestJournalEntry],
    events: &[LiveEventEnvelope],
) -> String {
    let mut parts = Vec::new();
    if let Some(series) = match_series {
        parts.push(format!("match {} is {:?}", series.id, series.status));
    }
    if let Some(tournament) = tournament {
        parts.push(
            format!("tournament {:?} ", tournament.status)
                .trim()
                .to_string(),
        );
    }
    if let Some(checkpoint) = checkpoint {
        parts.push(format!(
            "live seq {} {:?}",
            checkpoint.seq, checkpoint.status
        ));
    }
    if let Some(game) = game {
        parts.push(format!(
            "game ended {:?} via {:?}",
            game.result, game.termination
        ));
    }
    if let Some(request) = requests.first() {
        parts.push(format!(
            "latest request {} {}",
            request.method, request.status_code
        ));
    }
    if !events.is_empty() {
        parts.push(format!("{} recent live events", events.len()));
    }
    if parts.is_empty() {
        "debug bundle assembled".to_string()
    } else {
        parts.join(" | ")
    }
}

fn summarize_failures(
    checkpoint: Option<&arena_core::LiveRuntimeCheckpoint>,
    requests: &[crate::state::RequestJournalEntry],
    recent_errors: &[crate::state::RequestJournalEntry],
    events: &[LiveEventEnvelope],
) -> Value {
    let latest_error = recent_errors.first();
    let failure_class = if latest_error
        .map(|entry| entry.route.contains("/live"))
        .unwrap_or(false)
    {
        "live_api"
    } else if latest_error
        .map(|entry| entry.route.contains("/debug"))
        .unwrap_or(false)
    {
        "debug_endpoint"
    } else if checkpoint.is_some() && events.is_empty() && !requests.is_empty() {
        "live_runtime_visibility"
    } else {
        "unknown"
    };
    let suspected_files: Vec<PathBuf> = match failure_class {
        "live_api" | "live_runtime_visibility" => vec![
            PathBuf::from("crates/arena-server/src/api.rs"),
            PathBuf::from("crates/arena-server/src/live.rs"),
            PathBuf::from("frontend/src/app/live.ts"),
        ],
        "debug_endpoint" => vec![
            PathBuf::from("crates/arena-server/src/api.rs"),
            PathBuf::from("frontend/src/App.tsx"),
        ],
        _ => vec![PathBuf::from("crates/arena-server/src/api.rs")],
    };
    json!({
        "failure_class": failure_class,
        "confidence": if latest_error.is_some() { "medium" } else { "low" },
        "signals": {
            "request_count": requests.len(),
            "error_count": recent_errors.len(),
            "live_event_count": events.len(),
            "has_checkpoint": checkpoint.is_some(),
        },
        "latest_error": latest_error,
        "next_debug_targets": suspected_files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::init_db,
        live::{move_committed_from_checkpoint, snapshot_from_checkpoint},
        registry::{SetupRegistryCache, sync_setup_registry_if_changed},
        state::{HumanGameStore, TournamentCoordinator},
        storage::{insert_live_runtime_event, upsert_live_runtime_checkpoint},
    };
    use arena_core::{
        LiveResult, LiveRuntimeCheckpoint, LiveStatus, LiveTermination, ProtocolLiveSide,
    };
    use axum::body::Body;
    use axum::http::StatusCode;
    use chrono::Utc;
    use sqlx::sqlite::SqlitePoolOptions;
    use tower::ServiceExt;

    async fn setup_state() -> AppState {
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
            debug_reports_dir: std::env::temp_dir()
                .join(format!("mlchess-debug-reports-{}", Uuid::new_v4())),
            frontend_dist: None,
            setup_registry,
        }
    }

    fn checkpoint(match_id: Uuid, seq: u64, moves: &[&str]) -> LiveRuntimeCheckpoint {
        LiveRuntimeCheckpoint {
            match_id,
            seq,
            status: LiveStatus::Running,
            result: LiveResult::None,
            termination: LiveTermination::None,
            fen: if moves.is_empty() {
                "4k3/8/8/8/8/8/8/4K3 w - - 0 1".to_string()
            } else {
                "4k3/8/8/8/8/8/8/4K3 b - - 0 1".to_string()
            },
            moves: moves.iter().map(|value| (*value).to_string()).collect(),
            white_remaining_ms: 60_000,
            black_remaining_ms: 60_000,
            side_to_move: if moves.is_empty() {
                ProtocolLiveSide::White
            } else {
                ProtocolLiveSide::Black
            },
            turn_started_server_unix_ms: Utc::now().timestamp_millis(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn initial_stream_events_prefers_replay_when_available() {
        let state = setup_state().await;
        let match_id = Uuid::new_v4();
        let first = checkpoint(match_id, 1, &[]);
        let second = checkpoint(match_id, 2, &["e2e4"]);
        upsert_live_runtime_checkpoint(&state.db, &second)
            .await
            .unwrap();
        insert_live_runtime_event(
            &state.db,
            &LiveEventEnvelope::Snapshot(snapshot_from_checkpoint(&first)),
        )
        .await
        .unwrap();
        insert_live_runtime_event(
            &state.db,
            &LiveEventEnvelope::MoveCommitted(move_committed_from_checkpoint(&second)),
        )
        .await
        .unwrap();
        state
            .live_matches
            .bootstrap_from_db(&state.db, match_id)
            .await
            .unwrap();

        let events =
            initial_stream_events(&state, match_id, Some(1), snapshot_from_checkpoint(&second))
                .await;
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], LiveEventEnvelope::MoveCommitted(_)));
    }

    #[tokio::test]
    async fn initial_stream_events_falls_back_to_snapshot_after_gap() {
        let state = setup_state().await;
        let match_id = Uuid::new_v4();
        let latest = checkpoint(match_id, 200, &["e2e4"]);
        upsert_live_runtime_checkpoint(&state.db, &latest)
            .await
            .unwrap();
        for seq in 73..=200 {
            let event_checkpoint = LiveRuntimeCheckpoint {
                seq,
                ..latest.clone()
            };
            let event = if seq == 73 {
                LiveEventEnvelope::Snapshot(snapshot_from_checkpoint(&event_checkpoint))
            } else {
                LiveEventEnvelope::MoveCommitted(move_committed_from_checkpoint(&event_checkpoint))
            };
            insert_live_runtime_event(&state.db, &event).await.unwrap();
        }
        state
            .live_matches
            .bootstrap_from_db(&state.db, match_id)
            .await
            .unwrap();

        let events =
            initial_stream_events(&state, match_id, Some(1), snapshot_from_checkpoint(&latest))
                .await;
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], LiveEventEnvelope::Snapshot(_)));
    }

    #[tokio::test]
    async fn initial_stream_events_bootstraps_with_full_history_for_new_watcher() {
        let state = setup_state().await;
        let match_id = Uuid::new_v4();
        let first = checkpoint(match_id, 1, &[]);
        let second = checkpoint(match_id, 2, &["e2e4"]);
        upsert_live_runtime_checkpoint(&state.db, &second)
            .await
            .unwrap();
        insert_live_runtime_event(
            &state.db,
            &LiveEventEnvelope::Snapshot(snapshot_from_checkpoint(&first)),
        )
        .await
        .unwrap();
        insert_live_runtime_event(
            &state.db,
            &LiveEventEnvelope::MoveCommitted(move_committed_from_checkpoint(&second)),
        )
        .await
        .unwrap();
        state
            .live_matches
            .bootstrap_from_db(&state.db, match_id)
            .await
            .unwrap();

        let events =
            initial_stream_events(&state, match_id, None, snapshot_from_checkpoint(&second)).await;
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], LiveEventEnvelope::Snapshot(_)));
        assert!(matches!(events[1], LiveEventEnvelope::MoveCommitted(_)));
    }

    #[tokio::test]
    async fn error_payload_echoes_request_id() {
        let state = setup_state().await;
        let app = crate::build_app(state);
        let request_id = Uuid::new_v4();
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/api/games/{}", Uuid::new_v4()))
                    .header("x-request-id", request_id.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            payload
                .get("request_id")
                .and_then(serde_json::Value::as_str),
            Some(request_id.to_string().as_str())
        );
    }

    #[tokio::test]
    async fn match_debug_bundle_includes_recent_requests() {
        let state = setup_state().await;
        let app = crate::build_app(state.clone());
        let pool_id = crate::storage::list_pools(&state.db)
            .await
            .unwrap()
            .remove(0)
            .id;
        let mut versions = crate::storage::list_agent_versions(&state.db, None)
            .await
            .unwrap();
        let white = versions.remove(0);
        let black = versions.remove(0);
        let tournament = arena_core::Tournament {
            id: Uuid::new_v4(),
            name: "debug bundle".to_string(),
            kind: arena_core::TournamentKind::RoundRobin,
            pool_id,
            participant_version_ids: vec![white.id, black.id],
            worker_count: 1,
            games_per_pairing: 1,
            status: arena_core::TournamentStatus::Running,
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            completed_at: None,
        };
        crate::storage::insert_tournament(&state.db, &tournament)
            .await
            .unwrap();
        let series = arena_core::MatchSeries {
            id: Uuid::new_v4(),
            tournament_id: tournament.id,
            pool_id,
            round_index: 0,
            white_version_id: white.id,
            black_version_id: black.id,
            opening_id: None,
            game_index: 0,
            status: arena_core::MatchStatus::Running,
            created_at: Utc::now(),
        };
        crate::storage::insert_match_series(&state.db, &series)
            .await
            .unwrap();
        let checkpoint = checkpoint(series.id, 1, &[]);
        crate::storage::upsert_live_runtime_checkpoint(&state.db, &checkpoint)
            .await
            .unwrap();
        state
            .live_matches
            .bootstrap_from_db(&state.db, series.id)
            .await
            .unwrap();

        let request_id = Uuid::new_v4();
        let _ = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/api/matches/{}/live", series.id))
                    .header("x-request-id", request_id.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/api/debug/matches/{}/bundle", series.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let requests = payload
            .get("recent_requests")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(!requests.is_empty());
        assert!(requests.iter().any(|entry| {
            entry.get("request_id").and_then(serde_json::Value::as_str)
                == Some(&request_id.to_string())
        }));
        let first = requests.first().expect("expected recent request");
        assert!(first.get("duration_ms").is_some());
    }

    #[tokio::test]
    async fn request_journal_persists_client_route_context() {
        let state = setup_state().await;
        let app = crate::build_app(state.clone());
        let request_id = Uuid::new_v4();
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/leaderboards")
                    .header("x-request-id", request_id.to_string())
                    .header("x-client-route", "/#/watch/test-match")
                    .header("x-client-ts", "2026-04-12T17:44:23.450Z")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let entries = crate::storage::list_recent_request_errors(&state.db, 5)
            .await
            .unwrap();
        assert!(entries.is_empty());

        let leaderboards_request = crate::storage::get_request_journal_entry(&state.db, request_id)
            .await
            .unwrap()
            .expect("request should be stored");
        assert_eq!(
            leaderboards_request.client_route.as_deref(),
            Some("/#/watch/test-match")
        );
        assert_eq!(
            leaderboards_request.client_ts.as_deref(),
            Some("2026-04-12T17:44:23.450Z")
        );
        assert!(leaderboards_request.duration_ms >= 0);
    }

    #[tokio::test]
    async fn save_debug_report_writes_repo_local_file() {
        let state = setup_state().await;
        let reports_dir = state.debug_reports_dir.clone();
        let app = crate::build_app(state);
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/debug/reports")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "preferred_filename": "custom report",
                            "report": {
                                "correlation": { "match_id": Uuid::new_v4() },
                                "report_summary": { "headline": "Test report" }
                            }
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let path = payload
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap();
        assert!(path.contains("custom-report.json"));
        assert!(std::path::Path::new(path).exists());
        assert!(path.starts_with(reports_dir.to_string_lossy().as_ref()));
    }
}
