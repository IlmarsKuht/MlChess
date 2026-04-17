use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RatingSnapshot {
    pub id: Uuid,
    pub pool_id: Option<Uuid>,
    pub agent_version_id: Uuid,
    pub rating: f64,
    pub games_played: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub agent_version_id: Uuid,
    pub rating: f64,
    pub games_played: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
}