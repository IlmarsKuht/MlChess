//! Material-based position evaluation using bitboards.

use chess_core::{Color, PieceKind, Position};

/// Material values in centipawns, indexed by PieceKind::idx().
/// Order: Pawn, Knight, Bishop, Rook, Queen, King
const PIECE_VALUES: [i32; 6] = [100, 320, 330, 500, 900, 0];

/// Evaluates the position from the side-to-move's perspective.
///
/// Uses bitboard popcount for efficient material counting.
///
/// Returns a score in centipawns:
/// - Positive = good for side to move
/// - Negative = bad for side to move
/// - 0 = equal position
pub fn evaluate(pos: &Position) -> i32 {
    let mut score = 0i32;

    // Count material using bitboard popcount (much faster than mailbox iteration)
    for kind in PieceKind::ALL {
        let value = PIECE_VALUES[kind.idx()];
        let white_count = pos.bitboards.pieces(Color::White, kind).popcount() as i32;
        let black_count = pos.bitboards.pieces(Color::Black, kind).popcount() as i32;
        score += value * (white_count - black_count);
    }

    // Convert to side-to-move perspective
    if pos.side_to_move == Color::White {
        score
    } else {
        -score
    }
}
