use crate::{board::Position, eval::evaluate, movegen::legal_moves_into, types::Move};

fn position_key(pos: &Position) -> u64 {
    // Lightweight FNV-based hash over board, side, castling, and ep for repetition detection.
    fn mix(mut h: u64, x: u64) -> u64 {
        h ^= x;
        h = h.wrapping_mul(0x100000001b3);
        h
    }

    let mut h = 0xcbf29ce484222325u64;
    h = mix(
        h,
        match pos.side_to_move {
            crate::types::Color::White => 1,
            crate::types::Color::Black => 2,
        },
    );
    h = mix(h, if pos.castling.wk { 3 } else { 5 });
    h = mix(h, if pos.castling.wq { 7 } else { 11 });
    h = mix(h, if pos.castling.bk { 13 } else { 17 });
    h = mix(h, if pos.castling.bq { 19 } else { 23 });
    if let Some(ep) = pos.en_passant {
        h = mix(h, 29 + ep as u64);
    }
    for (i, sq) in pos.board.iter().enumerate() {
        let v = if let Some(pc) = sq {
            (i as u64) ^ ((pc.color.idx() as u64) << 6) ^ ((pc.kind as u64) << 3)
        } else {
            i as u64
        };
        h = mix(h, v);
    }
    h
}

pub fn pick_best_move(pos: &Position, depth: u8) -> Option<(Move, i32)> {
    let mut tmp = pos.clone();
    let mut moves = Vec::with_capacity(64);
    legal_moves_into(&mut tmp, &mut moves);
    if moves.is_empty() {
        return None;
    }

    let mut best = moves[0];
    let mut best_score = i32::MIN + 1;

    let mut history = Vec::with_capacity((depth as usize) + 1);
    history.push(position_key(&tmp));

    for mv in moves {
        let undo = tmp.make_move(mv);
        history.push(position_key(&tmp));
        let score = -negamax(
            &mut tmp,
            depth.saturating_sub(1),
            i32::MIN / 2,
            i32::MAX / 2,
            &mut history,
        );
        history.pop();
        tmp.unmake_move(mv, undo);

        if score > best_score {
            best_score = score;
            best = mv;
        }
    }
    Some((best, best_score))
}

fn negamax(
    pos: &mut Position,
    depth: u8,
    mut alpha: i32,
    beta: i32,
    history: &mut Vec<u64>,
) -> i32 {
    // Immediate draw conditions
    if pos.halfmove_clock >= 100 {
        return 0; // 50-move rule reached
    }

    let curr_key = *history.last().unwrap_or(&position_key(pos));
    let repeats = history.iter().filter(|&&k| k == curr_key).count();
    if repeats >= 3 {
        return 0; // threefold repetition draw
    }

    let mut moves = Vec::with_capacity(64);
    legal_moves_into(pos, &mut moves);

    if moves.is_empty() {
        if pos.in_check(pos.side_to_move) {
            return -100000;
        }
        return 0;
    }
    if depth == 0 {
        return evaluate(pos);
    }

    let mut best = i32::MIN + 1;
    for mv in moves {
        let undo = pos.make_move(mv);
        history.push(position_key(pos));
        let score = -negamax(pos, depth - 1, -beta, -alpha, history);
        history.pop();
        pos.unmake_move(mv, undo);

        if score > best {
            best = score;
        }
        if best > alpha {
            alpha = best;
        }
        if alpha >= beta {
            break;
        }
    }
    best
}
