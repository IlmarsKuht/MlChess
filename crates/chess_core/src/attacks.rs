//! Pre-computed attack tables for fast move generation and attack detection.
//!
//! This module contains:
//! - Knight attack tables (constant)
//! - King attack tables (constant)
//! - Pawn attack tables (constant, per color)
//! - Sliding piece attacks using classical approach (no magic bitboards yet)

use crate::bitboard::Bitboard;

/// Pre-computed knight attacks for each square.
pub static KNIGHT_ATTACKS: [Bitboard; 64] = {
    let mut attacks = [Bitboard::EMPTY; 64];
    let mut sq = 0u8;
    while sq < 64 {
        let bb = Bitboard::from_square(sq);

        // Knight moves: all 8 L-shaped jumps with proper masking
        let mut result = 0u64;

        // Up 2, right 1
        result |= (bb.0 << 17) & Bitboard::NOT_FILE_A.0;
        // Up 2, left 1
        result |= (bb.0 << 15) & Bitboard::NOT_FILE_H.0;
        // Up 1, right 2
        result |= (bb.0 << 10) & Bitboard::NOT_FILE_AB.0;
        // Up 1, left 2
        result |= (bb.0 << 6) & Bitboard::NOT_FILE_GH.0;
        // Down 1, right 2
        result |= (bb.0 >> 6) & Bitboard::NOT_FILE_AB.0;
        // Down 1, left 2
        result |= (bb.0 >> 10) & Bitboard::NOT_FILE_GH.0;
        // Down 2, right 1
        result |= (bb.0 >> 15) & Bitboard::NOT_FILE_A.0;
        // Down 2, left 1
        result |= (bb.0 >> 17) & Bitboard::NOT_FILE_H.0;

        attacks[sq as usize] = Bitboard(result);
        sq += 1;
    }
    attacks
};

/// Pre-computed king attacks for each square.
pub static KING_ATTACKS: [Bitboard; 64] = {
    let mut attacks = [Bitboard::EMPTY; 64];
    let mut sq = 0u8;
    while sq < 64 {
        let bb = Bitboard::from_square(sq);

        let mut result = 0u64;

        // All 8 directions
        result |= bb.0 << 8; // North
        result |= bb.0 >> 8; // South
        result |= (bb.0 << 1) & Bitboard::NOT_FILE_A.0; // East
        result |= (bb.0 >> 1) & Bitboard::NOT_FILE_H.0; // West
        result |= (bb.0 << 9) & Bitboard::NOT_FILE_A.0; // North-East
        result |= (bb.0 << 7) & Bitboard::NOT_FILE_H.0; // North-West
        result |= (bb.0 >> 7) & Bitboard::NOT_FILE_A.0; // South-East
        result |= (bb.0 >> 9) & Bitboard::NOT_FILE_H.0; // South-West

        attacks[sq as usize] = Bitboard(result);
        sq += 1;
    }
    attacks
};

/// Pre-computed pawn attacks for White (attacking north-east and north-west).
pub static WHITE_PAWN_ATTACKS: [Bitboard; 64] = {
    let mut attacks = [Bitboard::EMPTY; 64];
    let mut sq = 0u8;
    while sq < 64 {
        let bb = Bitboard::from_square(sq);

        let mut result = 0u64;
        result |= (bb.0 << 9) & Bitboard::NOT_FILE_A.0; // North-East
        result |= (bb.0 << 7) & Bitboard::NOT_FILE_H.0; // North-West

        attacks[sq as usize] = Bitboard(result);
        sq += 1;
    }
    attacks
};

/// Pre-computed pawn attacks for Black (attacking south-east and south-west).
pub static BLACK_PAWN_ATTACKS: [Bitboard; 64] = {
    let mut attacks = [Bitboard::EMPTY; 64];
    let mut sq = 0u8;
    while sq < 64 {
        let bb = Bitboard::from_square(sq);

        let mut result = 0u64;
        result |= (bb.0 >> 7) & Bitboard::NOT_FILE_A.0; // South-East
        result |= (bb.0 >> 9) & Bitboard::NOT_FILE_H.0; // South-West

        attacks[sq as usize] = Bitboard(result);
        sq += 1;
    }
    attacks
};

/// Get pawn attacks for a given color and square.
#[inline(always)]
pub fn pawn_attacks(sq: u8, is_white: bool) -> Bitboard {
    if is_white {
        WHITE_PAWN_ATTACKS[sq as usize]
    } else {
        BLACK_PAWN_ATTACKS[sq as usize]
    }
}

/// Get knight attacks for a given square.
#[inline(always)]
pub fn knight_attacks(sq: u8) -> Bitboard {
    KNIGHT_ATTACKS[sq as usize]
}

/// Get king attacks for a given square.
#[inline(always)]
pub fn king_attacks(sq: u8) -> Bitboard {
    KING_ATTACKS[sq as usize]
}

// =============================================================================
// Sliding piece attacks (classical ray approach - still fast but simpler than magic)
// =============================================================================

