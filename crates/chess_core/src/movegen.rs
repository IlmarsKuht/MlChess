//! Move generation using bitboards for maximum performance.
//!
//! This module generates pseudo-legal moves using bitboard operations,
//! then filters out illegal moves by checking if the king is in check.

use crate::attacks::{bishop_attacks, king_attacks, knight_attacks, queen_attacks, rook_attacks};
use crate::bitboard::Bitboard;
use crate::board::Position;
use crate::types::*;

/// Generate all legal moves, returning a freshly allocated vector.
/// Internally delegates to `legal_moves_into`, cloning the position only once.
pub fn legal_moves(pos: &Position) -> Vec<Move> {
    let mut tmp = pos.clone();
    let mut out = Vec::with_capacity(64);
    legal_moves_into(&mut tmp, &mut out);
    out
}

/// Generate all legal moves into the provided buffer, reusing it across calls.
pub fn legal_moves_into(pos: &mut Position, out: &mut Vec<Move>) {
    out.clear();
    pseudo_moves(pos, out);

    let mover = pos.side_to_move;
    // Filter illegal moves in-place by playing them on the mutable position.
    out.retain(|&mv| {
        let undo = pos.make_move(mv);
        let illegal = pos.in_check(mover);
        pos.unmake_move(mv, undo);
        !illegal
    });
}

/// Generate all pseudo-legal moves using bitboards.
fn pseudo_moves(pos: &Position, out: &mut Vec<Move>) {
    let us = pos.side_to_move;
    let them = us.other();
    let our_pieces = pos.bitboards.color(us);
    let their_pieces = pos.bitboards.color(them);
    let occupied = pos.bitboards.occupied();
    let empty = !occupied;

    // Generate moves for each piece type
    gen_pawn_moves(pos, us, our_pieces, their_pieces, empty, out);
    gen_knight_moves(pos, us, our_pieces, out);
    gen_bishop_moves(pos, us, our_pieces, occupied, out);
    gen_rook_moves(pos, us, our_pieces, occupied, out);
    gen_queen_moves(pos, us, our_pieces, occupied, out);
    gen_king_moves(pos, us, our_pieces, out);
    gen_castling_moves(pos, us, occupied, out);
}

/// Type alias for bitboard shift functions.
type ShiftFn = fn(Bitboard) -> Bitboard;

