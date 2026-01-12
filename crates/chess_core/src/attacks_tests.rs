use super::*;

#[test]
fn test_knight_attacks() {
    // Knight on e4 (square 28) should attack 8 squares
    let attacks = knight_attacks(28);
    assert_eq!(attacks.popcount(), 8);

    // Knight on a1 (square 0) should attack 2 squares
    let attacks = knight_attacks(0);
    assert_eq!(attacks.popcount(), 2);
    assert!(attacks.contains(10)); // c2
    assert!(attacks.contains(17)); // b3

    // Knight on h1 (square 7) should attack 2 squares
    let attacks = knight_attacks(7);
    assert_eq!(attacks.popcount(), 2);
}

#[test]
fn test_king_attacks() {
    // King on e4 should attack 8 squares
    let attacks = king_attacks(28);
    assert_eq!(attacks.popcount(), 8);

    // King on a1 should attack 3 squares
    let attacks = king_attacks(0);
    assert_eq!(attacks.popcount(), 3);
}

#[test]
fn test_pawn_attacks() {
    // White pawn on e4 attacks d5 and f5
    let attacks = pawn_attacks(28, true);
    assert_eq!(attacks.popcount(), 2);
    assert!(attacks.contains(35)); // d5
    assert!(attacks.contains(37)); // f5

    // White pawn on a2 attacks only b3
    let attacks = pawn_attacks(8, true);
    assert_eq!(attacks.popcount(), 1);
    assert!(attacks.contains(17)); // b3
}

#[test]
fn test_rook_attacks_empty_board() {
    // Rook on e4 (28) on empty board
    let attacks = rook_attacks(28, Bitboard::EMPTY);
    assert_eq!(attacks.popcount(), 14); // 7 + 7 squares
}

#[test]
fn test_bishop_attacks_empty_board() {
    // Bishop on e4 (28) on empty board
    let attacks = bishop_attacks(28, Bitboard::EMPTY);
    assert_eq!(attacks.popcount(), 13);
}

#[test]
fn test_rook_attacks_with_blockers() {
    // Rook on a1, blocker on a4
    let occupied = Bitboard::from_square(24); // a4
    let attacks = rook_attacks(0, occupied);
    // Should see a2, a3, a4 (blocker), and b1-h1
    assert!(attacks.contains(8)); // a2
    assert!(attacks.contains(16)); // a3
    assert!(attacks.contains(24)); // a4 (can capture)
    assert!(!attacks.contains(32)); // a5 (blocked)
    assert!(attacks.contains(1)); // b1
    assert!(attacks.contains(7)); // h1
}
