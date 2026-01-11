//! Tournament results storage and reporting

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::elo::MatchResult;

/// Complete tournament results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentResults {
    /// Name/description of the tournament
    pub name: String,
    /// Participating engines
    pub participants: Vec<String>,
    /// All match results (indexed by participant pairs)
    pub matches: Vec<MatchEntry>,
    /// Configuration used
    pub config: TournamentConfig,
}

/// A single match entry in the tournament
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchEntry {
    pub engine1: String,
    pub engine2: String,
    pub result: MatchResult,
}

/// Tournament configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TournamentConfig {
    pub games_per_match: u32,
    pub search_depth: u8,
    pub max_moves_per_game: u32,
}

impl Default for TournamentConfig {
    fn default() -> Self {
        Self {
            games_per_match: 10,
            search_depth: 4,
            max_moves_per_game: 200,
        }
    }
}

impl TournamentResults {
    pub fn new(name: &str, participants: Vec<String>, config: TournamentConfig) -> Self {
        Self {
            name: name.to_string(),
            participants,
            matches: Vec::new(),
            config,
        }
    }

    /// Add a match result
    pub fn add_match(&mut self, engine1: &str, engine2: &str, result: MatchResult) {
        self.matches.push(MatchEntry {
            engine1: engine1.to_string(),
            engine2: engine2.to_string(),
            result,
        });
    }

    /// Save results to JSON file
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize: {}", e))?;
        std::fs::write(path, json).map_err(|e| format!("Failed to write: {}", e))
    }

    /// Load results from JSON file
    pub fn load(path: &Path) -> Result<Self, String> {
        let contents =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read: {}", e))?;
        serde_json::from_str(&contents).map_err(|e| format!("Failed to parse: {}", e))
    }

    /// Generate a text report
    pub fn generate_report(&self) -> String {
        let mut report = String::new();
        report.push_str(&format!("=== Tournament: {} ===\n\n", self.name));
        report.push_str(&format!("Participants: {}\n", self.participants.join(", ")));
        report.push_str(&format!(
            "Config: {} games/match, depth {}\n\n",
            self.config.games_per_match, self.config.search_depth
        ));

        report.push_str("Results:\n");
        report.push_str(&format!(
            "{:<20} vs {:<20} {:>5}-{:<5}-{:<5}\n",
            "Engine 1", "Engine 2", "W", "L", "D"
        ));
        report.push_str(&"-".repeat(60));
        report.push('\n');

        for entry in &self.matches {
            report.push_str(&format!(
                "{:<20} vs {:<20} {:>5}-{:<5}-{:<5}\n",
                entry.engine1,
                entry.engine2,
                entry.result.wins,
                entry.result.losses,
                entry.result.draws
            ));
        }

        report
    }

    /// Print report to stdout
    pub fn print_report(&self) {
        println!("{}", self.generate_report());
    }
}
