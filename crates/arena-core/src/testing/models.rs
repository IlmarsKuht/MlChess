use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{GameRecord, LeaderboardEntry};

pub type EngineId = Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchPair {
    pub engine_a: EngineId,
    pub engine_b: EngineId,
    pub games: Vec<GameRecord>,
}

impl MatchPair {
    pub fn score_for_engine(&self, engine_id: EngineId) -> f64 {
        self.games
            .iter()
            .map(|game| {
                if game.white_version_id == engine_id {
                    game.result.white_score()
                } else if game.black_version_id == engine_id {
                    game.result.black_score()
                } else {
                    0.0
                }
            })
            .sum()
    }

    pub fn score_for_engine_a(&self) -> f64 {
        self.score_for_engine(self.engine_a)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StabilityConfig {
    pub min_pairs_per_engine: u32,
    pub checkpoint_interval_pairs: u32,
    pub stable_ordering_checkpoints: usize,
    pub rating_window: usize,
    pub top_k: usize,
    pub max_rating_delta: f64,
}

impl Default for StabilityConfig {
    fn default() -> Self {
        Self {
            min_pairs_per_engine: 30,
            checkpoint_interval_pairs: 20,
            stable_ordering_checkpoints: 5,
            rating_window: 5,
            top_k: 5,
            max_rating_delta: 5.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RatingCheckpoint {
    pub pair_index: u32,
    pub top_order: Vec<EngineId>,
    pub ratings: BTreeMap<EngineId, f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopReason {
    pub min_pairs_reached: bool,
    pub ranking_stable: bool,
    pub max_rating_delta_below_threshold: bool,
}

impl StopReason {
    pub fn is_stable(&self) -> bool {
        self.min_pairs_reached && self.ranking_stable && self.max_rating_delta_below_threshold
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StabilityTracker {
    config: StabilityConfig,
    pair_counts: HashMap<EngineId, u32>,
    total_pairs: u32,
    checkpoints: Vec<RatingCheckpoint>,
}

impl StabilityTracker {
    pub fn new(config: StabilityConfig) -> Self {
        Self {
            config,
            pair_counts: HashMap::new(),
            total_pairs: 0,
            checkpoints: Vec::new(),
        }
    }

    pub fn observe_pair(&mut self, pair: &MatchPair, ratings: &HashMap<EngineId, LeaderboardEntry>) {
        *self.pair_counts.entry(pair.engine_a).or_insert(0) += 1;
        *self.pair_counts.entry(pair.engine_b).or_insert(0) += 1;
        self.total_pairs += 1;

        if self.total_pairs % self.config.checkpoint_interval_pairs == 0 {
            self.checkpoints.push(build_checkpoint(
                self.total_pairs,
                ratings,
                self.config.top_k,
            ));
        }
    }

    pub fn total_pairs(&self) -> u32 {
        self.total_pairs
    }

    pub fn pair_count_for(&self, engine_id: EngineId) -> u32 {
        self.pair_counts.get(&engine_id).copied().unwrap_or_default()
    }

    pub fn should_stop(&self, participants: &[EngineId]) -> Option<StopReason> {
        let reason = self.current_reason(participants);
        reason.is_stable().then_some(reason)
    }

    pub fn current_reason(&self, participants: &[EngineId]) -> StopReason {
        let min_pairs_reached = participants
            .iter()
            .all(|id| self.pair_count_for(*id) >= self.config.min_pairs_per_engine);

        let recent = self.recent_checkpoints();
        let ranking_stable = recent.len() == self.config.stable_ordering_checkpoints
            && recent
                .windows(2)
                .all(|window| window[0].top_order == window[1].top_order);
        let max_rating_delta_below_threshold = recent.len() == self.config.rating_window
            && max_rating_delta(recent) <= self.config.max_rating_delta;

        StopReason {
            min_pairs_reached,
            ranking_stable,
            max_rating_delta_below_threshold,
        }
    }

    fn recent_checkpoints(&self) -> &[RatingCheckpoint] {
        let needed = self
            .config
            .stable_ordering_checkpoints
            .max(self.config.rating_window);
        let start = self.checkpoints.len().saturating_sub(needed);
        &self.checkpoints[start..]
    }
}

fn build_checkpoint(
    pair_index: u32,
    ratings: &HashMap<EngineId, LeaderboardEntry>,
    top_k: usize,
) -> RatingCheckpoint {
    let mut ordered: Vec<_> = ratings.values().cloned().collect();
    ordered.sort_by(|left, right| {
        right
            .rating
            .total_cmp(&left.rating)
            .then(right.games_played.cmp(&left.games_played))
            .then_with(|| left.agent_version_id.cmp(&right.agent_version_id))
    });

    RatingCheckpoint {
        pair_index,
        top_order: ordered
            .iter()
            .take(top_k)
            .map(|entry| entry.agent_version_id)
            .collect(),
        ratings: ordered
            .into_iter()
            .map(|entry| (entry.agent_version_id, entry.rating))
            .collect(),
    }
}

fn max_rating_delta(checkpoints: &[RatingCheckpoint]) -> f64 {
    let Some(first) = checkpoints.first() else {
        return f64::INFINITY;
    };
    let Some(last) = checkpoints.last() else {
        return f64::INFINITY;
    };

    first
        .ratings
        .iter()
        .filter_map(|(engine_id, first_rating)| {
            last.ratings
                .get(engine_id)
                .map(|last_rating| (last_rating - first_rating).abs())
        })
        .fold(0.0, f64::max)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::LeaderboardEntry;

    use super::*;

    fn entry(id: EngineId, rating: f64) -> LeaderboardEntry {
        LeaderboardEntry {
            agent_version_id: id,
            rating,
            games_played: 0,
            wins: 0,
            draws: 0,
            losses: 0,
        }
    }

    #[test]
    fn stability_tracker_detects_stable_window() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let pair = MatchPair {
            engine_a: a,
            engine_b: b,
            games: Vec::new(),
        };
        let mut tracker = StabilityTracker::new(StabilityConfig {
            min_pairs_per_engine: 2,
            checkpoint_interval_pairs: 1,
            stable_ordering_checkpoints: 3,
            rating_window: 3,
            top_k: 2,
            max_rating_delta: 5.0,
        });

        for _ in 0..3 {
            tracker.observe_pair(
                &pair,
                &HashMap::from([(a, entry(a, 10.0)), (b, entry(b, 2.0))]),
            );
        }

        let reason = tracker.should_stop(&[a, b]).expect("expected stable");
        assert!(reason.min_pairs_reached);
        assert!(reason.ranking_stable);
        assert!(reason.max_rating_delta_below_threshold);
    }
}