/// Generate pawn moves (pushes, double pushes, captures, en passant, promotions).
#[inline]
fn gen_pawn_moves(
    pos: &Position,
    us: Color,
    _our_pieces: Bitboard,
    their_pieces: Bitboard,
    empty: Bitboard,
    out: &mut Vec<Move>,
) {
    let pawns = pos.bitboards.pieces(us, PieceKind::Pawn);

    let (push_dir, start_rank, promo_rank, double_rank): (ShiftFn, Bitboard, Bitboard, Bitboard) =
        match us {
            Color::White => (
                Bitboard::north,
                Bitboard::RANK_2,
                Bitboard::RANK_8,
                Bitboard::RANK_4,
            ),
            Color::Black => (
                Bitboard::south,
                Bitboard::RANK_7,
                Bitboard::RANK_1,
                Bitboard::RANK_5,
            ),
        };

    let back_dir: i8 = match us {
        Color::White => -8,
        Color::Black => 8,
    };

    // Single pushes
    let single_push = push_dir(pawns) & empty;

    // Non-promotion pushes
    let mut non_promo_push = single_push & !promo_rank;
    while let Some(to) = non_promo_push.pop_lsb() {
        let from = (to as i8 + back_dir) as u8;
        out.push(Move::new(from, to));
    }

    // Promotion pushes
    let mut promo_push = single_push & promo_rank;
    while let Some(to) = promo_push.pop_lsb() {
        let from = (to as i8 + back_dir) as u8;
        add_promotions(from, to, out);
    }

    // Double pushes
    let can_double = pawns & start_rank;
    let first_push = push_dir(can_double) & empty;
    let mut double_push = push_dir(first_push) & empty & double_rank;
    while let Some(to) = double_push.pop_lsb() {
        let from = (to as i8 + 2 * back_dir) as u8;
        out.push(Move::new(from, to));
    }

    // Captures
    let (attack_left, attack_right): (ShiftFn, ShiftFn) = match us {
        Color::White => (Bitboard::north_west, Bitboard::north_east),
        Color::Black => (Bitboard::south_west, Bitboard::south_east),
    };

    let (back_left, back_right): (i8, i8) = match us {
        Color::White => (-7, -9),
        Color::Black => (9, 7),
    };

    // Left captures
    let mut left_captures = attack_left(pawns) & their_pieces & !promo_rank;
    while let Some(to) = left_captures.pop_lsb() {
        let from = (to as i8 + back_left) as u8;
        out.push(Move::new(from, to));
    }
    let mut left_promo_captures = attack_left(pawns) & their_pieces & promo_rank;
    while let Some(to) = left_promo_captures.pop_lsb() {
        let from = (to as i8 + back_left) as u8;
        add_promotions(from, to, out);
    }

    // Right captures
    let mut right_captures = attack_right(pawns) & their_pieces & !promo_rank;
    while let Some(to) = right_captures.pop_lsb() {
        let from = (to as i8 + back_right) as u8;
        out.push(Move::new(from, to));
    }
    let mut right_promo_captures = attack_right(pawns) & their_pieces & promo_rank;
    while let Some(to) = right_promo_captures.pop_lsb() {
        let from = (to as i8 + back_right) as u8;
        add_promotions(from, to, out);
    }

    // En passant
    if let Some(ep_sq) = pos.en_passant {
        let ep_bb = Bitboard::from_square(ep_sq);

        // Check pawns that can capture en passant
        if !(attack_left(pawns) & ep_bb).is_empty() {
            let from = (ep_sq as i8 + back_left) as u8;
            let mut mv = Move::new(from, ep_sq);
            mv.is_en_passant = true;
            out.push(mv);
        }
        if !(attack_right(pawns) & ep_bb).is_empty() {
            let from = (ep_sq as i8 + back_right) as u8;
            let mut mv = Move::new(from, ep_sq);
            mv.is_en_passant = true;
            out.push(mv);
        }
    }
}

#[inline]
fn add_promotions(from: u8, to: u8, out: &mut Vec<Move>) {
    for pk in [
        PieceKind::Queen,
        PieceKind::Rook,
        PieceKind::Bishop,
        PieceKind::Knight,
    ] {
        let mut mv = Move::new(from, to);
        mv.promo = Some(pk);
        out.push(mv);
    }
}

/// Generate knight moves using pre-computed attack tables.
#[inline]
fn gen_knight_moves(pos: &Position, us: Color, our_pieces: Bitboard, out: &mut Vec<Move>) {
    let mut knights = pos.bitboards.pieces(us, PieceKind::Knight);

    while let Some(from) = knights.pop_lsb() {
        let attacks = knight_attacks(from) & !our_pieces;
        let mut targets = attacks;
        while let Some(to) = targets.pop_lsb() {
            out.push(Move::new(from, to));
        }
    }
}

/// Generate bishop moves using ray attacks.
#[inline]
fn gen_bishop_moves(
    pos: &Position,
    us: Color,
    our_pieces: Bitboard,
    occupied: Bitboard,
    out: &mut Vec<Move>,
) {
    let mut bishops = pos.bitboards.pieces(us, PieceKind::Bishop);

    while let Some(from) = bishops.pop_lsb() {
        let attacks = bishop_attacks(from, occupied) & !our_pieces;
        let mut targets = attacks;
        while let Some(to) = targets.pop_lsb() {
            out.push(Move::new(from, to));
        }
    }
}

