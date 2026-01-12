pub mod attacks;
pub mod bitboard;
pub mod board;
pub mod movegen;
pub mod perft;
pub mod time_control;
pub mod types;
pub mod uci;
pub mod zobrist;

// Re-export core game logic (not engine-specific)
pub use attacks::*;
pub use bitboard::*;
pub use board::*;
pub use movegen::*;
pub use perft::perft;
pub use time_control::*;
pub use types::*;
pub use uci::*;
pub use zobrist::ZOBRIST;

// =============================================================================
// Engine trait — implemented by all chess engines (classical, neural, etc.)
// =============================================================================

/// Result of a search operation
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The best move found (None if no legal moves)
    pub best_move: Option<Move>,
    /// Evaluation score in centipawns from the engine's perspective
    pub score: i32,
    /// Search depth reached
    pub depth: u8,
    /// Number of nodes searched (optional, for stats)
    pub nodes: u64,
    /// Whether search was stopped early due to time limit
    pub stopped: bool,
}

/// Trait that all chess engines must implement.
///
/// This allows swapping between classical (alpha-beta) engines,
/// neural network engines, and hybrid approaches.
pub trait Engine: Send {
    /// Search the position with the given search limits.
    ///
    /// # Arguments
    /// * `pos` - The current position to analyze
    /// * `limits` - Search limits (depth, time, etc.)
    ///
    /// # Returns
    /// SearchResult containing best move, score, and statistics
    fn search(&mut self, pos: &Position, limits: SearchLimits) -> SearchResult;

    /// Returns the engine's name for UCI identification
    fn name(&self) -> &str;

    /// Returns the engine's author for UCI identification
    fn author(&self) -> &str {
        "ML-chess"
    }

    /// Reset internal state for a new game (clear hash tables, history, etc.)
    fn new_game(&mut self) {}

    /// Optional: Set a UCI option. Returns true if the option was recognized.
    fn set_option(&mut self, _name: &str, _value: &str) -> bool {
        false
    }
}
