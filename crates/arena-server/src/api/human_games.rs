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
#[serde(rename_all = "snake_case")]
enum HumanSideChoice {
    White,
    Black,
    Random,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateHumanGameRequest {
    name: String,
    pool_id: Uuid,
    engine_version_id: Uuid,
    human_side: HumanSideChoice,
}

pub(super) async fn create_human_game_handler(
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

pub(super) async fn get_human_player_handler(
    State(state): State<AppState>,
) -> Result<Json<HumanPlayerProfile>, ApiError> {
    Ok(Json(load_human_player_profile(&state.db).await?))
}
