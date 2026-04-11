use axum::{
    Json, Router,
    extract::{Path, Query, State},
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    routing::{get, post},
};
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use serde_json::{Value, json};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    ApiError,
    orchestration::{
        create_human_game, create_tournament_run, load_human_player_profile,
        resolve_preset_participants, submit_human_move,
    },
    presentation::{
        ApiGameRecord, ApiLeaderboardEntry, ApiMatchSeries, HumanPlayerProfile, ReplayPayload,
        api_game_record, api_leaderboard_entry, api_match_series, version_name_by_id,
    },
    registry::sync_setup_registry_if_changed,
    state::AppState,
    storage::{
        ensure_agent_version_exists, ensure_human_player, ensure_pool_exists, get_agent,
        get_agent_version, get_event_preset, get_game, get_opening_suite, get_pool,
        get_tournament, list_agent_versions, list_agents, list_event_presets, list_games,
        list_match_series, list_opening_suites, list_pools, list_tournaments,
        load_live_runtime_events_since,
        load_aggregate_leaderboard, load_pool_leaderboard, load_rating_history,
        update_tournament_status,
    },
};
use arena_core::{LiveEventEnvelope, LiveMatchSnapshot};
use crate::live::ReplayResult;

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
        .route("/event-presets/{id}/start", post(start_event_preset_handler))
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
#[serde(tag = "message_type", rename_all = "snake_case")]
enum LiveWsClientMessage {
    Subscribe { last_seq: Option<u64> },
    SubmitMove { intent_id: Option<Uuid>, move_uci: String },
}

