//! Tests for draw detection in chess
//!
//! This module tests all draw conditions:
//! - Stalemate
//! - Fifty-move rule
//! - Threefold repetition
//! - Insufficient material

use chess_core::{legal_moves_into, Position};

// =============================================================================
// Stalemate Tests
// =============================================================================

#[test]
fn test_stalemate_king_in_corner() {
    // Black king in corner, white queen stalemates
    // Position: Black king on a8, White queen on b6, White king on c7
    let pos = Position::from_fen("k7/2K5/1Q6/8/8/8/8/8 b - - 0 1");

    let mut pos_mut = pos.clone();
    let mut moves = Vec::new();
    legal_moves_into(&mut pos_mut, &mut moves);

    assert!(moves.is_empty(), "Stalemate position should have no legal moves");
    assert!(
        !pos.in_check(chess_core::Color::Black),
        "Stalemate means king is not in check"
    );
}

#[test]
fn test_stalemate_king_and_pawn_endgame() {
    // Classic king and pawn vs king stalemate
    // White king on g6, white pawn on g7, black king on g8
    let pos = Position::from_fen("6k1/6P1/6K1/8/8/8/8/8 b - - 0 1");

    let mut pos_mut = pos.clone();
    let mut moves = Vec::new();
    legal_moves_into(&mut pos_mut, &mut moves);

    assert!(moves.is_empty(), "Stalemate position should have no legal moves");
    assert!(
        !pos.in_check(chess_core::Color::Black),
        "Stalemate means king is not in check"
    );
}

// =============================================================================
// Fifty-Move Rule Tests
// =============================================================================

#[test]
fn test_fifty_move_rule_at_100_halfmoves() {
    // Position with halfmove clock at 100 (50 full moves without pawn move or capture)
    let pos = Position::from_fen("8/8/8/4k3/8/4K3/8/8 w - - 100 60");

    assert!(
        pos.is_fifty_move_draw(),
        "Position with halfmove_clock=100 should be a draw"
    );
}

#[test]
fn test_fifty_move_rule_at_99_halfmoves() {
    // Position with halfmove clock at 99 (not yet 50 full moves)
    let pos = Position::from_fen("8/8/8/4k3/8/4K3/8/8 w - - 99 60");

    assert!(
        !pos.is_fifty_move_draw(),
        "Position with halfmove_clock=99 should not be a draw yet"
    );
}

#[test]
fn test_fifty_move_rule_reset_on_pawn_move() {
    // Position with pawn on e2 and king on d3 (not blocking the pawn)
    let mut pos = Position::from_fen("8/8/8/4k3/8/3K4/4P3/8 w - - 99 60");

    // Get a copy for checking pieces (since legal_moves_into needs &mut)
    let pos_copy = pos.clone();

    // Make a pawn move (e2-e3 or e2-e4)
    let mut moves = Vec::new();
    legal_moves_into(&mut pos, &mut moves);

    // Find any pawn move - the white pawn is on e2
    let pawn_move = moves.iter().find(|m| {
        pos_copy.piece_at(m.from).map(|p| p.kind == chess_core::PieceKind::Pawn).unwrap_or(false)
    }).expect("Should have a pawn move available");
    pos.make_move(*pawn_move);

    assert!(
        !pos.is_fifty_move_draw(),
        "Pawn move should reset halfmove clock"
    );
    assert_eq!(
        pos.halfmove_clock, 0,
        "Halfmove clock should be 0 after pawn move"
    );
}

// =============================================================================
// Insufficient Material Tests
// =============================================================================

#[test]
fn test_insufficient_material_king_vs_king() {
    // Just two kings
    let pos = Position::from_fen("8/8/8/4k3/8/4K3/8/8 w - - 0 1");

    assert!(
        pos.is_insufficient_material(),
        "King vs King is insufficient material"
    );
}

