use axum::{
    body::{Body, to_bytes},
    extract::{MatchedPath, Request},
    http::{HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};
use serde_json::{Value, json};
use tracing::{info, warn};
use uuid::Uuid;

use crate::state::{AppState, RequestContext, RequestJournalEntry};

pub(crate) async fn request_context_middleware(
    axum::extract::State(state): axum::extract::State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let started_at = chrono::Utc::now();
    let method = request.method().as_str().to_string();
    let matched_path = request
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str)
        .unwrap_or_else(|| request.uri().path())
        .to_string();
    let request_id =
        request_header_uuid(request.headers(), "x-request-id").unwrap_or_else(Uuid::new_v4);
    let context = RequestContext {
        request_id,
        client_action_id: request_header_uuid(request.headers(), "x-client-action-id"),
        client_route: request
            .headers()
            .get("x-client-route")
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned),
        client_ts: request
            .headers()
            .get("x-client-ts")
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned),
        method: method.clone(),
        route: matched_path.clone(),
    };
    request.extensions_mut().insert(context.clone());
    let (match_id, tournament_id, game_id) = infer_entity_ids(request.uri().path());

    let mut response = next.run(request).await;
    response.headers_mut().insert(
        HeaderName::from_static("x-request-id"),
        HeaderValue::from_str(&request_id.to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("invalid-request-id")),
    );
    response = enrich_error_response(response, request_id).await;

    let completed_at = chrono::Utc::now();
    let status_code = response.status().as_u16();
    let error_text = response.extensions().get::<String>().cloned().or_else(|| {
        (status_code >= 400).then(|| format!("request failed with status {status_code}"))
    });
    let duration_ms = (completed_at - started_at).num_milliseconds();

    let journal = RequestJournalEntry {
        request_id,
        client_action_id: context.client_action_id,
        client_route: context.client_route.clone(),
        client_ts: context.client_ts.clone(),
        method,
        route: matched_path,
        status_code,
        match_id,
        tournament_id,
        game_id,
        started_at,
        completed_at,
        duration_ms,
        error_text,
    };
    if let Err(err) = crate::storage::insert_request_journal_entry(&state.db, &journal).await {
        warn!(request_id = %request_id, "failed to persist request journal entry: {err:#}");
    }
    info!(
        request_id = %request_id,
        client_action_id = ?context.client_action_id,
        client_route = ?context.client_route,
        status_code,
        route = %journal.route,
        match_id = ?journal.match_id,
        tournament_id = ?journal.tournament_id,
        game_id = ?journal.game_id,
        "handled api request"
    );
    response
}

async fn enrich_error_response(response: Response, request_id: Uuid) -> Response {
    if response.status().is_success() {
        return response;
    }
    let (parts, body) = response.into_parts();
    let bytes = match to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(_) => return Response::from_parts(parts, Body::empty()),
    };
    let mut payload = serde_json::from_slice::<Value>(&bytes)
        .unwrap_or_else(|_| json!({ "error": "request failed" }));
    if payload.get("request_id").is_none() {
        payload["request_id"] = json!(request_id);
    }
    let mut rebuilt = Response::from_parts(parts, Body::from(payload.to_string()));
    rebuilt.headers_mut().insert(
        HeaderName::from_static("content-type"),
        HeaderValue::from_static("application/json"),
    );
    if let Some(error_text) = payload.get("error").and_then(Value::as_str) {
        rebuilt.extensions_mut().insert(error_text.to_string());
    }
    rebuilt
}

fn request_header_uuid(headers: &axum::http::HeaderMap, name: &'static str) -> Option<Uuid> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
}

fn infer_entity_ids(path: &str) -> (Option<Uuid>, Option<Uuid>, Option<Uuid>) {
    let segments: Vec<_> = path.trim_matches('/').split('/').collect();
    let mut match_id = None;
    let mut tournament_id = None;
    let mut game_id = None;
    for window in segments.windows(2) {
        let Some(id) = Uuid::parse_str(window[1]).ok() else {
            continue;
        };
        match window[0] {
            "matches" => match_id = Some(id),
            "tournaments" => tournament_id = Some(id),
            "games" => game_id = Some(id),
            _ => {}
        }
    }
    (match_id, tournament_id, game_id)
}
