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

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(super::health::health))
        .route("/agents", get(super::agents::list_agents_handler))
        .route("/agents/{id}", get(super::agents::get_agent_handler))
        .route(
            "/agents/{id}/versions",
            get(super::agents::list_agent_versions_handler),
        )
        .route(
            "/agent-versions/{id}",
            get(super::agents::get_agent_version_handler),
        )
        .route("/pools", get(super::pools::list_pools_handler))
        .route("/pools/{id}", get(super::pools::get_pool_handler))
        .route(
            "/opening-suites",
            get(super::pools::list_opening_suites_handler),
        )
        .route(
            "/opening-suites/{id}",
            get(super::pools::get_opening_suite_handler),
        )
        .route(
            "/event-presets",
            get(super::event_presets::list_event_presets_handler),
        )
        .route(
            "/event-presets/{id}",
            get(super::event_presets::get_event_preset_handler),
        )
        .route(
            "/event-presets/{id}/start",
            post(super::event_presets::start_event_preset_handler),
        )
        .route("/duels", post(super::live_duel::create_live_duel_handler))
        .route(
            "/human-games",
            post(super::human_games::create_human_game_handler),
        )
        .route(
            "/human-player",
            get(super::human_games::get_human_player_handler),
        )
        .route("/tournaments", get(super::tournaments::list_tournaments_handler))
        .route(
            "/tournaments/{id}",
            get(super::tournaments::get_tournament_handler),
        )
        .route(
            "/tournaments/{id}/stop",
            post(super::tournaments::stop_tournament_handler),
        )
        .route("/matches", get(super::matches::list_matches_handler))
        .route("/matches/{id}/live", get(super::matches::get_live_match_handler))
        .route("/live/metrics", get(super::matches::get_live_metrics_handler))
        .route(
            "/matches/{id}/live/ws",
            get(super::live_ws::websocket_live_match_handler),
        )
        .route(
            "/debug/matches/{id}/bundle",
            get(super::debug::get_match_debug_bundle_handler),
        )
        .route(
            "/debug/matches/{id}/events",
            get(super::debug::get_match_debug_events_handler),
        )
        .route(
            "/debug/games/{id}/bundle",
            get(super::debug::get_game_debug_bundle_handler),
        )
        .route(
            "/debug/tournaments/{id}/bundle",
            get(super::debug::get_tournament_debug_bundle_handler),
        )
        .route(
            "/debug/request/{request_id}",
            get(super::debug::get_request_debug_handler),
        )
        .route(
            "/debug/recent-errors",
            get(super::debug::get_recent_errors_handler),
        )
        .route(
            "/debug/reports",
            post(super::debug::save_debug_report_handler),
        )
        .route("/games", get(super::games::list_games_handler))
        .route("/games/{id}", get(super::games::get_game_handler))
        .route("/games/{id}/replay", get(super::games::get_game_replay_handler))
        .route("/games/{id}/logs", get(super::games::get_game_logs_handler))
        .route("/leaderboards", get(super::leaderboards::get_leaderboard_handler))
        .route(
            "/ratings/history",
            get(super::leaderboards::get_rating_history_handler),
        )
}
