use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::Result;
use arena_core::TournamentStatus;
use chrono::Utc;
use tracing::error;
use uuid::Uuid;

use crate::{
    ApiError,
    state::AppState,
    storage::update_tournament_status,
    tournaments::service::run_tournament,
};

#[derive(Clone, Default)]
pub(crate) struct TournamentCoordinator {
    running: Arc<tokio::sync::Mutex<HashMap<Uuid, Arc<AtomicBool>>>>,
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
