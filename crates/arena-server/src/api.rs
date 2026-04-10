use std::convert::Infallible;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
};
use chrono::Utc;
use futures::stream::{self, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    ApiError,
    orchestration::{
        create_human_game, create_tournament_run, load_human_player_profile,
        resolve_preset_participants, submit_human_move,
    },
    presentation::{
        ApiGameRecord, ApiLeaderboardEntry, ApiLiveGameState, ApiMatchSeries, HumanPlayerProfile,
        ReplayPayload, api_game_record, api_leaderboard_entry, api_live_game_state,
        api_match_series, is_terminal_live_status, live_game_event, version_name_by_id,
    },
    registry::sync_setup_registry_if_changed,
    state::AppState,
    storage::{
        ensure_agent_version_exists, ensure_human_player, ensure_pool_exists, get_agent,
        get_agent_version, get_event_preset, get_game, get_opening_suite, get_pool,
        get_tournament, list_agent_versions, list_agents, list_event_presets, list_games,
        list_match_series, list_opening_suites, list_pools, list_tournaments,
        load_aggregate_leaderboard, load_pool_leaderboard, load_rating_history,
        update_match_series_status, update_tournament_status,
    },
};
use arena_core::{GameResult, GameTermination, LiveGameFrame, LiveGameState, LiveSide, MatchStatus};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/agents", get(list_agents_handler))
        .route("/agents/{id}", get(get_agent_handler))
        .route("/agents/{id}/versions", get(list_agent_versions_handler))
        .route("/agent-versions/{id}", get(get_agent_version_handler))
        .route("/pools", get(list_pools_handler))
        .route("/pools/{id}", get(get_pool_handler))
        .route("/opening-suites", get(list_opening_suites_handler))
        .route("/opening-suites/{id}", get(get_opening_suite_handler))
        .route("/event-presets", get(list_event_presets_handler))
        .route("/event-presets/{id}", get(get_event_preset_handler))
        .route("/event-presets/{id}/start", post(start_event_preset_handler))
        .route("/duels", post(create_live_duel_handler))
        .route("/human-games", post(create_human_game_handler))
        .route("/human-games/{id}/move", post(submit_human_move_handler))
        .route("/human-player", get(get_human_player_handler))
        .route("/tournaments", get(list_tournaments_handler))
        .route("/tournaments/{id}", get(get_tournament_handler))
        .route("/tournaments/{id}/stop", post(stop_tournament_handler))
        .route("/matches", get(list_matches_handler))
        .route("/matches/{id}/live", get(get_live_match_handler))
        .route("/matches/{id}/live/stream", get(stream_live_match_handler))
        .route("/games", get(list_games_handler))
        .route("/games/{id}", get(get_game_handler))
        .route("/games/{id}/replay", get(get_game_replay_handler))
        .route("/games/{id}/logs", get(get_game_logs_handler))
        .route("/leaderboards", get(get_leaderboard_handler))
        .route("/ratings/history", get(get_rating_history_handler))
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}

