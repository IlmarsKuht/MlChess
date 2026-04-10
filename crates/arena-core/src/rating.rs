use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{GameResult, LeaderboardEntry, RatingSnapshot};

pub const DEFAULT_ELO: f64 = 0.0;
pub const DEFAULT_K_FACTOR: f64 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EloConfig {
    pub default_rating: f64,
    pub k_factor: f64,
}

impl Default for EloConfig {
    fn default() -> Self {
        Self {
            default_rating: DEFAULT_ELO,
            k_factor: DEFAULT_K_FACTOR,
        }
    }
}

pub fn expected_score(player_rating: f64, opponent_rating: f64) -> f64 {
    1.0 / (1.0 + 10_f64.powf((opponent_rating - player_rating) / 400.0))
}

pub fn apply_game_result(
    white_rating: f64,
    black_rating: f64,
    result: GameResult,
    config: EloConfig,
) -> (f64, f64) {
    let white_expected = expected_score(white_rating, black_rating);
    let black_expected = expected_score(black_rating, white_rating);
    let white_next = white_rating + config.k_factor * (result.white_score() - white_expected);
    let black_next = black_rating + config.k_factor * (result.black_score() - black_expected);
    (white_next, black_next)
}

pub fn update_leaderboard_entry(
    entry: &mut LeaderboardEntry,
    opponent_rating: f64,
    result_score: f64,
    config: EloConfig,
) {
    let expected = expected_score(entry.rating, opponent_rating);
    entry.rating += config.k_factor * (result_score - expected);
    entry.games_played += 1;
    match result_score {
        score if (score - 1.0).abs() < f64::EPSILON => entry.wins += 1,
        score if (score - 0.5).abs() < f64::EPSILON => entry.draws += 1,
        _ => entry.losses += 1,
    }
}

pub fn snapshot_from_entry(pool_id: Option<Uuid>, entry: &LeaderboardEntry) -> RatingSnapshot {
    RatingSnapshot {
        id: Uuid::new_v4(),
        pool_id,
        agent_version_id: entry.agent_version_id,
        rating: entry.rating,
        games_played: entry.games_played,
        wins: entry.wins,
        draws: entry.draws,
        losses: entry.losses,
        created_at: Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_win_transfers_rating() {
        let (white, black) =
            apply_game_result(1200.0, 1200.0, GameResult::WhiteWin, EloConfig::default());
        assert!(white > 1200.0);
        assert!(black < 1200.0);
    }

    #[test]
    fn balanced_ratings_expect_half_score() {
        let expected = expected_score(1500.0, 1500.0);
        assert!((expected - 0.5).abs() < 1e-9);
    }
}
