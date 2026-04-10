use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{LeaderboardEntry, expected_score};

use super::{EngineId, MatchPair};

pub const DEFAULT_TEST_RATING: f64 = 0.0;
pub const DEFAULT_TEST_K_FACTOR: f64 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PairEloConfig {
    pub default_rating: f64,
    pub k_factor: f64,
}

impl Default for PairEloConfig {
    fn default() -> Self {
        Self {
            default_rating: DEFAULT_TEST_RATING,
            k_factor: DEFAULT_TEST_K_FACTOR,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PairRatingUpdate {
    pub engine_a: LeaderboardEntry,
    pub engine_b: LeaderboardEntry,
}

pub fn default_rating_entry(engine_id: EngineId, config: PairEloConfig) -> LeaderboardEntry {
    LeaderboardEntry {
        agent_version_id: engine_id,
        rating: config.default_rating,
        games_played: 0,
        wins: 0,
        draws: 0,
        losses: 0,
    }
}

pub fn apply_match_pair(
    existing_entries: &HashMap<EngineId, LeaderboardEntry>,
    pair: &MatchPair,
    config: PairEloConfig,
) -> PairRatingUpdate {
    let engine_a_current = existing_entries
        .get(&pair.engine_a)
        .cloned()
        .unwrap_or_else(|| default_rating_entry(pair.engine_a, config));
    let engine_b_current = existing_entries
        .get(&pair.engine_b)
        .cloned()
        .unwrap_or_else(|| default_rating_entry(pair.engine_b, config));

    let engine_a_score = pair.score_for_engine_a();
    let expected_a = 2.0 * expected_score(engine_a_current.rating, engine_b_current.rating);
    let rating_delta = config.k_factor * (engine_a_score - expected_a);

    let mut engine_a = engine_a_current;
    let mut engine_b = engine_b_current;
    engine_a.rating += rating_delta;
    engine_b.rating -= rating_delta;

    for game in &pair.games {
        engine_a.games_played += 1;
        engine_b.games_played += 1;

        let a_score = if game.white_version_id == pair.engine_a {
            game.result.white_score()
        } else {
            game.result.black_score()
        };

        match a_score {
            score if (score - 1.0).abs() < f64::EPSILON => {
                engine_a.wins += 1;
                engine_b.losses += 1;
            }
            score if (score - 0.5).abs() < f64::EPSILON => {
                engine_a.draws += 1;
                engine_b.draws += 1;
            }
            _ => {
                engine_a.losses += 1;
                engine_b.wins += 1;
            }
        }
    }

    PairRatingUpdate { engine_a, engine_b }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use crate::{GameRecord, GameResult, GameTermination, Variant};

    use super::*;

    fn game(white: EngineId, black: EngineId, result: GameResult) -> GameRecord {
        GameRecord {
            id: Uuid::new_v4(),
            tournament_id: Uuid::new_v4(),
            match_id: Uuid::new_v4(),
            pool_id: Uuid::new_v4(),
            variant: Variant::Standard,
            opening_id: None,
            white_version_id: white,
            black_version_id: black,
            result,
            termination: GameTermination::Unknown,
            start_fen: "startpos".to_string(),
            pgn: String::new(),
            moves_uci: Vec::new(),
            white_time_left_ms: 0,
            black_time_left_ms: 0,
            logs: Vec::new(),
            started_at: Utc::now(),
            completed_at: Utc::now(),
        }
    }

    #[test]
    fn pair_update_uses_combined_pair_score() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let pair = MatchPair {
            engine_a: a,
            engine_b: b,
            games: vec![
                game(a, b, GameResult::WhiteWin),
                game(b, a, GameResult::Draw),
            ],
        };

        let update = apply_match_pair(&HashMap::new(), &pair, PairEloConfig::default());

        assert_eq!(update.engine_a.games_played, 2);
        assert_eq!(update.engine_b.games_played, 2);
        assert!(update.engine_a.rating > update.engine_b.rating);
    }
}
