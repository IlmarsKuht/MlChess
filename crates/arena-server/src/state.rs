use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use anyhow::Result;
use arena_core::{GameResult, MatchSeries, MatchStatus, TimeControl, TournamentStatus, Variant};
use arena_runner::AgentAdapter;
use chrono::{DateTime, Utc};
use cozy_chess::{Board, Color};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tracing::error;
use uuid::Uuid;

use crate::{
    ApiError, live::LiveMatchStore, orchestration::run_tournament, registry::SetupRegistryCache,
    storage::update_tournament_status,
};

#[derive(Clone)]
pub struct AppState {
    pub(crate) db: SqlitePool,
    pub(crate) coordinator: TournamentCoordinator,
    pub(crate) live_matches: LiveMatchStore,
    pub(crate) live_metrics: LiveMetricsStore,
    pub(crate) human_games: HumanGameStore,
    pub(crate) debug_reports_dir: PathBuf,
    pub(crate) frontend_dist: Option<PathBuf>,
    pub(crate) setup_registry: SetupRegistryCache,
}

#[derive(Clone, Default)]
pub(crate) struct TournamentCoordinator {
    running: Arc<tokio::sync::Mutex<HashMap<Uuid, Arc<AtomicBool>>>>,
}

#[derive(Clone, Default)]
pub(crate) struct HumanGameStore {
    sessions: Arc<tokio::sync::RwLock<HashMap<Uuid, HumanGameHandle>>>,
}

#[derive(Clone, Default)]
pub(crate) struct LiveMetricsStore {
    pub(crate) published_events: Arc<AtomicU64>,
    pub(crate) replay_requests: Arc<AtomicU64>,
    pub(crate) replay_events_served: Arc<AtomicU64>,
    pub(crate) snapshot_fallbacks: Arc<AtomicU64>,
    pub(crate) restored_matches: Arc<AtomicU64>,
    pub(crate) websocket_connections: Arc<AtomicU64>,
    pub(crate) move_intent_errors: Arc<AtomicU64>,
    pub(crate) timeout_fires: Arc<AtomicU64>,
}

#[derive(Clone)]
pub(crate) struct HumanGameHandle {
    pub(crate) command_tx: tokio::sync::mpsc::Sender<HumanGameCommand>,
}

