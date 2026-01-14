//! Random Move Chess Engine
//!
//! A simple engine that selects moves uniformly at random from all legal moves.
//! Useful for:
//! - Testing infrastructure before training ML models
//! - Baseline comparisons (any real engine should easily beat this)
//! - Stress testing move generation

use chess_core::{legal_moves_into, Engine, Position, SearchLimits, SearchResult};
use rand::seq::SliceRandom;
use rand::thread_rng;

#[cfg(test)]
mod lib_tests;

/// A chess engine that plays random legal moves.
///
/// This engine provides no evaluation - it simply picks a random move
/// from all available legal moves. It's the simplest possible engine
/// and serves as a baseline for testing.
#[derive(Debug, Clone, Default)]
pub struct RandomEngine {
    nodes: u64,
}

impl RandomEngine {
    pub fn new() -> Self {
        Self { nodes: 0 }
    }
}

impl Engine for RandomEngine {
    fn search(&mut self, pos: &Position, _limits: SearchLimits) -> SearchResult {
        self.nodes = 0;

        let mut pos_copy = pos.clone();
        let mut moves = Vec::with_capacity(64);
        legal_moves_into(&mut pos_copy, &mut moves);

        self.nodes = 1;

        let best_move = moves.choose(&mut thread_rng()).copied();

        SearchResult {
            best_move,
            score: 0,
            depth: 1,
            nodes: self.nodes,
            stopped: false,
        }
    }

    fn name(&self) -> &str {
        "Random v1.0"
    }

    fn author(&self) -> &str {
        "ML-chess"
    }

    fn new_game(&mut self) {
        self.nodes = 0;
    }
}
