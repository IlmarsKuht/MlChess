use crate::{board::Position, eval::evaluate, movegen::legal_moves_into, types::Move};

pub fn pick_best_move(pos: &Position, depth: u8) -> Option<(Move, i32)> {
    let mut tmp = pos.clone();
    let mut moves = Vec::with_capacity(64);
    legal_moves_into(&mut tmp, &mut moves);
    if moves.is_empty() {
        return None;
    }

    let mut best = moves[0];
    let mut best_score = i32::MIN + 1;

    for mv in moves {
        let undo = tmp.make_move(mv);
        let score = -negamax(
            &mut tmp,
            depth.saturating_sub(1),
            i32::MIN / 2,
            i32::MAX / 2,
        );
        tmp.unmake_move(mv, undo);

        if score > best_score {
            best_score = score;
            best = mv;
        }
    }
    Some((best, best_score))
}

fn negamax(pos: &mut Position, depth: u8, mut alpha: i32, beta: i32) -> i32 {
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
        let score = -negamax(pos, depth - 1, -beta, -alpha);
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
