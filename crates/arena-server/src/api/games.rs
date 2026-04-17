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
pub(super) struct GamesQuery {
    tournament_id: Option<Uuid>,
    agent_version_id: Option<Uuid>,
}


pub(super) async fn list_games_handler(
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

pub(super) async fn get_game_handler(
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

pub(super) async fn get_game_replay_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ReplayPayload>, ApiError> {
    let game = get_game(&state.db, id).await?;
    let frames = build_replay_frames(game.variant, &game.start_fen, &game.moves_uci)?;
    let start_fen = frames
        .first()
        .cloned()
        .unwrap_or_else(|| game.start_fen.clone());
    Ok(Json(ReplayPayload {
        id: game.id,
        variant: game.variant,
        frames,
        start_fen,
        pgn: game.pgn,
        moves_uci: game.moves_uci,
        result: game.result,
        termination: game.termination,
    }))
}

pub(super) async fn get_game_logs_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let game = get_game(&state.db, id).await?;
    Ok(Json(json!({ "id": game.id, "logs": game.logs })))
}
