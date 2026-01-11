//! Material-based position evaluation

use chess_core::{Color, PieceKind, Position};

/// Evaluates the position from the side-to-move's perspective.
///
/// Returns a score in centipawns:
/// - Positive = good for side to move
/// - Negative = bad for side to move
/// - 0 = equal position
pub fn evaluate(pos: &Position) -> i32 {
    let mut score = 0i32;

    for sq in 0..64u8 {
        if let Some(pc) = pos.piece_at(sq) {
            let v = piece_value(pc.kind);
            score += if pc.color == Color::White { v } else { -v };
        }
    }

    // Convert to side-to-move perspective
    if pos.side_to_move == Color::White {
        score
    } else {
        -score
    }
}

/// Returns the material value of a piece in centipawns.
#[inline]
pub fn piece_value(kind: PieceKind) -> i32 {
    match kind {
        PieceKind::Pawn => 100,
        PieceKind::Knight => 320,
        PieceKind::Bishop => 330,
        PieceKind::Rook => 500,
        PieceKind::Queen => 900,
        PieceKind::King => 0,
    }
}