#[derive(Debug, Deserialize)]
struct CreateLiveDuelRequest {
    name: String,
    pool_id: Uuid,
    white_version_id: Uuid,
    black_version_id: Uuid,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum HumanSideChoice {
    White,
    Black,
    Random,
}

#[derive(Debug, Deserialize)]
struct CreateHumanGameRequest {
    name: String,
    pool_id: Uuid,
    engine_version_id: Uuid,
    human_side: HumanSideChoice,
}

#[derive(Debug, Deserialize)]
struct SubmitHumanMoveRequest {
    uci: String,
}

#[derive(Debug, Deserialize)]
struct MatchesQuery {
    tournament_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct GamesQuery {
    tournament_id: Option<Uuid>,
    agent_version_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct LeaderboardQuery {
    pool_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct RatingHistoryQuery {
    pool_id: Option<Uuid>,
    agent_version_id: Option<Uuid>,
}

async fn sync_registry(state: &AppState) -> Result<(), ApiError> {
    sync_setup_registry_if_changed(&state.db, &state.setup_registry)
        .await
        .map_err(ApiError::Internal)
}

async fn list_agents_handler(State(state): State<AppState>) -> Result<Json<Vec<arena_core::Agent>>, ApiError> {
    sync_registry(&state).await?;
    Ok(Json(list_agents(&state.db).await?))
}

async fn get_agent_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<arena_core::Agent>, ApiError> {
    sync_registry(&state).await?;
    get_agent(&state.db, id).await.map(Json)
}

async fn list_agent_versions_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<Uuid>,
) -> Result<Json<Vec<arena_core::AgentVersion>>, ApiError> {
    sync_registry(&state).await?;
    Ok(Json(list_agent_versions(&state.db, Some(agent_id)).await?))
}

async fn get_agent_version_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<arena_core::AgentVersion>, ApiError> {
    sync_registry(&state).await?;
    get_agent_version(&state.db, id).await.map(Json)
}

async fn list_pools_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<arena_core::BenchmarkPool>>, ApiError> {
    sync_registry(&state).await?;
    Ok(Json(list_pools(&state.db).await?))
}

async fn get_pool_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<arena_core::BenchmarkPool>, ApiError> {
    sync_registry(&state).await?;
    get_pool(&state.db, id).await.map(Json)
}

async fn list_opening_suites_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<arena_core::OpeningSuite>>, ApiError> {
    sync_registry(&state).await?;
    Ok(Json(list_opening_suites(&state.db).await?))
}

async fn get_opening_suite_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<arena_core::OpeningSuite>, ApiError> {
    sync_registry(&state).await?;
    get_opening_suite(&state.db, id).await.map(Json)
}

async fn list_event_presets_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<arena_core::EventPreset>>, ApiError> {
    sync_registry(&state).await?;
    Ok(Json(list_event_presets(&state.db).await?))
}

async fn get_event_preset_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<arena_core::EventPreset>, ApiError> {
    sync_registry(&state).await?;
    get_event_preset(&state.db, id).await.map(Json)
}

async fn list_tournaments_handler(
    State(state): State<AppState>,
) -> Result<Json<Vec<arena_core::Tournament>>, ApiError> {
    Ok(Json(list_tournaments(&state.db).await?))
}

async fn get_tournament_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<arena_core::Tournament>, ApiError> {
    get_tournament(&state.db, id).await.map(Json)
}

async fn start_event_preset_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    sync_registry(&state).await?;
    let preset = get_event_preset(&state.db, id).await?;
    if !preset.active {
        return Err(ApiError::Conflict("event preset is inactive".to_string()));
    }

    let participant_version_ids = resolve_preset_participants(&state.db, &preset).await?;
    if participant_version_ids.len() < 2 {
        return Err(ApiError::BadRequest(
            "this event preset currently resolves to fewer than two engines".to_string(),
        ));
    }

    let tournament = create_tournament_run(
        &state.db,
        preset.name.clone(),
        preset.kind,
        preset.pool_id,
        participant_version_ids,
        preset.worker_count,
        preset.games_per_pairing,
    )
    .await?;

    let started = state.coordinator.start(state.clone(), tournament.id).await?;
    if !started {
        return Err(ApiError::Conflict("tournament is already running".to_string()));
    }
    Ok(Json(json!({
        "started": true,
        "tournament_id": tournament.id,
        "event_preset_id": preset.id
    })))
}

async fn create_live_duel_handler(
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

    let started = state.coordinator.start(state.clone(), tournament.id).await?;
    if !started {
        return Err(ApiError::Conflict("tournament is already running".to_string()));
    }

    Ok(Json(json!({
        "started": true,
        "tournament_id": tournament.id,
    })))
}

async fn create_human_game_handler(
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

async fn submit_human_move_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<SubmitHumanMoveRequest>,
) -> Result<Json<Value>, ApiError> {
    submit_human_move(state, id, payload.uci).await?;
    Ok(Json(json!({ "accepted": true, "match_id": id })))
}

async fn get_human_player_handler(
    State(state): State<AppState>,
) -> Result<Json<HumanPlayerProfile>, ApiError> {
    Ok(Json(load_human_player_profile(&state.db).await?))
}

async fn stop_tournament_handler(
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

async fn list_matches_handler(
    State(state): State<AppState>,
    Query(query): Query<MatchesQuery>,
) -> Result<Json<Vec<ApiMatchSeries>>, ApiError> {
    reconcile_running_matches(&state, query.tournament_id).await?;
    let versions = list_agent_versions(&state.db, None).await?;
    let version_name_by_id = version_name_by_id(&versions);
    let human_player = ensure_human_player(&state.db).await?;
    let mut matches: Vec<_> = list_match_series(&state.db, query.tournament_id)
        .await?
        .into_iter()
        .map(|series| api_match_series(&series, &version_name_by_id, &human_player, false))
        .collect();
    let human_matches = state.human_games.list().await;
    matches.extend(human_matches.into_iter().map(|session| {
        api_match_series(
            &session.match_series,
            &version_name_by_id,
            &session.human_player,
            true,
        )
    }));
    matches.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(Json(matches))
}

async fn get_live_match_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApiLiveGameState>, ApiError> {
    reconcile_single_running_match(&state, id).await?;
    let live_state = state
        .live_games
        .get(id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("live state for match {id} not found")))?;
    let versions = list_agent_versions(&state.db, None).await?;
    let version_name_by_id = version_name_by_id(&versions);
    let human_player = ensure_human_player(&state.db).await?;
    let interactive = state.human_games.get(id).await.is_some();
    Ok(Json(api_live_game_state(
        &live_state,
        &version_name_by_id,
        &human_player,
        interactive,
    )))
}

async fn stream_live_match_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    reconcile_single_running_match(&state, id).await?;
    let (initial_state, receiver) = state
        .live_games
        .subscribe(id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("live state for match {id} not found")))?;

