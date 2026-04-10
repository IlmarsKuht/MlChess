use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use anyhow::Result;
use arena_core::{
    GameResult, LiveGameFrame, LiveGameState, MatchSeries, MatchStatus, TimeControl,
    TournamentStatus, Variant,
};
use arena_runner::AgentAdapter;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use tracing::error;
use uuid::Uuid;

use crate::{
    ApiError, orchestration::run_tournament, registry::SetupRegistryCache,
    storage::update_tournament_status,
};

#[derive(Clone)]
pub struct AppState {
    pub(crate) db: SqlitePool,
    pub(crate) coordinator: TournamentCoordinator,
    pub(crate) live_games: LiveGameStore,
    pub(crate) human_games: HumanGameStore,
    pub(crate) frontend_dist: Option<PathBuf>,
    pub(crate) setup_registry: SetupRegistryCache,
}

#[derive(Clone, Default)]
pub(crate) struct TournamentCoordinator {
    running: Arc<tokio::sync::Mutex<HashMap<Uuid, Arc<AtomicBool>>>>,
}

#[derive(Clone, Default)]
pub(crate) struct LiveGameStore {
    states: Arc<tokio::sync::RwLock<HashMap<Uuid, LiveGameState>>>,
    channels:
        Arc<tokio::sync::RwLock<HashMap<Uuid, tokio::sync::broadcast::Sender<LiveGameState>>>>,
}

#[derive(Clone, Default)]
pub(crate) struct HumanGameStore {
    sessions: Arc<tokio::sync::RwLock<HashMap<Uuid, HumanGameSession>>>,
}

#[derive(Clone)]
pub(crate) struct HumanGameSession {
    pub(crate) name: String,
    pub(crate) match_series: MatchSeries,
    pub(crate) human_player: HumanPlayer,
    pub(crate) runtime: Arc<tokio::sync::Mutex<HumanGameRuntime>>,
}

pub(crate) struct HumanGameRuntime {
    pub(crate) tournament_id: Uuid,
    pub(crate) variant: Variant,
    pub(crate) time_control: TimeControl,
    pub(crate) start_fen: String,
    pub(crate) current_fen: String,
    pub(crate) board: cozy_chess::Board,
    pub(crate) repetitions: HashMap<u64, u8>,
    pub(crate) move_history: Vec<String>,
    pub(crate) white_time_left_ms: u64,
    pub(crate) black_time_left_ms: u64,
    pub(crate) max_plies: u16,
    pub(crate) engine_side: cozy_chess::Color,
    pub(crate) engine: Box<dyn AgentAdapter>,
    pub(crate) logs: Vec<arena_core::GameLogEntry>,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) turn_started_at: Instant,
    pub(crate) result: Option<GameResult>,
    pub(crate) termination: Option<arena_core::GameTermination>,
    pub(crate) status: MatchStatus,
}

#[derive(Debug, Clone)]
pub(crate) struct HumanPlayer {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) created_at: DateTime<Utc>,
}

impl TournamentCoordinator {
    pub(crate) async fn start(&self, state: AppState, tournament_id: Uuid) -> Result<bool, ApiError> {
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

impl LiveGameStore {
    pub(crate) async fn upsert(&self, state: LiveGameState) {
        let match_id = state.match_id;
        let next_state = {
            let mut states = self.states.write().await;
            let mut next = state;
            if let Some(previous) = states.get(&match_id) {
                next.live_frames = merge_live_frames(&previous.live_frames, &next.live_frames);
            }
            states.insert(match_id, next.clone());
            next
        };
        let sender = {
            let mut channels = self.channels.write().await;
            channels
                .entry(match_id)
                .or_insert_with(|| {
                    let (sender, _) = tokio::sync::broadcast::channel(64);
                    sender
                })
                .clone()
        };
        let _ = sender.send(next_state);
    }

    pub(crate) async fn get(&self, match_id: Uuid) -> Option<LiveGameState> {
        self.states.read().await.get(&match_id).cloned()
    }

    pub(crate) async fn subscribe(
        &self,
        match_id: Uuid,
    ) -> Option<(
        LiveGameState,
        tokio::sync::broadcast::Receiver<LiveGameState>,
    )> {
        let state = self.states.read().await.get(&match_id).cloned()?;
        let receiver = {
            let mut channels = self.channels.write().await;
            channels
                .entry(match_id)
                .or_insert_with(|| {
                    let (sender, _) = tokio::sync::broadcast::channel(64);
                    sender
                })
                .subscribe()
        };
        Some((state, receiver))
    }

    pub(crate) async fn remove(&self, match_id: Uuid) {
        self.states.write().await.remove(&match_id);
        self.channels.write().await.remove(&match_id);
    }
}

fn merge_live_frames(
    previous: &[LiveGameFrame],
    incoming: &[LiveGameFrame],
) -> Vec<LiveGameFrame> {
    let mut merged = previous.to_vec();
    for frame in incoming {
        if let Some(existing) = merged.iter_mut().find(|candidate| candidate.ply == frame.ply) {
            *existing = frame.clone();
        } else {
            merged.push(frame.clone());
        }
    }
    merged.sort_by_key(|frame| frame.ply);
    merged
}

impl HumanGameStore {
    pub(crate) async fn insert(&self, session: HumanGameSession) {
        self.sessions
            .write()
            .await
            .insert(session.match_series.id, session);
    }

    pub(crate) async fn get(&self, match_id: Uuid) -> Option<HumanGameSession> {
        self.sessions.read().await.get(&match_id).cloned()
    }

    pub(crate) async fn list(&self) -> Vec<HumanGameSession> {
        self.sessions.read().await.values().cloned().collect()
    }

    pub(crate) async fn remove(&self, match_id: Uuid) -> Option<HumanGameSession> {
        self.sessions.write().await.remove(&match_id)
    }
}