#[test]
fn test_insufficient_material_king_bishop_vs_king() {
    // King and bishop vs king
    let pos = Position::from_fen("8/8/8/4k3/8/4KB2/8/8 w - - 0 1");

    assert!(
        pos.is_insufficient_material(),
        "King + Bishop vs King is insufficient material"
    );
}

#[test]
fn test_insufficient_material_king_knight_vs_king() {
    // King and knight vs king
    let pos = Position::from_fen("8/8/8/4k3/8/4KN2/8/8 w - - 0 1");

    assert!(
        pos.is_insufficient_material(),
        "King + Knight vs King is insufficient material"
    );
}

#[test]
fn test_insufficient_material_king_vs_king_bishop() {
    // King vs king and bishop (symmetric test)
    let pos = Position::from_fen("8/8/4b3/4k3/8/4K3/8/8 w - - 0 1");

    assert!(
        pos.is_insufficient_material(),
        "King vs King + Bishop is insufficient material"
    );
}

#[test]
fn test_insufficient_material_king_vs_king_knight() {
    // King vs king and knight (symmetric test)
    let pos = Position::from_fen("8/8/4n3/4k3/8/4K3/8/8 w - - 0 1");

    assert!(
        pos.is_insufficient_material(),
        "King vs King + Knight is insufficient material"
    );
}

#[test]
fn test_insufficient_material_same_color_bishops() {
    // King + light-squared bishop vs King + light-squared bishop
    // Both bishops on light squares (e.g., c1 and f8)
    let pos = Position::from_fen("5b2/8/8/4k3/8/4K3/8/2B5 w - - 0 1");

    assert!(
        pos.is_insufficient_material(),
        "K+B vs K+B with same color bishops is insufficient material"
    );
}

#[test]
fn test_sufficient_material_opposite_color_bishops() {
    // King + light-squared bishop vs King + dark-squared bishop
    // White bishop on c1 (dark), Black bishop on c8 (light)
    // c1: file=2, rank=0, sum=2 (even = dark square)
    // c8: file=2, rank=7, sum=9 (odd = light square)
    let pos = Position::from_fen("2b5/8/8/4k3/8/4K3/8/2B5 w - - 0 1");

    assert!(
        !pos.is_insufficient_material(),
        "K+B vs K+B with opposite color bishops is sufficient material (mate is possible)"
    );
}

#[test]
fn test_sufficient_material_with_pawn() {
    // King + pawn vs King
    let pos = Position::from_fen("8/8/8/4k3/8/4K3/4P3/8 w - - 0 1");

    assert!(
        !pos.is_insufficient_material(),
        "King + Pawn vs King is sufficient material"
    );
}

#[test]
fn test_sufficient_material_with_rook() {
    // King + rook vs King
    let pos = Position::from_fen("8/8/8/4k3/8/4K3/8/4R3 w - - 0 1");

    assert!(
        !pos.is_insufficient_material(),
        "King + Rook vs King is sufficient material"
    );
}

#[test]
fn test_sufficient_material_with_queen() {
    // King + queen vs King
    let pos = Position::from_fen("8/8/8/4k3/8/4K3/8/4Q3 w - - 0 1");

    assert!(
        !pos.is_insufficient_material(),
        "King + Queen vs King is sufficient material"
    );
}

#[test]
fn test_sufficient_material_two_knights() {
    // King + two knights vs King - technically sufficient (though mate is difficult)
    let pos = Position::from_fen("8/8/8/4k3/8/4K3/3NN3/8 w - - 0 1");

    assert!(
        !pos.is_insufficient_material(),
        "King + 2 Knights vs King is sufficient material (can't force mate but position isn't drawn)"
    );
}

// =============================================================================
// Position Hash Tests (for threefold repetition)
// =============================================================================

#[test]
fn test_position_hash_same_position() {
    let pos1 = Position::startpos();
    let pos2 = Position::startpos();

    assert_eq!(
        pos1.position_hash(),
        pos2.position_hash(),
        "Same positions should have same hash"
    );
}