/// Generate rook moves using ray attacks.
#[inline]
fn gen_rook_moves(
    pos: &Position,
    us: Color,
    our_pieces: Bitboard,
    occupied: Bitboard,
    out: &mut Vec<Move>,
) {
    let mut rooks = pos.bitboards.pieces(us, PieceKind::Rook);

    while let Some(from) = rooks.pop_lsb() {
        let attacks = rook_attacks(from, occupied) & !our_pieces;
        let mut targets = attacks;
        while let Some(to) = targets.pop_lsb() {
            out.push(Move::new(from, to));
        }
    }
}

/// Generate queen moves using combined ray attacks.
#[inline]
fn gen_queen_moves(
    pos: &Position,
    us: Color,
    our_pieces: Bitboard,
    occupied: Bitboard,
    out: &mut Vec<Move>,
) {
    let mut queens = pos.bitboards.pieces(us, PieceKind::Queen);

    while let Some(from) = queens.pop_lsb() {
        let attacks = queen_attacks(from, occupied) & !our_pieces;
        let mut targets = attacks;
        while let Some(to) = targets.pop_lsb() {
            out.push(Move::new(from, to));
        }
    }
}

/// Generate king moves using pre-computed attack tables.
#[inline]
fn gen_king_moves(pos: &Position, us: Color, our_pieces: Bitboard, out: &mut Vec<Move>) {
    let mut kings = pos.bitboards.pieces(us, PieceKind::King);

    while let Some(from) = kings.pop_lsb() {
        let attacks = king_attacks(from) & !our_pieces;
        let mut targets = attacks;
        while let Some(to) = targets.pop_lsb() {
            out.push(Move::new(from, to));
        }
    }
}

/// Generate castling moves.
#[inline]
fn gen_castling_moves(pos: &Position, us: Color, occupied: Bitboard, out: &mut Vec<Move>) {
    // Can't castle out of check
    if pos.in_check(us) {
        return;
    }

    let enemy = us.other();

    match us {
        Color::White => {
            // King side: e1 -> g1, f1 and g1 must be empty, f1 and g1 not attacked
            if pos.castling.wk {
                let path_clear = (occupied & Bitboard(0x60)).is_empty(); // f1, g1
                if path_clear
                    && !pos.is_square_attacked(5, enemy)
                    && !pos.is_square_attacked(6, enemy)
                {
                    let mut mv = Move::new(4, 6);
                    mv.is_castle = true;
                    out.push(mv);
                }
            }
            // Queen side: e1 -> c1, b1, c1, d1 must be empty, c1 and d1 not attacked
            if pos.castling.wq {
                let path_clear = (occupied & Bitboard(0x0E)).is_empty(); // b1, c1, d1
                if path_clear
                    && !pos.is_square_attacked(2, enemy)
                    && !pos.is_square_attacked(3, enemy)
                {
                    let mut mv = Move::new(4, 2);
                    mv.is_castle = true;
                    out.push(mv);
                }
            }
        }
        Color::Black => {
            // King side: e8 -> g8
            if pos.castling.bk {
                let path_clear = (occupied & Bitboard(0x6000000000000000)).is_empty(); // f8, g8
                if path_clear
                    && !pos.is_square_attacked(61, enemy)
                    && !pos.is_square_attacked(62, enemy)
                {
                    let mut mv = Move::new(60, 62);
                    mv.is_castle = true;
                    out.push(mv);
                }
            }
            // Queen side: e8 -> c8
            if pos.castling.bq {
                let path_clear = (occupied & Bitboard(0x0E00000000000000)).is_empty(); // b8, c8, d8
                if path_clear
                    && !pos.is_square_attacked(58, enemy)
                    && !pos.is_square_attacked(59, enemy)
                {
                    let mut mv = Move::new(60, 58);
                    mv.is_castle = true;
                    out.push(mv);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_startpos_moves() {
        let pos = Position::startpos();
        let moves = legal_moves(&pos);
        // Starting position has 20 legal moves
        assert_eq!(moves.len(), 20);
    }

    #[test]
    fn test_kiwipete_moves() {
        // Kiwipete position - complex with many move types
        let pos =
            Position::from_fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -");
        let moves = legal_moves(&pos);
        assert_eq!(moves.len(), 48);
    }
}
