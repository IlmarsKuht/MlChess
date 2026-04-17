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
pub(super) struct CreateLiveDuelRequest {
    name: String,
    pool_id: Uuid,
    white_version_id: Uuid,
    black_version_id: Uuid,
}

pub(super) async fn create_live_duel_handler(
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