#[test]
fn test_position_hash_different_side_to_move() {
    let pos1 = Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    let pos2 = Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR b KQkq - 0 1");

    assert_ne!(
        pos1.position_hash(),
        pos2.position_hash(),
        "Positions with different side to move should have different hashes"
    );
}

#[test]
fn test_position_hash_different_castling_rights() {
    let pos1 = Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    let pos2 = Position::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w Kq - 0 1");

    assert_ne!(
        pos1.position_hash(),
        pos2.position_hash(),
        "Positions with different castling rights should have different hashes"
    );
}

#[test]
fn test_position_hash_different_en_passant() {
    let pos1 = Position::from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1");
    let pos2 = Position::from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1");

    assert_ne!(
        pos1.position_hash(),
        pos2.position_hash(),
        "Positions with different en passant squares should have different hashes"
    );
}

#[test]
fn test_position_hash_same_after_move_sequence() {
    // Test that returning to the same position produces the same hash
    // We'll test knights shuffling back to original position

    // Position after 1.e4 e5 2.Nf3 Nc6
    let pos1 = Position::from_fen("r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3");
    let hash1 = pos1.position_hash();

    // Same position reached again after 3.Ng1 Nb8 4.Nf3 Nc6
    // The board is identical, only halfmove clock differs (which should not affect hash)
    let pos2 = Position::from_fen("r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 6 5");
    let hash2 = pos2.position_hash();

    // Same position (different halfmove clock but hash ignores that)
    assert_eq!(
        hash1, hash2,
        "Same board position should produce same hash regardless of halfmove clock"
    );
}

#[test]
fn test_threefold_repetition_detection() {
    // Simulate a threefold repetition scenario with position history
    let pos1 = Position::from_fen("r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 2 3");

    let mut history = vec![pos1.position_hash()];

    // Different positions
    let pos2 = Position::from_fen("r1bqkbnr/pppp1ppp/2n5/4p3/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 3 3");
    history.push(pos2.position_hash());

    let pos3 = Position::from_fen("rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 4 4");
    history.push(pos3.position_hash());

    // Back to pos1 equivalent
    let pos4 = Position::from_fen("r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 6 5");
    history.push(pos4.position_hash());

    // Count: pos1 appears twice now
    let count = history.iter().filter(|&&h| h == pos1.position_hash()).count();
    assert_eq!(count, 2, "Position should appear twice after one repetition");

    // More moves to another position and back
    history.push(pos2.position_hash());
    history.push(pos3.position_hash());

    // Third occurrence
    let pos7 = Position::from_fen("r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 10 7");
    history.push(pos7.position_hash());

    let count = history.iter().filter(|&&h| h == pos1.position_hash()).count();
    assert_eq!(count, 3, "Position should appear three times (threefold repetition)");
}

// =============================================================================
// Integration Tests - Not Checkmate Scenarios
// =============================================================================

#[test]
fn test_checkmate_is_not_stalemate() {
    // Scholar's mate position - this is checkmate, not stalemate
    let pos = Position::from_fen("r1bqkb1r/pppp1Qpp/2n2n2/4p3/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 0 4");

    let mut pos_mut = pos.clone();
    let mut moves = Vec::new();
    legal_moves_into(&mut pos_mut, &mut moves);

    assert!(moves.is_empty(), "Checkmate position should have no legal moves");
    assert!(
        pos.in_check(chess_core::Color::Black),
        "Checkmate means king IS in check"
    );
}

#[test]
fn test_check_is_not_checkmate() {
    // Simple check position - not checkmate
    let pos = Position::from_fen("rnbqkbnr/ppppp1pp/8/5p1Q/4P3/8/PPPP1PPP/RNB1KBNR b KQkq - 1 2");

    let mut pos_mut = pos.clone();
    let mut moves = Vec::new();
    legal_moves_into(&mut pos_mut, &mut moves);

    assert!(!moves.is_empty(), "Check position should have legal moves");
    assert!(
        pos.in_check(chess_core::Color::Black),
        "Black king should be in check"
    );
}
