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
pub(super) struct LeaderboardQuery {
    pool_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RatingHistoryQuery {
    pool_id: Option<Uuid>,
    agent_version_id: Option<Uuid>,
}

pub(super) async fn get_leaderboard_handler(
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

pub(super) async fn get_rating_history_handler(
    State(state): State<AppState>,
    Query(query): Query<RatingHistoryQuery>,
) -> Result<Json<Vec<arena_core::RatingSnapshot>>, ApiError> {
    Ok(Json(
        load_rating_history(&state.db, query.pool_id, query.agent_version_id).await?,
    ))
}
