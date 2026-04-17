use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::Variant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentKind {
    RoundRobin,
    Ladder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventPresetSelectionMode {
    AllActiveEngines,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentStatus {
    Draft,
    Running,
    Completed,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeControl {
    pub initial_ms: u64,
    pub increment_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FairnessConfig {
    pub paired_games: bool,
    pub swap_colors: bool,
    pub opening_suite_id: Option<Uuid>,
    pub opening_seed: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkPool {
    pub id: Uuid,
    pub registry_key: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub variant: Variant,
    pub time_control: TimeControl,
    pub fairness: FairnessConfig,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkPoolKey {
    pub variant: Variant,
    pub initial_ms: u64,
    pub increment_ms: u64,
    pub opening_suite_id: Option<Uuid>,
    pub paired_games: bool,
    pub swap_colors: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventPreset {
    pub id: Uuid,
    pub registry_key: Option<String>,
    pub name: String,
    pub kind: TournamentKind,
    pub pool_id: Uuid,
    pub selection_mode: EventPresetSelectionMode,
    pub worker_count: u16,
    pub games_per_pairing: u16,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

impl From<&BenchmarkPool> for BenchmarkPoolKey {
    fn from(pool: &BenchmarkPool) -> Self {
        Self {
            variant: pool.variant,
            initial_ms: pool.time_control.initial_ms,
            increment_ms: pool.time_control.increment_ms,
            opening_suite_id: pool.fairness.opening_suite_id,
            paired_games: pool.fairness.paired_games,
            swap_colors: pool.fairness.swap_colors,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tournament {
    pub id: Uuid,
    pub name: String,
    pub kind: TournamentKind,
    pub pool_id: Uuid,
    pub participant_version_ids: Vec<Uuid>,
    pub worker_count: u16,
    pub games_per_pairing: u16,
    pub status: TournamentStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchSeries {
    pub id: Uuid,
    pub tournament_id: Uuid,
    pub pool_id: Uuid,
    pub round_index: u32,
    pub white_version_id: Uuid,
    pub black_version_id: Uuid,
    pub opening_id: Option<Uuid>,
    pub game_index: u32,
    pub status: MatchStatus,
    pub created_at: DateTime<Utc>,
}