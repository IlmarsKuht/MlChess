use crate::{board::Position, types::*};

pub fn evaluate(pos: &Position) -> i32 {
    // Simple material evaluation from side-to-move perspective
    let mut score = 0i32;
    for sq in 0..64u8 {
        if let Some(pc) = pos.piece_at(sq) {
            let v = match pc.kind {
                PieceKind::Pawn => 100,
                PieceKind::Knight => 320,
                PieceKind::Bishop => 330,
                PieceKind::Rook => 500,
                PieceKind::Queen => 900,
                PieceKind::King => 0,
            };
            score += if pc.color == Color::White { v } else { -v };
        }
    }
    // Convert to side-to-move
    if pos.side_to_move == Color::White {
        score
    } else {
        -score
    }
}
