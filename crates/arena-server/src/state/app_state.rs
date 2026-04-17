use std::path::PathBuf;

use sqlx::SqlitePool;

use crate::{
    live::LiveMatchStore,
    registry::SetupRegistryCache,
    state::{HumanGameStore, LiveMetricsStore, TournamentCoordinator},
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
