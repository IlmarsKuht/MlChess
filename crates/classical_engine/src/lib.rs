//! Classical Chess Engine
//!
//! Alpha-beta search with material-based evaluation.
//! This is the baseline engine for comparison with ML approaches.

mod eval;
mod search;

use chess_core::{Engine, Position, SearchResult};

/// Classical chess engine using negamax with alpha-beta pruning.
///
/// This engine uses:
/// - Negamax search with alpha-beta pruning
/// - Simple material evaluation
/// - 50-move rule and threefold repetition detection
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
    fn search(&mut self, pos: &Position, depth: u8) -> SearchResult {
        self.nodes = 0;
        let result = search::pick_best_move(pos, depth, &mut self.nodes);

        SearchResult {
            best_move: result.map(|(mv, _)| mv),
            score: result.map(|(_, s)| s).unwrap_or(0),
            depth,
            nodes: self.nodes,
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
pub use search::pick_best_move;