    let versions = list_agent_versions(&state.db, None).await?;
    let version_name_by_id = version_name_by_id(&versions);
    let human_player = ensure_human_player(&state.db).await?;
    let interactive = state.human_games.get(id).await.is_some();
    let initial_terminal = is_terminal_live_status(initial_state.status);
    let initial_payload =
        api_live_game_state(&initial_state, &version_name_by_id, &human_player, interactive);
    let initial_stream =
        stream::once(async move { Ok::<Event, Infallible>(live_game_event(&initial_payload)) });
    let updates = stream::unfold(
        (
            receiver,
            initial_terminal,
            version_name_by_id,
            human_player,
            interactive,
        ),
        |(mut receiver, finished, version_name_by_id, human_player, interactive)| async move {
            if finished {
                return None;
            }

            loop {
                match receiver.recv().await {
                    Ok(state) => {
                        let terminal = is_terminal_live_status(state.status);
                        let payload = api_live_game_state(
                            &state,
                            &version_name_by_id,
                            &human_player,
                            interactive,
                        );
                        let event = live_game_event(&payload);
                        return Some((
                            Ok(event),
                            (
                                receiver,
                                terminal,
                                version_name_by_id,
                                human_player,
                                interactive,
                            ),
                        ));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                }
            }
        },
    );

    Ok(Sse::new(initial_stream.chain(updates)).keep_alive(KeepAlive::default()))
}

async fn reconcile_running_matches(
    state: &AppState,
    tournament_id: Option<Uuid>,
) -> Result<(), ApiError> {
    let matches = list_match_series(&state.db, tournament_id).await?;
    for series in matches {
        if series.status == MatchStatus::Running {
            reconcile_single_running_match(state, series.id).await?;
        }
    }
    Ok(())
}

async fn reconcile_single_running_match(state: &AppState, match_id: Uuid) -> Result<(), ApiError> {
    let Some(live_state) = state.live_games.get(match_id).await else {
        return Ok(());
    };

    let Some(next_state) = stale_timeout_live_state(&live_state) else {
        return Ok(());
    };

    state.live_games.upsert(next_state).await;
    update_match_series_status(&state.db, match_id, MatchStatus::Completed).await?;
    Ok(())
}

fn stale_timeout_live_state(live_state: &LiveGameState) -> Option<LiveGameState> {
    if live_state.status != MatchStatus::Running {
        return None;
    }

    let frame = live_state.live_frames.last()?;
    let elapsed_ms = (Utc::now() - frame.updated_at).num_milliseconds().max(0) as u64;
    let remaining_ms = match frame.side_to_move {
        LiveSide::White => frame.white_time_left_ms,
        LiveSide::Black => frame.black_time_left_ms,
    };
    let grace_ms = 3_000;
    if elapsed_ms <= remaining_ms.saturating_add(grace_ms) {
        return None;
    }

    let (result, white_time_left_ms, black_time_left_ms) = match frame.side_to_move {
        LiveSide::White => (GameResult::BlackWin, 0, frame.black_time_left_ms),
        LiveSide::Black => (GameResult::WhiteWin, frame.white_time_left_ms, 0),
    };
    let updated_at = Utc::now();

    Some(LiveGameState {
        match_id: live_state.match_id,
        tournament_id: live_state.tournament_id,
        pool_id: live_state.pool_id,
        variant: live_state.variant,
        white_version_id: live_state.white_version_id,
        black_version_id: live_state.black_version_id,
        start_fen: live_state.start_fen.clone(),
        current_fen: live_state.current_fen.clone(),
        moves_uci: live_state.moves_uci.clone(),
        white_time_left_ms,
        black_time_left_ms,
        status: MatchStatus::Completed,
        result: Some(result),
        termination: Some(GameTermination::Timeout),
        updated_at,
        live_frames: vec![LiveGameFrame {
            ply: live_state.moves_uci.len() as u32,
            fen: live_state.current_fen.clone(),
            move_uci: live_state.moves_uci.last().cloned(),
            white_time_left_ms,
            black_time_left_ms,
            updated_at,
            side_to_move: frame.side_to_move.clone(),
            status: MatchStatus::Completed,
            result: Some(result),
            termination: Some(GameTermination::Timeout),
        }],
    })
}

async fn list_games_handler(
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

async fn get_game_handler(
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

async fn get_game_replay_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ReplayPayload>, ApiError> {
    let game = get_game(&state.db, id).await?;
    Ok(Json(ReplayPayload {
        id: game.id,
        variant: game.variant,
        start_fen: game.start_fen,
        pgn: game.pgn,
        moves_uci: game.moves_uci,
        result: game.result,
        termination: game.termination,
    }))
}

async fn get_game_logs_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let game = get_game(&state.db, id).await?;
    Ok(Json(json!({ "id": game.id, "logs": game.logs })))
}

async fn get_leaderboard_handler(
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

async fn get_rating_history_handler(
    State(state): State<AppState>,
    Query(query): Query<RatingHistoryQuery>,
) -> Result<Json<Vec<arena_core::RatingSnapshot>>, ApiError> {
    Ok(Json(
        load_rating_history(&state.db, query.pool_id, query.agent_version_id).await?,
    ))
}