pub(crate) enum HumanGameCommand {
    SubmitMove {
        intent_id: Uuid,
        client_action_id: Option<Uuid>,
        ws_connection_id: Option<Uuid>,
        move_uci: String,
        respond_to: tokio::sync::oneshot::Sender<HumanMoveAck>,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum HumanMoveAck {
    Accepted,
    RejectedIllegal,
    RejectedNotYourTurn,
    RejectedGameFinished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletedGameTable {
    Engine,
    Human,
}

#[derive(Clone)]
pub(crate) struct MatchSession {
    pub(crate) name: String,
    pub(crate) match_series: MatchSeries,
    pub(crate) completed_game_table: CompletedGameTable,
}

pub(crate) struct MatchRuntime {
    pub(crate) tournament_id: Uuid,
    pub(crate) variant: Variant,
    pub(crate) time_control: TimeControl,
    pub(crate) start_fen: String,
    pub(crate) current_fen: String,
    pub(crate) board: Board,
    pub(crate) repetitions: HashMap<u64, u8>,
    pub(crate) move_history: Vec<String>,
    pub(crate) white_time_left_ms: u64,
    pub(crate) black_time_left_ms: u64,
    pub(crate) max_plies: u16,
    pub(crate) white_seat: MatchSeatController,
    pub(crate) black_seat: MatchSeatController,
    pub(crate) logs: Vec<arena_core::GameLogEntry>,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) turn_started_server_unix_ms: i64,
    pub(crate) seq: u64,
    pub(crate) result: Option<GameResult>,
    pub(crate) termination: Option<arena_core::GameTermination>,
    pub(crate) status: MatchStatus,
}

pub(crate) enum MatchSeatController {
    Engine(EngineSeatController),
    Human(HumanSeatController),
}

pub(crate) struct EngineSeatController {
    pub(crate) adapter: Option<Box<dyn AgentAdapter>>,
}

pub(crate) struct HumanSeatController {
    pub(crate) player: HumanPlayer,
    pub(crate) command_rx: tokio::sync::mpsc::Receiver<HumanGameCommand>,
    pub(crate) seen_intents: HashMap<Uuid, HumanMoveAck>,
}

impl MatchRuntime {
    pub(crate) fn active_side(&self) -> Color {
        self.board.side_to_move()
    }

    pub(crate) fn has_human_seat(&self) -> bool {
        matches!(self.white_seat, MatchSeatController::Human(_))
            || matches!(self.black_seat, MatchSeatController::Human(_))
    }

    pub(crate) fn active_seat(&self) -> &MatchSeatController {
        if self.active_side() == Color::White {
            &self.white_seat
        } else {
            &self.black_seat
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HumanPlayer {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RequestContext {
    pub(crate) request_id: Uuid,
    pub(crate) client_action_id: Option<Uuid>,
    pub(crate) client_route: Option<String>,
    pub(crate) client_ts: Option<String>,
    pub(crate) method: String,
    pub(crate) route: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MoveDebugContext {
    pub(crate) request_id: Option<Uuid>,
    pub(crate) client_action_id: Option<Uuid>,
    pub(crate) ws_connection_id: Option<Uuid>,
    pub(crate) intent_id: Uuid,
    pub(crate) move_uci: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RequestJournalEntry {
    pub(crate) request_id: Uuid,
    pub(crate) client_action_id: Option<Uuid>,
    pub(crate) client_route: Option<String>,
    pub(crate) client_ts: Option<String>,
    pub(crate) method: String,
    pub(crate) route: String,
    pub(crate) status_code: u16,
    pub(crate) match_id: Option<Uuid>,
    pub(crate) tournament_id: Option<Uuid>,
    pub(crate) game_id: Option<Uuid>,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) completed_at: DateTime<Utc>,
    pub(crate) duration_ms: i64,
    pub(crate) error_text: Option<String>,
}

impl TournamentCoordinator {
    pub(crate) async fn start(
        &self,
        state: AppState,
        tournament_id: Uuid,
    ) -> Result<bool, ApiError> {
        let mut running = self.running.lock().await;
        if running.contains_key(&tournament_id) {
            return Ok(false);
        }

        let stop_flag = Arc::new(AtomicBool::new(false));
        running.insert(tournament_id, stop_flag.clone());
        drop(running);

        tokio::spawn({
            let coordinator = self.clone();
            async move {
                let run_result =
                    run_tournament(state.clone(), tournament_id, stop_flag.clone()).await;
                if let Err(err) = run_result {
                    error!("tournament {tournament_id} failed: {err:#}");
                    let _ = update_tournament_status(
                        &state.db,
                        tournament_id,
                        TournamentStatus::Failed,
                        None,
                        Some(Utc::now()),
                    )
                    .await;
                }
                coordinator.finish(tournament_id).await;
            }
        });

        Ok(true)
    }

    pub(crate) async fn stop(&self, tournament_id: Uuid) -> bool {
        let running = self.running.lock().await;
        if let Some(flag) = running.get(&tournament_id) {
            flag.store(true, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    async fn finish(&self, tournament_id: Uuid) {
        self.running.lock().await.remove(&tournament_id);
    }
}

impl HumanGameStore {
    pub(crate) async fn insert(&self, match_id: Uuid, session: HumanGameHandle) {
        self.sessions.write().await.insert(match_id, session);
    }

    pub(crate) async fn get(&self, match_id: Uuid) -> Option<HumanGameHandle> {
        self.sessions.read().await.get(&match_id).cloned()
    }

    pub(crate) async fn remove(&self, match_id: Uuid) -> Option<HumanGameHandle> {
        self.sessions.write().await.remove(&match_id)
    }
}

impl LiveMetricsStore {
    pub(crate) fn snapshot(&self) -> LiveMetricsSnapshot {
        LiveMetricsSnapshot {
            published_events: self.published_events.load(Ordering::Relaxed),
            replay_requests: self.replay_requests.load(Ordering::Relaxed),
            replay_events_served: self.replay_events_served.load(Ordering::Relaxed),
            snapshot_fallbacks: self.snapshot_fallbacks.load(Ordering::Relaxed),
            restored_matches: self.restored_matches.load(Ordering::Relaxed),
            websocket_connections: self.websocket_connections.load(Ordering::Relaxed),
            move_intent_errors: self.move_intent_errors.load(Ordering::Relaxed),
            timeout_fires: self.timeout_fires.load(Ordering::Relaxed),
        }
    }
}

#[derive(serde::Serialize)]
pub(crate) struct LiveMetricsSnapshot {
    pub(crate) published_events: u64,
    pub(crate) replay_requests: u64,
    pub(crate) replay_events_served: u64,
    pub(crate) snapshot_fallbacks: u64,
    pub(crate) restored_matches: u64,
    pub(crate) websocket_connections: u64,
    pub(crate) move_intent_errors: u64,
    pub(crate) timeout_fires: u64,
}