#[derive(Debug, Serialize)]
#[serde(tag = "message_type", rename_all = "snake_case")]
enum LiveWsServerMessage {
    IntentAck { match_id: Uuid, intent_id: Uuid, ack: &'static str },
    Error { error: String },
}

async fn sync_registry(state: &AppState) -> Result<(), ApiError> {
    sync_setup_registry_if_changed(&state.db, &state.setup_registry)
        .await
        .map_err(ApiError::Internal)
}

async fn list_agents_handler(State(state): State<AppState>) -> Result<Json<Vec<arena_core::Agent>>, ApiError> {
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

    let started = state.coordinator.start(state.clone(), tournament.id).await?;
    if !started {
        return Err(ApiError::Conflict("tournament is already running".to_string()));
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

    let started = state.coordinator.start(state.clone(), tournament.id).await?;
    if !started {
        return Err(ApiError::Conflict("tournament is already running".to_string()));
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
            let interactive =
                series.white_version_id == human_player.id || series.black_version_id == human_player.id;
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

async fn get_live_metrics_handler(State(state): State<AppState>) -> Json<crate::state::LiveMetricsSnapshot> {
    Json(state.live_metrics.snapshot())
}

async fn websocket_live_match_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    state.live_matches.bootstrap_from_db(&state.db, id).await?;
    Ok(ws.on_upgrade(move |socket| handle_live_socket(state, id, socket)))
}

async fn handle_live_socket(state: AppState, match_id: Uuid, mut socket: WebSocket) {
    state
        .live_metrics
        .websocket_connections
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut subscribed = false;
    let mut receiver: Option<tokio::sync::broadcast::Receiver<LiveEventEnvelope>> = None;

    loop {
        tokio::select! {
            maybe_message = socket.recv() => {
                let Some(Ok(message)) = maybe_message else {
                    break;
                };
                match message {
                    Message::Text(text) => {
                        let Ok(client_message) = serde_json::from_str::<LiveWsClientMessage>(&text) else {
                            let _ = send_ws_error(&mut socket, "Malformed live websocket message").await;
                            continue;
                        };
                        match client_message {
                            LiveWsClientMessage::Subscribe { last_seq } => {
                                match subscribe_live_socket(&state, match_id, last_seq).await {
                                    Ok((initial_events, next_receiver)) => {
                                        receiver = Some(next_receiver);
                                        subscribed = true;
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
                                        let _ = send_ws_error(&mut socket, &err.to_string()).await;
                                    }
                                }
                            }
                            LiveWsClientMessage::SubmitMove { intent_id, move_uci } => {
                                let intent_id = intent_id.unwrap_or_else(Uuid::new_v4);
                                match submit_human_move(state.clone(), match_id, intent_id, move_uci).await {
                                    Ok(ack) => {
                                        if send_ws_json(
                                            &mut socket,
                                            &LiveWsServerMessage::IntentAck { match_id, intent_id, ack }
                                        ).await.is_err() {
                                            return;
                                        }
                                    }
                                    Err(err) => {
                                        let _ = send_ws_error(&mut socket, &err.to_string()).await;
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
) -> Result<(Vec<LiveEventEnvelope>, tokio::sync::broadcast::Receiver<LiveEventEnvelope>), ApiError> {
    state.live_matches.bootstrap_from_db(&state.db, match_id).await?;
    let (snapshot, receiver) = state
        .live_matches
        .subscribe(match_id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("live state for match {match_id} not found")))?;
    let initial_events = initial_stream_events(state, match_id, last_seq, snapshot).await;
    Ok((initial_events, receiver))
}

async fn send_live_event(socket: &mut WebSocket, event: &LiveEventEnvelope) -> Result<(), axum::Error> {
    socket.send(Message::Text(serde_json::to_string(event).expect("event should serialize").into())).await
}

async fn send_ws_json(socket: &mut WebSocket, message: &LiveWsServerMessage) -> Result<(), axum::Error> {
    socket.send(Message::Text(serde_json::to_string(message).expect("message should serialize").into())).await
}

async fn send_ws_error(socket: &mut WebSocket, error: &str) -> Result<(), axum::Error> {
    send_ws_json(socket, &LiveWsServerMessage::Error { error: error.to_string() }).await
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
    use arena_core::{LiveResult, LiveRuntimeCheckpoint, LiveStatus, LiveTermination, ProtocolLiveSide};
    use chrono::Utc;
    use sqlx::sqlite::SqlitePoolOptions;

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
        upsert_live_runtime_checkpoint(&state.db, &second).await.unwrap();
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
        state.live_matches.bootstrap_from_db(&state.db, match_id).await.unwrap();

        let events =
            initial_stream_events(&state, match_id, Some(1), snapshot_from_checkpoint(&second)).await;
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], LiveEventEnvelope::MoveCommitted(_)));
    }

    #[tokio::test]
    async fn initial_stream_events_falls_back_to_snapshot_after_gap() {
        let state = setup_state().await;
        let match_id = Uuid::new_v4();
        let latest = checkpoint(match_id, 200, &["e2e4"]);
        upsert_live_runtime_checkpoint(&state.db, &latest).await.unwrap();
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
        state.live_matches.bootstrap_from_db(&state.db, match_id).await.unwrap();

        let events =
            initial_stream_events(&state, match_id, Some(1), snapshot_from_checkpoint(&latest)).await;
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], LiveEventEnvelope::Snapshot(_)));
    }

    #[tokio::test]
    async fn initial_stream_events_bootstraps_with_full_history_for_new_watcher() {
        let state = setup_state().await;
        let match_id = Uuid::new_v4();
        let first = checkpoint(match_id, 1, &[]);
        let second = checkpoint(match_id, 2, &["e2e4"]);
        upsert_live_runtime_checkpoint(&state.db, &second).await.unwrap();
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
        state.live_matches.bootstrap_from_db(&state.db, match_id).await.unwrap();

        let events =
            initial_stream_events(&state, match_id, None, snapshot_from_checkpoint(&second)).await;
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], LiveEventEnvelope::Snapshot(_)));
        assert!(matches!(events[1], LiveEventEnvelope::MoveCommitted(_)));
    }
}
