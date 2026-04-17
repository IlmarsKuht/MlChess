use std::path::PathBuf;

use arena_core::LiveEventEnvelope;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    ApiError,
    presentation::{participant_for_id, version_name_by_id},
    state::AppState,
    storage::{
        ensure_human_player, list_agent_versions, list_recent_request_errors,
        list_request_journal_for_entities, load_live_runtime_checkpoint,
        load_live_runtime_events_since,
    },
};
pub(crate) async fn build_debug_bundle(
    state: &AppState,
    match_series: Option<arena_core::MatchSeries>,
    tournament: Option<arena_core::Tournament>,
    game: Option<arena_core::GameRecord>,
    match_id: Option<Uuid>,
    tournament_id: Option<Uuid>,
) -> Result<Value, ApiError> {
    let resolved_match_id = match_id
        .or(match_series.as_ref().map(|value| value.id))
        .or(game.as_ref().map(|value| value.match_id));
    let resolved_tournament_id = tournament_id
        .or(tournament.as_ref().map(|value| value.id))
        .or(match_series.as_ref().map(|value| value.tournament_id))
        .or(game.as_ref().map(|value| value.tournament_id));
    let resolved_game_id = game.as_ref().map(|value| value.id);

    let checkpoint = match resolved_match_id {
        Some(value) => load_live_runtime_checkpoint(&state.db, value).await?,
        None => None,
    };
    let recent_live_events = match resolved_match_id {
        Some(value) => {
            let events = load_live_runtime_events_since(&state.db, value, 0).await?;
            let start = events.len().saturating_sub(20);
            events.into_iter().skip(start).collect::<Vec<_>>()
        }
        None => Vec::new(),
    };
    let recent_requests = list_request_journal_for_entities(
        &state.db,
        resolved_match_id,
        resolved_tournament_id,
        resolved_game_id,
        20,
    )
    .await?;
    let recent_errors = list_recent_request_errors(&state.db, 20)
        .await?
        .into_iter()
        .filter(|entry| {
            resolved_match_id
                .map(|value| entry.match_id == Some(value))
                .unwrap_or(false)
                || resolved_tournament_id
                    .map(|value| entry.tournament_id == Some(value))
                    .unwrap_or(false)
                || resolved_game_id
                    .map(|value| entry.game_id == Some(value))
                    .unwrap_or(false)
        })
        .take(10)
        .collect::<Vec<_>>();

    let recent_persisted_logs = game
        .as_ref()
        .map(|value| {
            let start = value.logs.len().saturating_sub(30);
            value.logs.iter().skip(start).cloned().collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let versions = list_agent_versions(&state.db, None).await?;
    let version_names = version_name_by_id(&versions);
    let human_player = ensure_human_player(&state.db).await?;
    let participants = json!({
        "white": match_series.as_ref().map(|series| participant_for_id(series.white_version_id, &version_names, &human_player)),
        "black": match_series.as_ref().map(|series| participant_for_id(series.black_version_id, &version_names, &human_player)),
    });
    let summary = summarize_bundle(
        match_series.as_ref(),
        tournament.as_ref(),
        game.as_ref(),
        checkpoint.as_ref(),
        &recent_requests,
        &recent_live_events,
    );
    let failure_analysis = summarize_failures(
        checkpoint.as_ref(),
        &recent_requests,
        &recent_errors,
        &recent_live_events,
    );

    Ok(json!({
        "summary": summary,
        "failure_analysis": failure_analysis,
        "entity": {
            "match": match_series,
            "tournament": tournament,
            "game": game,
        },
        "related": {
            "match_id": resolved_match_id,
            "tournament_id": resolved_tournament_id,
            "game_id": resolved_game_id,
        },
        "checkpoint": checkpoint,
        "recent_live_events": recent_live_events,
        "recent_persisted_logs": recent_persisted_logs,
        "recent_requests": recent_requests,
        "recent_errors": recent_errors,
        "live_metrics": state.live_metrics.snapshot(),
        "participants": participants,
        "correlation": {
            "request_ids": recent_requests.iter().map(|entry| entry.request_id).collect::<Vec<_>>(),
            "client_action_ids": recent_requests.iter().filter_map(|entry| entry.client_action_id).collect::<Vec<_>>(),
        }
    }))
}

fn summarize_bundle(
    match_series: Option<&arena_core::MatchSeries>,
    tournament: Option<&arena_core::Tournament>,
    game: Option<&arena_core::GameRecord>,
    checkpoint: Option<&arena_core::LiveRuntimeCheckpoint>,
    requests: &[crate::state::RequestJournalEntry],
    events: &[LiveEventEnvelope],
) -> String {
    let mut parts = Vec::new();
    if let Some(series) = match_series {
        parts.push(format!("match {} is {:?}", series.id, series.status));
    }
    if let Some(tournament) = tournament {
        parts.push(
            format!("tournament {:?} ", tournament.status)
                .trim()
                .to_string(),
        );
    }
    if let Some(checkpoint) = checkpoint {
        parts.push(format!(
            "live seq {} {:?}",
            checkpoint.seq, checkpoint.status
        ));
    }
    if let Some(game) = game {
        parts.push(format!(
            "game ended {:?} via {:?}",
            game.result, game.termination
        ));
    }
    if let Some(request) = requests.first() {
        parts.push(format!(
            "latest request {} {}",
            request.method, request.status_code
        ));
    }
    if !events.is_empty() {
        parts.push(format!("{} recent live events", events.len()));
    }
    if parts.is_empty() {
        "debug bundle assembled".to_string()
    } else {
        parts.join(" | ")
    }
}

fn summarize_failures(
    checkpoint: Option<&arena_core::LiveRuntimeCheckpoint>,
    requests: &[crate::state::RequestJournalEntry],
    recent_errors: &[crate::state::RequestJournalEntry],
    events: &[LiveEventEnvelope],
) -> Value {
    let latest_error = recent_errors.first();
    let failure_class = if latest_error
        .map(|entry| entry.route.contains("/live"))
        .unwrap_or(false)
    {
        "live_api"
    } else if latest_error
        .map(|entry| entry.route.contains("/debug"))
        .unwrap_or(false)
    {
        "debug_endpoint"
    } else if checkpoint.is_some() && events.is_empty() && !requests.is_empty() {
        "live_runtime_visibility"
    } else {
        "unknown"
    };
    let suspected_files: Vec<PathBuf> = match failure_class {
        "live_api" | "live_runtime_visibility" => vec![
            PathBuf::from("crates/arena-server/src/api.rs"),
            PathBuf::from("crates/arena-server/src/live.rs"),
            PathBuf::from("frontend/src/app/live.ts"),
        ],
        "debug_endpoint" => vec![
            PathBuf::from("crates/arena-server/src/api.rs"),
            PathBuf::from("frontend/src/App.tsx"),
        ],
        _ => vec![PathBuf::from("crates/arena-server/src/api.rs")],
    };
    json!({
        "failure_class": failure_class,
        "confidence": if latest_error.is_some() { "medium" } else { "low" },
        "signals": {
            "request_count": requests.len(),
            "error_count": recent_errors.len(),
            "live_event_count": events.len(),
            "has_checkpoint": checkpoint.is_some(),
        },
        "latest_error": latest_error,
        "next_debug_targets": suspected_files,
    })
}
