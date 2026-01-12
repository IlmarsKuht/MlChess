//! Negamax search with alpha-beta pruning

use chess_core::{legal_moves_into, Color, Move, Position, TimeControl};

use crate::eval::evaluate;

/// Result from pick_best_move indicating whether search completed or was stopped.
pub struct SearchOutcome {
    /// Best move found (if any legal moves exist)
    pub best_move: Option<(Move, i32)>,
    /// True if search was stopped early due to time
    pub stopped: bool,
}

/// Computes a lightweight hash for repetition detection.
fn position_key(pos: &Position) -> u64 {
    fn mix(mut h: u64, x: u64) -> u64 {
        h ^= x;
        h = h.wrapping_mul(0x100000001b3);
        h
    }

    let mut h = 0xcbf29ce484222325u64;
    h = mix(
        h,
        match pos.side_to_move {
            Color::White => 1,
            Color::Black => 2,
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

/// Searches the position and returns the best move with its score.
///
/// # Arguments
/// * `pos` - The position to search
/// * `depth` - Maximum search depth in plies
/// * `nodes` - Counter for nodes searched (for statistics)
/// * `tc` - Time control for aborting search when time expires
///
/// # Returns
/// `SearchOutcome` containing the best move (if any) and whether search was stopped
pub fn pick_best_move(
    pos: &Position,
    depth: u8,
    nodes: &mut u64,
    tc: &TimeControl,
) -> SearchOutcome {
    let mut tmp = pos.clone();
    let mut moves = Vec::with_capacity(64);
    legal_moves_into(&mut tmp, &mut moves);

    if moves.is_empty() {
        return SearchOutcome {
            best_move: None,
            stopped: false,
        };
    }

    let mut best = moves[0];
    let mut best_score = i32::MIN + 1;
    let mut stopped = false;

    let mut history = Vec::with_capacity((depth as usize) + 1);
    history.push(position_key(&tmp));

    for mv in moves {
        // Check time before starting each root move
        if tc.should_check_time(*nodes) && tc.check_time() {
            stopped = true;
            break;
        }

        let undo = tmp.make_move(mv);
        history.push(position_key(&tmp));
        *nodes += 1;

        let (score, was_stopped) = negamax(
            &mut tmp,
            depth.saturating_sub(1),
            i32::MIN / 2,
            i32::MAX / 2,
            &mut history,
            nodes,
            tc,
        );
        let score = -score;

        history.pop();
        tmp.unmake_move(mv, undo);

        if was_stopped {
            stopped = true;
            break;
        }

        if score > best_score {
            best_score = score;
            best = mv;
        }
    }

    SearchOutcome {
        best_move: Some((best, best_score)),
        stopped,
    }
}

/// Recursive negamax search with alpha-beta pruning.
///
/// Returns (score, stopped) where stopped indicates if search was aborted due to time.
fn negamax(
    pos: &mut Position,
    depth: u8,
    mut alpha: i32,
    beta: i32,
    history: &mut Vec<u64>,
    nodes: &mut u64,
    tc: &TimeControl,
) -> (i32, bool) {
    // Check time periodically
    if tc.should_check_time(*nodes) && tc.check_time() {
        return (0, true);
    }

    // Immediate draw conditions
    if pos.is_fifty_move_draw() {
        return (0, false);
    }

    let curr_key = *history.last().unwrap_or(&position_key(pos));
    let repeats = history.iter().filter(|&&k| k == curr_key).count();
    if repeats >= 3 {
        return (0, false); // threefold repetition draw
    }

    if pos.is_insufficient_material() {
        return (0, false);
    }

    let mut moves = Vec::with_capacity(64);
    legal_moves_into(pos, &mut moves);

    if moves.is_empty() {
        if pos.in_check(pos.side_to_move) {
            return (-100_000, false); // Checkmate
        }
        return (0, false); // Stalemate
    }

    if depth == 0 {
        return (evaluate(pos), false);
    }

    let mut best = i32::MIN + 1;

    for mv in moves {
        let undo = pos.make_move(mv);
        history.push(position_key(pos));
        *nodes += 1;

        let (score, stopped) = negamax(pos, depth - 1, -beta, -alpha, history, nodes, tc);
        let score = -score;

        history.pop();
        pos.unmake_move(mv, undo);

        if stopped {
            return (best, true);
        }

        if score > best {
            best = score;
        }
        if best > alpha {
            alpha = best;
        }
        if alpha >= beta {
            break; // Beta cutoff
        }
    }

    (best, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chess_core::Position;

    #[test]
    fn test_pick_best_move_start_position() {
        let pos = Position::startpos();
        let mut nodes = 0;
        let tc = TimeControl::new(None);
        tc.start();
        let result = pick_best_move(&pos, 3, &mut nodes, &tc);
        assert!(result.best_move.is_some());
        assert!(nodes > 0);
    }

    #[test]
    fn test_pick_best_move_finds_mate_in_one() {
        // Position where Qh7# is mate in one
        let pos = Position::from_fen("6k1/5ppp/8/8/8/8/5PPP/4Q1K1 w - - 0 1");
        let mut nodes = 0;
        let tc = TimeControl::new(None);
        tc.start();
        let result = pick_best_move(&pos, 2, &mut nodes, &tc);
        assert!(result.best_move.is_some());
    }
}
