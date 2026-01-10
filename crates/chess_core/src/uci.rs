use crate::{board::Position, movegen::legal_moves, types::*};

pub fn move_to_uci(mv: Move) -> String {
    let mut s = String::new();
    s.push_str(&sq_to_coord(mv.from));
    s.push_str(&sq_to_coord(mv.to));
    if let Some(p) = mv.promo {
        let ch = match p {
            PieceKind::Queen => 'q',
            PieceKind::Rook => 'r',
            PieceKind::Bishop => 'b',
            PieceKind::Knight => 'n',
            _ => 'q',
        };
        s.push(ch);
    }
    s
}

pub fn parse_uci_move(pos: &Position, txt: &str) -> Option<Move> {
    // We parse and then match against legal moves so flags (castle/ep) are correct.
    if txt.len() < 4 {
        return None;
    }
    let from = coord_to_sq(&txt[0..2])?;
    let to = coord_to_sq(&txt[2..4])?;
    let promo = if txt.len() >= 5 {
        match txt.as_bytes()[4] as char {
            'q' | 'Q' => Some(PieceKind::Queen),
            'r' | 'R' => Some(PieceKind::Rook),
            'b' | 'B' => Some(PieceKind::Bishop),
            'n' | 'N' => Some(PieceKind::Knight),
            _ => None,
        }
    } else {
        None
    };

    let legals = legal_moves(pos);
    for mut m in legals {
        if m.from == from && m.to == to {
            if promo.is_some() {
                m.promo = promo;
            }
            // Must match promotion if present
            if promo.is_some() && m.promo != promo {
                continue;
            }
            return Some(m);
        }
    }
    None
}

pub fn set_position_from_uci(pos: &mut Position, args: &[&str]) {
    // Supports: "startpos" and "startpos moves ..."
    // (FEN support can be added later; startpos is enough to play.)
    if args.is_empty() {
        *pos = Position::startpos();
        return;
    }
    let mut i = 0;
    if args[i] == "startpos" {
        *pos = Position::startpos();
        i += 1;
    } else {
        // minimal fallback: if not startpos, still reset
        *pos = Position::startpos();
    }

    if i < args.len() && args[i] == "moves" {
        i += 1;
        while i < args.len() {
            if let Some(mv) = parse_uci_move(pos, args[i]) {
                pos.make_move(mv);
            }
            i += 1;
        }
    }
}
