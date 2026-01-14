//! Negamax search with alpha-beta pruning

use chess_core::{legal_moves_into, Move, Position, TimeControl};

use crate::eval::evaluate;

/// Result from pick_best_move indicating whether search completed or was stopped.
pub struct SearchOutcome {
    /// Best move found (if any legal moves exist)
    pub best_move: Option<(Move, i32)>,
    /// True if search was stopped early due to time
    pub stopped: bool,
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
    history.push(tmp.position_hash());

    for mv in moves {
        // Check time before starting each root move
        if tc.should_check_time(*nodes) && tc.check_time() {
            stopped = true;
            break;
        }

        let undo = tmp.make_move(mv);
        history.push(tmp.position_hash());
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

    let curr_key = *history.last().unwrap_or(&pos.position_hash());
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
        history.push(pos.position_hash());
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
#[path = "search_tests.rs"]
mod search_tests;

