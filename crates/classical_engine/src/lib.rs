//! Classical Chess Engine
//!
//! Alpha-beta search with material-based evaluation.
//! This is the baseline engine for comparison with ML approaches.

mod eval;
mod search;

use chess_core::{Engine, Position, SearchLimits, SearchResult};

/// Classical chess engine using negamax with alpha-beta pruning.
///
/// This engine uses:
/// - Negamax search with alpha-beta pruning
/// - Simple material evaluation
/// - 50-move rule and threefold repetition detection
/// - Time control support for move time limits
#[derive(Debug, Clone, Default)]
pub struct ClassicalEngine {
    /// Node counter for statistics
    nodes: u64,
}

impl ClassicalEngine {
    pub fn new() -> Self {
        Self { nodes: 0 }
    }
}

impl Engine for ClassicalEngine {
    fn search(&mut self, pos: &Position, limits: SearchLimits) -> SearchResult {
        self.nodes = 0;
        limits.start();

        let outcome = search::pick_best_move(pos, limits.depth, &mut self.nodes, &limits.time_control);

        SearchResult {
            best_move: outcome.best_move.map(|(mv, _)| mv),
            score: outcome.best_move.map(|(_, s)| s).unwrap_or(0),
            depth: limits.depth,
            nodes: self.nodes,
            stopped: outcome.stopped,
        }
    }

    fn name(&self) -> &str {
        "Classical v1.0"
    }

    fn author(&self) -> &str {
        "ML-chess"
    }

    fn new_game(&mut self) {
        self.nodes = 0;
    }
}

// Re-export for direct use if needed
pub use eval::evaluate;
