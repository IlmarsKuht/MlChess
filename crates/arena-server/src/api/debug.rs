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
pub(super) struct DebugEventsQuery {
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SaveDebugReportRequest {
    report: Value,
    preferred_filename: Option<String>,
}

pub(super) async fn get_match_debug_bundle_handler(
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

pub(super) async fn get_game_debug_bundle_handler(
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

pub(super) async fn get_tournament_debug_bundle_handler(
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

pub(super) async fn get_match_debug_events_handler(
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

pub(super) async fn get_request_debug_handler(
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

pub(super) async fn get_recent_errors_handler(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let errors = list_recent_request_errors(&state.db, 20).await?;
    Ok(Json(json!({ "errors": errors })))
}

pub(super) async fn save_debug_report_handler(
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