/// Pre-computed ray attacks in each direction.
/// RAYS[direction][square] gives all squares in that direction from sq (not including sq).
/// Directions: 0=N, 1=NE, 2=E, 3=SE, 4=S, 5=SW, 6=W, 7=NW
pub static RAYS: [[Bitboard; 64]; 8] = {
    let mut rays = [[Bitboard::EMPTY; 64]; 8];

    let mut sq = 0u8;
    while sq < 64 {
        let file = sq % 8;
        let rank = sq / 8;

        // North (direction 0)
        {
            let mut bb = 0u64;
            let mut r = rank + 1;
            while r < 8 {
                bb |= 1u64 << (r * 8 + file);
                r += 1;
            }
            rays[0][sq as usize] = Bitboard(bb);
        }

        // North-East (direction 1)
        {
            let mut bb = 0u64;
            let mut r = rank + 1;
            let mut f = file + 1;
            while r < 8 && f < 8 {
                bb |= 1u64 << (r * 8 + f);
                r += 1;
                f += 1;
            }
            rays[1][sq as usize] = Bitboard(bb);
        }

        // East (direction 2)
        {
            let mut bb = 0u64;
            let mut f = file + 1;
            while f < 8 {
                bb |= 1u64 << (rank * 8 + f);
                f += 1;
            }
            rays[2][sq as usize] = Bitboard(bb);
        }

        // South-East (direction 3)
        {
            let mut bb = 0u64;
            let mut r = rank.wrapping_sub(1);
            let mut f = file + 1;
            while r < 8 && f < 8 {
                bb |= 1u64 << (r * 8 + f);
                r = r.wrapping_sub(1);
                f += 1;
            }
            rays[3][sq as usize] = Bitboard(bb);
        }

        // South (direction 4)
        {
            let mut bb = 0u64;
            let mut r = rank.wrapping_sub(1);
            while r < 8 {
                bb |= 1u64 << (r * 8 + file);
                r = r.wrapping_sub(1);
            }
            rays[4][sq as usize] = Bitboard(bb);
        }

        // South-West (direction 5)
        {
            let mut bb = 0u64;
            let mut r = rank.wrapping_sub(1);
            let mut f = file.wrapping_sub(1);
            while r < 8 && f < 8 {
                bb |= 1u64 << (r * 8 + f);
                r = r.wrapping_sub(1);
                f = f.wrapping_sub(1);
            }
            rays[5][sq as usize] = Bitboard(bb);
        }

        // West (direction 6)
        {
            let mut bb = 0u64;
            let mut f = file.wrapping_sub(1);
            while f < 8 {
                bb |= 1u64 << (rank * 8 + f);
                f = f.wrapping_sub(1);
            }
            rays[6][sq as usize] = Bitboard(bb);
        }

        // North-West (direction 7)
        {
            let mut bb = 0u64;
            let mut r = rank + 1;
            let mut f = file.wrapping_sub(1);
            while r < 8 && f < 8 {
                bb |= 1u64 << (r * 8 + f);
                r += 1;
                f = f.wrapping_sub(1);
            }
            rays[7][sq as usize] = Bitboard(bb);
        }

        sq += 1;
    }
    rays
};

/// Calculate bishop attacks given a square and occupied squares.
#[inline]
pub fn bishop_attacks(sq: u8, occupied: Bitboard) -> Bitboard {
    let mut attacks = Bitboard::EMPTY;

    // Positive rays (NE=1, NW=7): find first blocker, mask everything beyond
    for &dir in &[1, 7] {
        let ray = RAYS[dir][sq as usize];
        let blockers = ray & occupied;
        if let Some(blocker_sq) = blockers.lsb() {
            // Include the blocker, exclude everything beyond
            attacks |= ray & !RAYS[dir][blocker_sq as usize];
        } else {
            attacks |= ray;
        }
    }

    // Negative rays (SE=3, SW=5): find first blocker from MSB side
    for &dir in &[3, 5] {
        let ray = RAYS[dir][sq as usize];
        let blockers = ray & occupied;
        if blockers.0 != 0 {
            // For negative rays, we need the MSB (highest square in the ray)
            let blocker_sq = 63 - blockers.0.leading_zeros() as u8;
            attacks |= ray & !RAYS[dir][blocker_sq as usize];
        } else {
            attacks |= ray;
        }
    }

    attacks
}

/// Calculate rook attacks given a square and occupied squares.
#[inline]
pub fn rook_attacks(sq: u8, occupied: Bitboard) -> Bitboard {
    let mut attacks = Bitboard::EMPTY;

    // Positive rays (N=0, E=2): find first blocker (LSB)
    for &dir in &[0, 2] {
        let ray = RAYS[dir][sq as usize];
        let blockers = ray & occupied;
        if let Some(blocker_sq) = blockers.lsb() {
            attacks |= ray & !RAYS[dir][blocker_sq as usize];
        } else {
            attacks |= ray;
        }
    }

    // Negative rays (S=4, W=6): find first blocker (MSB)
    for &dir in &[4, 6] {
        let ray = RAYS[dir][sq as usize];
        let blockers = ray & occupied;
        if blockers.0 != 0 {
            let blocker_sq = 63 - blockers.0.leading_zeros() as u8;
            attacks |= ray & !RAYS[dir][blocker_sq as usize];
        } else {
            attacks |= ray;
        }
    }

    attacks
}

/// Calculate queen attacks (union of bishop and rook attacks).
#[inline]
pub fn queen_attacks(sq: u8, occupied: Bitboard) -> Bitboard {
    bishop_attacks(sq, occupied) | rook_attacks(sq, occupied)
}

#[cfg(test)]
#[path = "attacks_tests.rs"]
mod attacks_tests;

