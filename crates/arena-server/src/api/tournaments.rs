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

pub(super) async fn list_tournaments_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<arena_core::Tournament>>, ApiError> {
    let tournaments = list_tournaments(&state.db).await?;
    let matches = list_match_series(&state.db, None).await?;
    let games = list_games(&state.db, None, None).await?;
    let checkpoints = list_live_runtime_checkpoints(&state.db, None).await?;
    let game_id_by_match_id = games
        .into_iter()
        .map(|game| (game.match_id, game.id))
        .collect::<std::collections::HashMap<_, _>>();
    let checkpoint_by_match_id = checkpoints
        .into_iter()
        .map(|checkpoint| (checkpoint.match_id, checkpoint))
        .collect::<std::collections::HashMap<_, _>>();
    let mut match_statuses_by_tournament_id: std::collections::HashMap<
        Uuid,
        Vec<arena_core::MatchStatus>,
    > = std::collections::HashMap::new();
    let tournament_status_by_id = tournaments
        .iter()
        .map(|tournament| (tournament.id, tournament.status))
        .collect::<std::collections::HashMap<_, _>>();
    for series in &matches {
        let (status, _, _) = resolve_match_lifecycle(
            series,
            tournament_status_by_id
                .get(&series.tournament_id)
                .copied()
                .unwrap_or(arena_core::TournamentStatus::Running),
            game_id_by_match_id.get(&series.id).copied(),
            checkpoint_by_match_id.get(&series.id),
        );
        match_statuses_by_tournament_id
            .entry(series.tournament_id)
            .or_default()
            .push(status);
    }

    Ok(Json(
        tournaments
            .into_iter()
            .map(|mut tournament| {
                if let Some(match_statuses) = match_statuses_by_tournament_id.get(&tournament.id) {
                    tournament.status =
                        resolve_tournament_status(&tournament, match_statuses, Utc::now());
                } else {
                    tournament.status = resolve_tournament_status(&tournament, &[], Utc::now());
                }
                tournament
            })
            .collect(),
    ))
}

pub(super) async fn get_tournament_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<arena_core::Tournament>, ApiError> {
    get_tournament(&state.db, id).await.map(Json)
}

pub(super) async fn stop_tournament_handler(
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