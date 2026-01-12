use super::*;

#[test]
fn test_from_square() {
    assert_eq!(Bitboard::from_square(0).0, 1); // a1
    assert_eq!(Bitboard::from_square(7).0, 128); // h1
    assert_eq!(Bitboard::from_square(63).0, 1 << 63); // h8
}

#[test]
fn test_popcount() {
    assert_eq!(Bitboard::EMPTY.popcount(), 0);
    assert_eq!(Bitboard::from_square(0).popcount(), 1);
    assert_eq!(Bitboard::FILE_A.popcount(), 8);
    assert_eq!(Bitboard::RANK_1.popcount(), 8);
    assert_eq!(Bitboard::ALL.popcount(), 64);
}

#[test]
fn test_iterator() {
    let bb = Bitboard(0b1010);
    let squares: Vec<u8> = bb.collect();
    assert_eq!(squares, vec![1, 3]);
}

#[test]
fn test_shifts() {
    let a1 = Bitboard::from_square(0);
    assert_eq!(a1.north(), Bitboard::from_square(8));
    assert_eq!(a1.east(), Bitboard::from_square(1));
    assert_eq!(a1.west(), Bitboard::EMPTY); // Wraps off board

    let h1 = Bitboard::from_square(7);
    assert_eq!(h1.east(), Bitboard::EMPTY); // Wraps off board
    assert_eq!(h1.west(), Bitboard::from_square(6));
}
