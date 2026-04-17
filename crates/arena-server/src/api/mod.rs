// Legacy transition module.
// Do not add new behavior here.
// Move route families into focused api submodules instead.

#[cfg(test)]
use crate::live::stream_bootstrap::initial_stream_events;
use crate::{
    ApiError,
    registry::sync_setup_registry_if_changed,
    state::AppState,
};

mod agents;
mod debug;
mod event_presets;
mod games;
mod health;
mod human_games;
mod live_duel;
mod live_ws;
mod pools;
mod leaderboards;
mod matches;
mod routes;
mod tournaments;
pub(crate) use routes::router;

async fn sync_registry(state: &AppState) -> Result<(), ApiError> {
    sync_setup_registry_if_changed(&state.db, &state.setup_registry)
        .await
        .map_err(ApiError::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::init_db,
        live::{move_committed_from_checkpoint, snapshot_from_checkpoint},
        registry::{SetupRegistryCache, sync_setup_registry_if_changed},
        state::{HumanGameStore, TournamentCoordinator},
        storage::{insert_live_runtime_event, upsert_live_runtime_checkpoint},
    };
    use arena_core::{
        LiveEventEnvelope, LiveResult, LiveRuntimeCheckpoint, LiveStatus, LiveTermination,
        ProtocolLiveSide,
    };
    use axum::body::Body;
    use axum::http::StatusCode;
    use chrono::Utc;
    use serde_json::{Value, json};
    use sqlx::sqlite::SqlitePoolOptions;
    use tower::ServiceExt;
    use uuid::Uuid;

    async fn setup_state() -> AppState {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        init_db(&db).await.unwrap();
        let setup_registry = SetupRegistryCache::default();
        sync_setup_registry_if_changed(&db, &setup_registry)
            .await
            .unwrap();
        AppState {
            db,
            coordinator: TournamentCoordinator::default(),
            live_matches: crate::live::LiveMatchStore::default(),
            live_metrics: crate::state::LiveMetricsStore::default(),
            human_games: HumanGameStore::default(),
            debug_reports_dir: std::env::temp_dir()
                .join(format!("mlchess-debug-reports-{}", Uuid::new_v4())),
            frontend_dist: None,
            setup_registry,
        }
    }

    fn checkpoint(match_id: Uuid, seq: u64, moves: &[&str]) -> LiveRuntimeCheckpoint {
        LiveRuntimeCheckpoint {
            match_id,
            seq,
            status: LiveStatus::Running,
            result: LiveResult::None,
            termination: LiveTermination::None,
            start_fen: "4k3/8/8/8/8/8/8/4K3 w - - 0 1".to_string(),
            fen: if moves.is_empty() {
                "4k3/8/8/8/8/8/8/4K3 w - - 0 1".to_string()
            } else {
                "4k3/8/8/8/8/8/8/4K3 b - - 0 1".to_string()
            },
            moves: moves.iter().map(|value| (*value).to_string()).collect(),
            white_remaining_ms: 60_000,
            black_remaining_ms: 60_000,
            side_to_move: if moves.is_empty() {
                ProtocolLiveSide::White
            } else {
                ProtocolLiveSide::Black
            },
            turn_started_server_unix_ms: Utc::now().timestamp_millis(),
            updated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn initial_stream_events_prefers_replay_when_available() {
        let state = setup_state().await;
        let match_id = Uuid::new_v4();
        let first = checkpoint(match_id, 1, &[]);
        let second = checkpoint(match_id, 2, &["e2e4"]);
        upsert_live_runtime_checkpoint(&state.db, &second)
            .await
            .unwrap();
        insert_live_runtime_event(
            &state.db,
            &LiveEventEnvelope::Snapshot(snapshot_from_checkpoint(&first)),
        )
        .await
        .unwrap();
        insert_live_runtime_event(
            &state.db,
            &LiveEventEnvelope::MoveCommitted(move_committed_from_checkpoint(&second)),
        )
        .await
        .unwrap();
        state
            .live_matches
            .bootstrap_from_db(&state.db, match_id)
            .await
            .unwrap();

        let events =
            initial_stream_events(&state, match_id, Some(1), snapshot_from_checkpoint(&second))
                .await;
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], LiveEventEnvelope::MoveCommitted(_)));
    }

    #[tokio::test]
    async fn initial_stream_events_falls_back_to_snapshot_after_gap() {
        let state = setup_state().await;
        let match_id = Uuid::new_v4();
        let latest = checkpoint(match_id, 200, &["e2e4"]);
        upsert_live_runtime_checkpoint(&state.db, &latest)
            .await
            .unwrap();
        for seq in 73..=200 {
            let event_checkpoint = LiveRuntimeCheckpoint {
                seq,
                ..latest.clone()
            };
            let event = if seq == 73 {
                LiveEventEnvelope::Snapshot(snapshot_from_checkpoint(&event_checkpoint))
            } else {
                LiveEventEnvelope::MoveCommitted(move_committed_from_checkpoint(&event_checkpoint))
            };
            insert_live_runtime_event(&state.db, &event).await.unwrap();
        }
        state
            .live_matches
            .bootstrap_from_db(&state.db, match_id)
            .await
            .unwrap();

        let events =
            initial_stream_events(&state, match_id, Some(1), snapshot_from_checkpoint(&latest))
                .await;
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], LiveEventEnvelope::Snapshot(_)));
    }

    #[tokio::test]
    async fn initial_stream_events_bootstraps_with_full_history_for_new_watcher() {
        let state = setup_state().await;
        let match_id = Uuid::new_v4();
        let first = checkpoint(match_id, 1, &[]);
        let second = checkpoint(match_id, 2, &["e2e4"]);
        upsert_live_runtime_checkpoint(&state.db, &second)
            .await
            .unwrap();
        insert_live_runtime_event(
            &state.db,
            &LiveEventEnvelope::Snapshot(snapshot_from_checkpoint(&first)),
        )
        .await
        .unwrap();
        insert_live_runtime_event(
            &state.db,
            &LiveEventEnvelope::MoveCommitted(move_committed_from_checkpoint(&second)),
        )
        .await
        .unwrap();
        state
            .live_matches
            .bootstrap_from_db(&state.db, match_id)
            .await
            .unwrap();

        let events =
            initial_stream_events(&state, match_id, None, snapshot_from_checkpoint(&second)).await;
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], LiveEventEnvelope::Snapshot(_)));
        assert!(matches!(events[1], LiveEventEnvelope::MoveCommitted(_)));
    }

    #[tokio::test]
    async fn error_payload_echoes_request_id() {
        let state = setup_state().await;
        let app = crate::build_app(state);
        let request_id = Uuid::new_v4();
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/api/games/{}", Uuid::new_v4()))
                    .header("x-request-id", request_id.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            payload
                .get("request_id")
                .and_then(serde_json::Value::as_str),
            Some(request_id.to_string().as_str())
        );
    }

    #[tokio::test]
    async fn match_debug_bundle_includes_recent_requests() {
        let state = setup_state().await;
        let app = crate::build_app(state.clone());
        let pool_id = crate::storage::list_pools(&state.db)
            .await
            .unwrap()
            .remove(0)
            .id;
        let mut versions = crate::storage::list_agent_versions(&state.db, None)
            .await
            .unwrap();
        let white = versions.remove(0);
        let black = versions.remove(0);
        let tournament = arena_core::Tournament {
            id: Uuid::new_v4(),
            name: "debug bundle".to_string(),
            kind: arena_core::TournamentKind::RoundRobin,
            pool_id,
            participant_version_ids: vec![white.id, black.id],
            worker_count: 1,
            games_per_pairing: 1,
            status: arena_core::TournamentStatus::Running,
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            completed_at: None,
        };
        crate::storage::insert_tournament(&state.db, &tournament)
            .await
            .unwrap();
        let series = arena_core::MatchSeries {
            id: Uuid::new_v4(),
            tournament_id: tournament.id,
            pool_id,
            round_index: 0,
            white_version_id: white.id,
            black_version_id: black.id,
            opening_id: None,
            game_index: 0,
            status: arena_core::MatchStatus::Running,
            created_at: Utc::now(),
        };
        crate::storage::insert_match_series(&state.db, &series)
            .await
            .unwrap();
        let checkpoint = checkpoint(series.id, 1, &[]);
        crate::storage::upsert_live_runtime_checkpoint(&state.db, &checkpoint)
            .await
            .unwrap();
        state
            .live_matches
            .bootstrap_from_db(&state.db, series.id)
            .await
            .unwrap();

        let request_id = Uuid::new_v4();
        let _ = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/api/matches/{}/live", series.id))
                    .header("x-request-id", request_id.to_string())
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/api/debug/matches/{}/bundle", series.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let requests = payload
            .get("recent_requests")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(!requests.is_empty());
        assert!(requests.iter().any(|entry| {
            entry.get("request_id").and_then(serde_json::Value::as_str)
                == Some(&request_id.to_string())
        }));
        let first = requests.first().expect("expected recent request");
        assert!(first.get("duration_ms").is_some());
    }

    #[tokio::test]
    async fn request_journal_persists_client_route_context() {
        let state = setup_state().await;
        let app = crate::build_app(state.clone());
        let request_id = Uuid::new_v4();
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/leaderboards")
                    .header("x-request-id", request_id.to_string())
                    .header("x-client-route", "/#/watch/test-match")
                    .header("x-client-ts", "2026-04-12T17:44:23.450Z")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let entries = crate::storage::list_recent_request_errors(&state.db, 5)
            .await
            .unwrap();
        assert!(entries.is_empty());

        let leaderboards_request = crate::storage::get_request_journal_entry(&state.db, request_id)
            .await
            .unwrap()
            .expect("request should be stored");
        assert_eq!(
            leaderboards_request.client_route.as_deref(),
            Some("/#/watch/test-match")
        );
        assert_eq!(
            leaderboards_request.client_ts.as_deref(),
            Some("2026-04-12T17:44:23.450Z")
        );
        assert!(leaderboards_request.duration_ms >= 0);
    }

    #[tokio::test]
    async fn list_matches_resolves_finished_game_to_replay() {
        let state = setup_state().await;
        let app = crate::build_app(state.clone());
        let pool = crate::storage::list_pools(&state.db)
            .await
            .unwrap()
            .remove(0);
        let mut versions = crate::storage::list_agent_versions(&state.db, None)
            .await
            .unwrap();
        let white = versions.remove(0);
        let black = versions.remove(0);
        let tournament = arena_core::Tournament {
            id: Uuid::new_v4(),
            name: "resolved".to_string(),
            kind: arena_core::TournamentKind::RoundRobin,
            pool_id: pool.id,
            participant_version_ids: vec![white.id, black.id],
            worker_count: 1,
            games_per_pairing: 1,
            status: arena_core::TournamentStatus::Completed,
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            completed_at: Some(Utc::now()),
        };
        crate::storage::insert_tournament(&state.db, &tournament)
            .await
            .unwrap();
        let series = arena_core::MatchSeries {
            id: Uuid::new_v4(),
            tournament_id: tournament.id,
            pool_id: pool.id,
            round_index: 0,
            white_version_id: white.id,
            black_version_id: black.id,
            opening_id: None,
            game_index: 0,
            status: arena_core::MatchStatus::Running,
            created_at: Utc::now(),
        };
        crate::storage::insert_match_series(&state.db, &series)
            .await
            .unwrap();
        let game = arena_core::GameRecord {
            id: Uuid::new_v4(),
            tournament_id: tournament.id,
            match_id: series.id,
            pool_id: pool.id,
            variant: pool.variant,
            opening_id: None,
            white_version_id: white.id,
            black_version_id: black.id,
            result: arena_core::GameResult::WhiteWin,
            termination: arena_core::GameTermination::EngineFailure,
            start_fen: "startpos".to_string(),
            pgn: String::new(),
            moves_uci: vec!["e2e4".to_string()],
            white_time_left_ms: pool.time_control.initial_ms,
            black_time_left_ms: pool.time_control.initial_ms,
            logs: Vec::new(),
            started_at: Utc::now(),
            completed_at: Utc::now(),
        };
        crate::storage::insert_game(&state.db, &game).await.unwrap();

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/matches")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let first = payload.as_array().and_then(|items| items.first()).unwrap();
        assert_eq!(
            first.get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(
            first.get("watch_state").and_then(Value::as_str),
            Some("replay")
        );
        assert_eq!(
            first.get("game_id").and_then(Value::as_str),
            Some(game.id.to_string().as_str())
        );
    }

    #[tokio::test]
    async fn list_matches_reports_unavailable_when_running_row_has_no_live_state() {
        let state = setup_state().await;
        let app = crate::build_app(state.clone());
        let pool = crate::storage::list_pools(&state.db)
            .await
            .unwrap()
            .remove(0);
        let mut versions = crate::storage::list_agent_versions(&state.db, None)
            .await
            .unwrap();
        let white = versions.remove(0);
        let black = versions.remove(0);
        let tournament = arena_core::Tournament {
            id: Uuid::new_v4(),
            name: "unavailable".to_string(),
            kind: arena_core::TournamentKind::RoundRobin,
            pool_id: pool.id,
            participant_version_ids: vec![white.id, black.id],
            worker_count: 1,
            games_per_pairing: 1,
            status: arena_core::TournamentStatus::Running,
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            completed_at: None,
        };
        crate::storage::insert_tournament(&state.db, &tournament)
            .await
            .unwrap();
        crate::storage::insert_match_series(
            &state.db,
            &arena_core::MatchSeries {
                id: Uuid::new_v4(),
                tournament_id: tournament.id,
                pool_id: pool.id,
                round_index: 0,
                white_version_id: white.id,
                black_version_id: black.id,
                opening_id: None,
                game_index: 0,
                status: arena_core::MatchStatus::Running,
                created_at: Utc::now(),
            },
        )
        .await
        .unwrap();

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/matches")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let first = payload.as_array().and_then(|items| items.first()).unwrap();
        assert_eq!(first.get("status").and_then(Value::as_str), Some("running"));
        assert_eq!(
            first.get("watch_state").and_then(Value::as_str),
            Some("unavailable")
        );
    }

    #[tokio::test]
    async fn list_matches_marks_pending_rows_as_skipped_for_completed_tournaments() {
        let state = setup_state().await;
        let app = crate::build_app(state.clone());
        let pool = crate::storage::list_pools(&state.db)
            .await
            .unwrap()
            .remove(0);
        let mut versions = crate::storage::list_agent_versions(&state.db, None)
            .await
            .unwrap();
        let white = versions.remove(0);
        let black = versions.remove(0);
        let tournament = arena_core::Tournament {
            id: Uuid::new_v4(),
            name: "completed".to_string(),
            kind: arena_core::TournamentKind::RoundRobin,
            pool_id: pool.id,
            participant_version_ids: vec![white.id, black.id],
            worker_count: 1,
            games_per_pairing: 1,
            status: arena_core::TournamentStatus::Completed,
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            completed_at: Some(Utc::now()),
        };
        crate::storage::insert_tournament(&state.db, &tournament)
            .await
            .unwrap();
        crate::storage::insert_match_series(
            &state.db,
            &arena_core::MatchSeries {
                id: Uuid::new_v4(),
                tournament_id: tournament.id,
                pool_id: pool.id,
                round_index: 0,
                white_version_id: white.id,
                black_version_id: black.id,
                opening_id: None,
                game_index: 1,
                status: arena_core::MatchStatus::Pending,
                created_at: Utc::now(),
            },
        )
        .await
        .unwrap();

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/matches")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let first = payload.as_array().and_then(|items| items.first()).unwrap();
        assert_eq!(first.get("status").and_then(Value::as_str), Some("skipped"));
        assert_eq!(
            first.get("watch_state").and_then(Value::as_str),
            Some("unavailable")
        );
    }

    #[tokio::test]
    async fn list_tournaments_resolves_running_rows_with_completed_matches() {
        let state = setup_state().await;
        let app = crate::build_app(state.clone());
        let pool = crate::storage::list_pools(&state.db)
            .await
            .unwrap()
            .remove(0);
        let mut versions = crate::storage::list_agent_versions(&state.db, None)
            .await
            .unwrap();
        let white = versions.remove(0);
        let black = versions.remove(0);
        let tournament = arena_core::Tournament {
            id: Uuid::new_v4(),
            name: "stale running".to_string(),
            kind: arena_core::TournamentKind::RoundRobin,
            pool_id: pool.id,
            participant_version_ids: vec![white.id, black.id],
            worker_count: 1,
            games_per_pairing: 1,
            status: arena_core::TournamentStatus::Running,
            created_at: Utc::now(),
            started_at: Some(Utc::now()),
            completed_at: None,
        };
        crate::storage::insert_tournament(&state.db, &tournament)
            .await
            .unwrap();
        let series = arena_core::MatchSeries {
            id: Uuid::new_v4(),
            tournament_id: tournament.id,
            pool_id: pool.id,
            round_index: 0,
            white_version_id: white.id,
            black_version_id: black.id,
            opening_id: None,
            game_index: 0,
            status: arena_core::MatchStatus::Completed,
            created_at: Utc::now(),
        };
        crate::storage::insert_match_series(&state.db, &series)
            .await
            .unwrap();
        crate::storage::insert_game(
            &state.db,
            &arena_core::GameRecord {
                id: Uuid::new_v4(),
                tournament_id: tournament.id,
                match_id: series.id,
                pool_id: pool.id,
                variant: pool.variant,
                opening_id: None,
                white_version_id: white.id,
                black_version_id: black.id,
                result: arena_core::GameResult::WhiteWin,
                termination: arena_core::GameTermination::Checkmate,
                start_fen: "startpos".to_string(),
                pgn: String::new(),
                moves_uci: vec!["e2e4".to_string()],
                white_time_left_ms: pool.time_control.initial_ms,
                black_time_left_ms: pool.time_control.initial_ms,
                logs: Vec::new(),
                started_at: Utc::now(),
                completed_at: Utc::now(),
            },
        )
        .await
        .unwrap();

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/tournaments")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let first = payload.as_array().and_then(|items| items.first()).unwrap();
        assert_eq!(
            first.get("status").and_then(Value::as_str),
            Some("completed")
        );
    }

    #[tokio::test]
    async fn list_tournaments_marks_stale_zero_match_running_tournament_failed() {
        let state = setup_state().await;
        let app = crate::build_app(state.clone());
        let pool = crate::storage::list_pools(&state.db)
            .await
            .unwrap()
            .remove(0);
        let mut versions = crate::storage::list_agent_versions(&state.db, None)
            .await
            .unwrap();
        let white = versions.remove(0);
        let black = versions.remove(0);
        crate::storage::insert_tournament(
            &state.db,
            &arena_core::Tournament {
                id: Uuid::new_v4(),
                name: "ghost".to_string(),
                kind: arena_core::TournamentKind::RoundRobin,
                pool_id: pool.id,
                participant_version_ids: vec![white.id, black.id],
                worker_count: 1,
                games_per_pairing: 1,
                status: arena_core::TournamentStatus::Running,
                created_at: Utc::now() - chrono::Duration::seconds(31),
                started_at: Some(Utc::now() - chrono::Duration::seconds(31)),
                completed_at: None,
            },
        )
        .await
        .unwrap();

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/tournaments")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let first = payload.as_array().and_then(|items| items.first()).unwrap();
        assert_eq!(first.get("status").and_then(Value::as_str), Some("failed"));
    }

    #[tokio::test]
    async fn save_debug_report_writes_repo_local_file() {
        let state = setup_state().await;
        let reports_dir = state.debug_reports_dir.clone();
        let app = crate::build_app(state);
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/debug/reports")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "preferred_filename": "custom report",
                            "report": {
                                "correlation": { "match_id": Uuid::new_v4() },
                                "report_summary": { "headline": "Test report" }
                            }
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let path = payload
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap();
        assert!(path.contains("custom-report.json"));
        assert!(std::path::Path::new(path).exists());
        assert!(path.starts_with(reports_dir.to_string_lossy().as_ref()));
    }
}
