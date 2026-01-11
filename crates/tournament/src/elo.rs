//! Elo rating calculation and tracking

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Default starting Elo for new engines
pub const DEFAULT_ELO: f64 = 1500.0;

/// K-factor for Elo updates (higher = more volatile)
pub const K_FACTOR: f64 = 32.0;

/// Elo rating system for tracking engine strength
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EloTracker {
    /// Ratings for each engine (by name/version)
    pub ratings: HashMap<String, f64>,
    /// Number of games played by each engine
    pub games_played: HashMap<String, u32>,
    /// Match history for analysis
    pub history: Vec<MatchRecord>,
}

/// Record of a single match result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchRecord {
    pub engine1: String,
    pub engine2: String,
    pub result: MatchResult,
    pub timestamp: String,
    pub elo_change: f64,
}

/// Result of a single game
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum GameResult {
    Win,
    Loss,
    Draw,
}

/// Result of a match (multiple games)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchResult {
    pub wins: u32,
    pub losses: u32,
    pub draws: u32,
}

impl MatchResult {
    pub fn new() -> Self {
        Self {
            wins: 0,
            losses: 0,
            draws: 0,
        }
    }

    pub fn total_games(&self) -> u32 {
        self.wins + self.losses + self.draws
    }

    /// Score from engine1's perspective (1 for win, 0.5 for draw, 0 for loss)
    pub fn score(&self) -> f64 {
        let total = self.total_games() as f64;
        if total == 0.0 {
            return 0.5;
        }
        (self.wins as f64 + 0.5 * self.draws as f64) / total
    }
}

impl Default for MatchResult {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for EloTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl EloTracker {
    pub fn new() -> Self {
        Self {
            ratings: HashMap::new(),
            games_played: HashMap::new(),
            history: Vec::new(),
        }
    }

    /// Load tracker from a JSON file
    pub fn load(path: &str) -> Result<Self, String> {
        let contents =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;
        serde_json::from_str(&contents).map_err(|e| format!("Failed to parse JSON: {}", e))
    }

    /// Save tracker to a JSON file
    pub fn save(&self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize: {}", e))?;
        std::fs::write(path, json).map_err(|e| format!("Failed to write file: {}", e))
    }

    /// Get or initialize rating for an engine
    pub fn get_rating(&mut self, engine: &str) -> f64 {
        *self.ratings.entry(engine.to_string()).or_insert(DEFAULT_ELO)
    }

    /// Calculate expected score for engine1 against engine2
    pub fn expected_score(&mut self, engine1: &str, engine2: &str) -> f64 {
        let r1 = self.get_rating(engine1);
        let r2 = self.get_rating(engine2);
        1.0 / (1.0 + 10.0_f64.powf((r2 - r1) / 400.0))
    }

    /// Update ratings after a match
    pub fn update_ratings(&mut self, engine1: &str, engine2: &str, result: &MatchResult) {
        let expected = self.expected_score(engine1, engine2);
        let actual = result.score();

        let games = result.total_games() as f64;
        let elo_change = K_FACTOR * games * (actual - expected);

        // Update ratings
        let r1 = self.get_rating(engine1);
        let r2 = self.get_rating(engine2);
        self.ratings.insert(engine1.to_string(), r1 + elo_change);
        self.ratings.insert(engine2.to_string(), r2 - elo_change);

        // Update games played
        *self.games_played.entry(engine1.to_string()).or_insert(0) += result.total_games();
        *self.games_played.entry(engine2.to_string()).or_insert(0) += result.total_games();

        // Record match
        self.history.push(MatchRecord {
            engine1: engine1.to_string(),
            engine2: engine2.to_string(),
            result: result.clone(),
            timestamp: chrono_lite_now(),
            elo_change,
        });
    }

    /// Get a sorted leaderboard
    pub fn leaderboard(&self) -> Vec<(String, f64, u32)> {
        let mut entries: Vec<_> = self
            .ratings
            .iter()
            .map(|(name, &rating)| {
                let games = self.games_played.get(name).copied().unwrap_or(0);
                (name.clone(), rating, games)
            })
            .collect();
        entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        entries
    }

    /// Print leaderboard to stdout
    pub fn print_leaderboard(&self) {
        println!("\n=== Engine Leaderboard ===");
        println!("{:<30} {:>8} {:>8}", "Engine", "Elo", "Games");
        println!("{}", "-".repeat(50));
        for (name, rating, games) in self.leaderboard() {
            println!("{:<30} {:>8.1} {:>8}", name, rating, games);
        }
        println!();
    }
}

/// Simple timestamp without external dependency
fn chrono_lite_now() -> String {
    // Use system time for basic timestamp
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elo_calculation() {
        let mut tracker = EloTracker::new();

        // Equal ratings should give 50% expected score
        let expected = tracker.expected_score("engine1", "engine2");
        assert!((expected - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_elo_update() {
        let mut tracker = EloTracker::new();

        // Engine1 wins all games
        let result = MatchResult {
            wins: 10,
            losses: 0,
            draws: 0,
        };
        tracker.update_ratings("engine1", "engine2", &result);

        assert!(tracker.get_rating("engine1") > DEFAULT_ELO);
        assert!(tracker.get_rating("engine2") < DEFAULT_ELO);
    }
}
