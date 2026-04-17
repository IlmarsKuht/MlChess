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
pub(super) struct MatchesQuery {
    tournament_id: Option<Uuid>,
}

pub(super) async fn list_matches_handler(
    State(state): State<AppState>,
    Query(query): Query<MatchesQuery>,
) -> Result<Json<Vec<ApiMatchSeries>>, ApiError> {
    let versions = list_agent_versions(&state.db, None).await?;
    let version_name_by_id = version_name_by_id(&versions);
    let human_player = ensure_human_player(&state.db).await?;
    let tournament_status_by_id = list_tournaments(&state.db)
        .await?
        .into_iter()
        .map(|tournament| (tournament.id, tournament.status))
        .collect::<std::collections::HashMap<_, _>>();
    let games = list_games(&state.db, query.tournament_id, None).await?;
    let checkpoints = list_live_runtime_checkpoints(&state.db, None).await?;
    let game_id_by_match_id = games
        .into_iter()
        .map(|game| (game.match_id, game.id))
        .collect::<std::collections::HashMap<_, _>>();
    let checkpoint_by_match_id = checkpoints
        .into_iter()
        .map(|checkpoint| (checkpoint.match_id, checkpoint))
        .collect::<std::collections::HashMap<_, _>>();
    let mut matches: Vec<_> = list_match_series(&state.db, query.tournament_id)
        .await?
        .into_iter()
        .map(|series| {
            let interactive = series.white_version_id == human_player.id
                || series.black_version_id == human_player.id;
            let (status, watch_state, game_id) = resolve_match_lifecycle(
                &series,
                tournament_status_by_id
                    .get(&series.tournament_id)
                    .copied()
                    .unwrap_or(arena_core::TournamentStatus::Running),
                game_id_by_match_id.get(&series.id).copied(),
                checkpoint_by_match_id.get(&series.id),
            );
            api_match_series(
                &series,
                status,
                watch_state,
                game_id,
                &version_name_by_id,
                &human_player,
                interactive,
            )
        })
        .collect();
    matches.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(Json(matches))
}

pub(super) async fn get_live_match_handler(
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

pub(super) async fn get_live_metrics_handler(
    State(state): State<AppState>,
) -> Json<crate::state::LiveMetricsSnapshot> {
    Json(state.live_metrics.snapshot())
}
