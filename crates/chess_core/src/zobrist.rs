//! Zobrist hashing for chess positions.
//!
//! Zobrist hashing enables incremental hash updates during make/unmake moves,
//! reducing hash computation from O(64) to O(1) per move. This is critical
//! for efficient repetition detection and transposition tables.
//!
//! The hash is computed by XOR-ing together random values for:
//! - Each piece on each square (12 pieces × 64 squares = 768 values)
//! - Side to move (1 value)
//! - Castling rights (4 values)
//! - En passant file (8 values)

use crate::types::Piece;

/// Pre-computed random values for Zobrist hashing.
/// Generated using a fixed seed for reproducibility.
pub struct ZobristKeys {
    /// Random values for each piece on each square.
    /// Indexed by [color][piece_kind][square]
    pub pieces: [[[u64; 64]; 6]; 2],
    /// Random value for black to move (XOR when black's turn)
    pub side_to_move: u64,
    /// Random values for castling rights [wk, wq, bk, bq]
    pub castling: [u64; 4],
    /// Random values for en passant file (0-7)
    pub en_passant: [u64; 8],
}

impl Default for ZobristKeys {
    fn default() -> Self {
        Self::new()
    }
}

impl ZobristKeys {
    /// Generate Zobrist keys using a simple PRNG with fixed seed.
    /// Uses xorshift64 for fast, reproducible random numbers.
    pub const fn new() -> Self {
        // Simple xorshift64 PRNG
        const fn xorshift64(mut state: u64) -> u64 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        }

        let mut state = 0x123456789ABCDEF0u64; // Fixed seed

        // Generate piece keys
        let mut pieces = [[[0u64; 64]; 6]; 2];
        let mut color = 0;
        while color < 2 {
            let mut piece = 0;
            while piece < 6 {
                let mut sq = 0;
                while sq < 64 {
                    state = xorshift64(state);
                    pieces[color][piece][sq] = state;
                    sq += 1;
                }
                piece += 1;
            }
            color += 1;
        }

        // Generate side to move key
        state = xorshift64(state);
        let side_to_move = state;

        // Generate castling keys
        let mut castling = [0u64; 4];
        let mut i = 0;
        while i < 4 {
            state = xorshift64(state);
            castling[i] = state;
            i += 1;
        }

        // Generate en passant keys
        let mut en_passant = [0u64; 8];
        let mut i = 0;
        while i < 8 {
            state = xorshift64(state);
            en_passant[i] = state;
            i += 1;
        }

        ZobristKeys {
            pieces,
            side_to_move,
            castling,
            en_passant,
        }
    }

    /// Get the Zobrist key for a piece on a square.
    #[inline(always)]
    pub fn piece_key(&self, piece: Piece, sq: u8) -> u64 {
        self.pieces[piece.color.idx()][piece.kind.idx()][sq as usize]
    }

    /// Get the Zobrist key for castling right index (0=wk, 1=wq, 2=bk, 3=bq).
    #[inline(always)]
    pub fn castling_key(&self, index: usize) -> u64 {
        self.castling[index]
    }

    /// Get the Zobrist key for en passant on a file (0-7).
    #[inline(always)]
    pub fn ep_key(&self, file: u8) -> u64 {
        self.en_passant[file as usize]
    }
}

/// Global static Zobrist keys, computed at compile time.
pub static ZOBRIST: ZobristKeys = ZobristKeys::new();

#[cfg(test)]
#[path = "zobrist_tests.rs"]
mod zobrist_tests;
