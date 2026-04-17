#![allow(unused_imports)]

use axum::{
    Json, Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::{Extension, Path, Query, State},
    routing::{get, post},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::info;
use uuid::Uuid;

use arena_core::{LiveEventEnvelope, LiveMatchSnapshot};
use crate::{
    ApiError,
    debug::query_service::build_debug_bundle,
    gameplay::build_replay_frames,
    human_games::service::{create_human_game, load_human_player_profile, submit_human_move},
    live::stream_bootstrap::subscribe_live_socket,
    presentation::{
        ApiGameRecord, ApiLeaderboardEntry, ApiMatchSeries, HumanPlayerProfile, ReplayPayload,
        api_game_record, api_leaderboard_entry, api_match_series, resolve_match_lifecycle,
        resolve_tournament_status, version_name_by_id,
    },
    state::{AppState, MoveDebugContext, RequestContext},
    storage::{
        ensure_agent_version_exists, ensure_human_player, ensure_pool_exists, get_agent,
        get_agent_version, get_event_preset, get_game, get_match_series, get_opening_suite,
        get_pool, get_request_journal_entry, get_tournament, list_agent_versions, list_agents,
        list_event_presets, list_games, list_live_runtime_checkpoints, list_match_series,
        list_opening_suites, list_pools, list_recent_request_errors, list_tournaments,
        load_aggregate_leaderboard, load_live_runtime_events_since, load_pool_leaderboard,
        load_rating_history, update_tournament_status,
    },
    tournaments::service::{create_tournament_run, resolve_preset_participants},
};
use super::sync_registry;

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


pub(super) async fn websocket_live_match_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Extension(request_context): Extension<RequestContext>,
    Path(id): Path<Uuid>,
) -> Result<impl axum::response::IntoResponse, ApiError> {
    state.live_matches.bootstrap_from_db(&state.db, id).await?;
    Ok(ws.on_upgrade(move |socket| handle_live_socket(state, id, socket, request_context)))
}

pub(super) async fn handle_live_socket(
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
