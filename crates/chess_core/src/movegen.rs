use crate::{board::Position, types::*};

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

fn pseudo_moves(pos: &Position, out: &mut Vec<Move>) {
    for sq in 0..64u8 {
        let pc = match pos.piece_at(sq) {
            Some(p) => p,
            None => continue,
        };
        if pc.color != pos.side_to_move {
            continue;
        }
        match pc.kind {
            PieceKind::Pawn => gen_pawn(pos, sq, pc.color, out),
            PieceKind::Knight => gen_knight(pos, sq, pc.color, out),
            PieceKind::Bishop => gen_slider(
                pos,
                sq,
                pc.color,
                out,
                &[(1, 1), (1, -1), (-1, 1), (-1, -1)],
            ),
            PieceKind::Rook => {
                gen_slider(pos, sq, pc.color, out, &[(1, 0), (-1, 0), (0, 1), (0, -1)])
            }
            PieceKind::Queen => gen_slider(
                pos,
                sq,
                pc.color,
                out,
                &[
                    (1, 1),
                    (1, -1),
                    (-1, 1),
                    (-1, -1),
                    (1, 0),
                    (-1, 0),
                    (0, 1),
                    (0, -1),
                ],
            ),
            PieceKind::King => {
                gen_king(pos, sq, pc.color, out);
                gen_castle(pos, sq, pc.color, out);
            }
        }
    }
}

fn gen_pawn(pos: &Position, from: u8, c: Color, out: &mut Vec<Move>) {
    let f = file_of(from);
    let r = rank_of(from);

    let dir: i8 = match c {
        Color::White => 1,
        Color::Black => -1,
    };
    let start_rank: i8 = match c {
        Color::White => 1,
        Color::Black => 6,
    };
    let promo_rank: i8 = match c {
        Color::White => 7,
        Color::Black => 0,
    };

    // forward 1
    if let Some(to) = sq(f, r + dir) {
        if pos.piece_at(to).is_none() {
            if rank_of(to) == promo_rank {
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
            } else {
                out.push(Move::new(from, to));
            }

            // forward 2 from start
            if r == start_rank {
                if let Some(to2) = sq(f, r + 2 * dir) {
                    if pos.piece_at(to2).is_none() {
                        out.push(Move::new(from, to2));
                    }
                }
            }
        }
    }

    // captures + en-passant
    for df in [-1, 1] {
        if let Some(to) = sq(f + df, r + dir) {
            if let Some(tpc) = pos.piece_at(to) {
                if tpc.color != c {
                    if rank_of(to) == promo_rank {
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
                    } else {
                        out.push(Move::new(from, to));
                    }
                }
            } else if pos.en_passant == Some(to) {
                let mut mv = Move::new(from, to);
                mv.is_en_passant = true;
                out.push(mv);
            }
        }
    }
}

fn gen_knight(pos: &Position, from: u8, c: Color, out: &mut Vec<Move>) {
    let f = file_of(from);
    let r = rank_of(from);
    let deltas = [
        (1, 2),
        (2, 1),
        (-1, 2),
        (-2, 1),
        (1, -2),
        (2, -1),
        (-1, -2),
        (-2, -1),
    ];
    for (df, dr) in deltas {
        if let Some(to) = sq(f + df, r + dr) {
            match pos.piece_at(to) {
                None => out.push(Move::new(from, to)),
                Some(pc) if pc.color != c => out.push(Move::new(from, to)),
                _ => {}
            }
        }
    }
}

fn gen_slider(pos: &Position, from: u8, c: Color, out: &mut Vec<Move>, dirs: &[(i8, i8)]) {
    let f0 = file_of(from);
    let r0 = rank_of(from);
    for (df, dr) in dirs {
        let mut f = f0 + df;
        let mut r = r0 + dr;
        while let Some(to) = sq(f, r) {
            match pos.piece_at(to) {
                None => out.push(Move::new(from, to)),
                Some(pc) if pc.color != c => {
                    out.push(Move::new(from, to));
                    break;
                }
                _ => break,
            }
            f += df;
            r += dr;
        }
    }
}

fn gen_king(pos: &Position, from: u8, c: Color, out: &mut Vec<Move>) {
    let f = file_of(from);
    let r = rank_of(from);
    let deltas = [
        (1, 1),
        (1, 0),
        (1, -1),
        (0, 1),
        (0, -1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
    ];
    for (df, dr) in deltas {
        if let Some(to) = sq(f + df, r + dr) {
            match pos.piece_at(to) {
                None => out.push(Move::new(from, to)),
                Some(pc) if pc.color != c => out.push(Move::new(from, to)),
                _ => {}
            }
        }
    }
}

fn gen_castle(pos: &Position, from: u8, c: Color, out: &mut Vec<Move>) {
    // Must be on original king square
    let (king_from, wk, wq, bk, bq) = match c {
        Color::White => (4u8, pos.castling.wk, pos.castling.wq, false, false),
        Color::Black => (60u8, false, false, pos.castling.bk, pos.castling.bq),
    };
    if from != king_from {
        return;
    }

    // Can't castle out of/through check: check squares must not be attacked.
    if pos.in_check(c) {
        return;
    }

    let enemy = c.other();
    if c == Color::White {
        // King side: e1->g1, squares f1,g1 empty and not attacked on f1,g1
        if wk && pos.piece_at(5).is_none() && pos.piece_at(6).is_none() {
            if !pos.is_square_attacked(5, enemy) && !pos.is_square_attacked(6, enemy) {
                let mut mv = Move::new(4, 6);
                mv.is_castle = true;
                out.push(mv);
            }
        }
        // Queen side: e1->c1, squares d1,c1,b1 empty; d1,c1 not attacked
        if wq && pos.piece_at(3).is_none() && pos.piece_at(2).is_none() && pos.piece_at(1).is_none()
        {
            if !pos.is_square_attacked(3, enemy) && !pos.is_square_attacked(2, enemy) {
                let mut mv = Move::new(4, 2);
                mv.is_castle = true;
                out.push(mv);
            }
        }
    } else {
        if bk && pos.piece_at(61).is_none() && pos.piece_at(62).is_none() {
            if !pos.is_square_attacked(61, enemy) && !pos.is_square_attacked(62, enemy) {
                let mut mv = Move::new(60, 62);
                mv.is_castle = true;
                out.push(mv);
            }
        }
        if bq
            && pos.piece_at(59).is_none()
            && pos.piece_at(58).is_none()
            && pos.piece_at(57).is_none()
        {
            if !pos.is_square_attacked(59, enemy) && !pos.is_square_attacked(58, enemy) {
                let mut mv = Move::new(60, 58);
                mv.is_castle = true;
                out.push(mv);
            }
        }
    }
}
