use super::*;
use crate::types::{Color, PieceKind};

#[test]
fn test_zobrist_keys_unique() {
    // Verify that piece keys are unique (no collisions in small sample)
    let mut seen = std::collections::HashSet::new();

    for color in 0..2 {
        for piece in 0..6 {
            for sq in 0..64 {
                let key = ZOBRIST.pieces[color][piece][sq];
                assert!(seen.insert(key), "Duplicate Zobrist key found");
            }
        }
    }

    // Check side to move
    assert!(
        seen.insert(ZOBRIST.side_to_move),
        "Side to move key collision"
    );

    // Check castling
    for i in 0..4 {
        assert!(seen.insert(ZOBRIST.castling[i]), "Castling key collision");
    }

    // Check en passant
    for i in 0..8 {
        assert!(
            seen.insert(ZOBRIST.en_passant[i]),
            "En passant key collision"
        );
    }
}

#[test]
fn test_zobrist_piece_key() {
    let piece = Piece {
        color: Color::White,
        kind: PieceKind::Pawn,
    };
    let key1 = ZOBRIST.piece_key(piece, 0);
    let key2 = ZOBRIST.piece_key(piece, 1);
    assert_ne!(key1, key2);
}
