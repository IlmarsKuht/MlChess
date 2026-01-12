//! Bitboard representation and operations for high-performance chess.
//!
//! A bitboard is a 64-bit integer where each bit represents a square on the board.
//! Bit 0 = a1, bit 1 = b1, ..., bit 63 = h8.

use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Not, Shl, Shr};

/// A bitboard representing a set of squares on the chess board.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bitboard(pub u64);

impl Bitboard {
    pub const EMPTY: Bitboard = Bitboard(0);
    pub const ALL: Bitboard = Bitboard(!0);

    // Files
    pub const FILE_A: Bitboard = Bitboard(0x0101010101010101);
    pub const FILE_B: Bitboard = Bitboard(0x0202020202020202);
    pub const FILE_C: Bitboard = Bitboard(0x0404040404040404);
    pub const FILE_D: Bitboard = Bitboard(0x0808080808080808);
    pub const FILE_E: Bitboard = Bitboard(0x1010101010101010);
    pub const FILE_F: Bitboard = Bitboard(0x2020202020202020);
    pub const FILE_G: Bitboard = Bitboard(0x4040404040404040);
    pub const FILE_H: Bitboard = Bitboard(0x8080808080808080);

    // Ranks
    pub const RANK_1: Bitboard = Bitboard(0x00000000000000FF);
    pub const RANK_2: Bitboard = Bitboard(0x000000000000FF00);
    pub const RANK_3: Bitboard = Bitboard(0x0000000000FF0000);
    pub const RANK_4: Bitboard = Bitboard(0x00000000FF000000);
    pub const RANK_5: Bitboard = Bitboard(0x000000FF00000000);
    pub const RANK_6: Bitboard = Bitboard(0x0000FF0000000000);
    pub const RANK_7: Bitboard = Bitboard(0x00FF000000000000);
    pub const RANK_8: Bitboard = Bitboard(0xFF00000000000000);

    // Useful masks
    pub const NOT_FILE_A: Bitboard = Bitboard(!0x0101010101010101);
    pub const NOT_FILE_H: Bitboard = Bitboard(!0x8080808080808080);
    pub const NOT_FILE_AB: Bitboard = Bitboard(!0x0303030303030303);
    pub const NOT_FILE_GH: Bitboard = Bitboard(!(0x8080808080808080 | 0x4040404040404040));

    /// Create a bitboard with a single square set.
    #[inline(always)]
    pub const fn from_square(sq: u8) -> Self {
        Bitboard(1u64 << sq)
    }

    /// Check if the bitboard is empty.
    #[inline(always)]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Check if a specific square is set.
    #[inline(always)]
    pub const fn contains(self, sq: u8) -> bool {
        (self.0 & (1u64 << sq)) != 0
    }

    /// Set a square in the bitboard.
    #[inline(always)]
    pub fn set(&mut self, sq: u8) {
        self.0 |= 1u64 << sq;
    }

    /// Clear a square in the bitboard.
    #[inline(always)]
    pub fn clear(&mut self, sq: u8) {
        self.0 &= !(1u64 << sq);
    }

    /// Count the number of set bits (population count).
    #[inline(always)]
    pub const fn popcount(self) -> u32 {
        self.0.count_ones()
    }

    /// Get the index of the least significant bit (0-63), or None if empty.
    #[inline(always)]
    pub const fn lsb(self) -> Option<u8> {
        if self.0 == 0 {
            None
        } else {
            Some(self.0.trailing_zeros() as u8)
        }
    }

    /// Get and remove the least significant bit. Returns the square index.
    #[inline(always)]
    pub fn pop_lsb(&mut self) -> Option<u8> {
        if self.0 == 0 {
            None
        } else {
            let sq = self.0.trailing_zeros() as u8;
            self.0 &= self.0 - 1; // Clear the LSB
            Some(sq)
        }
    }

    /// Shift the bitboard north (toward rank 8).
    #[inline(always)]
    pub const fn north(self) -> Bitboard {
        Bitboard(self.0 << 8)
    }

    /// Shift the bitboard south (toward rank 1).
    #[inline(always)]
    pub const fn south(self) -> Bitboard {
        Bitboard(self.0 >> 8)
    }

    /// Shift the bitboard east (toward file H), masking out wrapping.
    #[inline(always)]
    pub const fn east(self) -> Bitboard {
        Bitboard((self.0 << 1) & Self::NOT_FILE_A.0)
    }

    /// Shift the bitboard west (toward file A), masking out wrapping.
    #[inline(always)]
    pub const fn west(self) -> Bitboard {
        Bitboard((self.0 >> 1) & Self::NOT_FILE_H.0)
    }

    /// Shift north-east.
    #[inline(always)]
    pub const fn north_east(self) -> Bitboard {
        Bitboard((self.0 << 9) & Self::NOT_FILE_A.0)
    }

    /// Shift north-west.
    #[inline(always)]
    pub const fn north_west(self) -> Bitboard {
        Bitboard((self.0 << 7) & Self::NOT_FILE_H.0)
    }

    /// Shift south-east.
    #[inline(always)]
    pub const fn south_east(self) -> Bitboard {
        Bitboard((self.0 >> 7) & Self::NOT_FILE_A.0)
    }

    /// Shift south-west.
    #[inline(always)]
    pub const fn south_west(self) -> Bitboard {
        Bitboard((self.0 >> 9) & Self::NOT_FILE_H.0)
    }
}

// Operator implementations for convenient bitwise operations
impl BitAnd for Bitboard {
    type Output = Self;
    #[inline(always)]
    fn bitand(self, rhs: Self) -> Self::Output {
        Bitboard(self.0 & rhs.0)
    }
}

impl BitAndAssign for Bitboard {
    #[inline(always)]
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

impl BitOr for Bitboard {
    type Output = Self;
    #[inline(always)]
    fn bitor(self, rhs: Self) -> Self::Output {
        Bitboard(self.0 | rhs.0)
    }
}

impl BitOrAssign for Bitboard {
    #[inline(always)]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitXor for Bitboard {
    type Output = Self;
    #[inline(always)]
    fn bitxor(self, rhs: Self) -> Self::Output {
        Bitboard(self.0 ^ rhs.0)
    }
}

impl BitXorAssign for Bitboard {
    #[inline(always)]
    fn bitxor_assign(&mut self, rhs: Self) {
        self.0 ^= rhs.0;
    }
}

impl Not for Bitboard {
    type Output = Self;
    #[inline(always)]
    fn not(self) -> Self::Output {
        Bitboard(!self.0)
    }
}

impl Shl<u8> for Bitboard {
    type Output = Self;
    #[inline(always)]
    fn shl(self, rhs: u8) -> Self::Output {
        Bitboard(self.0 << rhs)
    }
}

impl Shr<u8> for Bitboard {
    type Output = Self;
    #[inline(always)]
    fn shr(self, rhs: u8) -> Self::Output {
        Bitboard(self.0 >> rhs)
    }
}

/// Iterator over set bits in a bitboard.
impl Iterator for Bitboard {
    type Item = u8;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.pop_lsb()
    }
}

#[cfg(test)]
mod tests {
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
}
