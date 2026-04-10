use anyhow::Result;
use cozy_chess::{Board, Color, Move, Piece, Square};
use engine_sdk::{SearchContext, UciEngine, legal_moves, material_score, run_uci_loop};
use rand::{SeedableRng, prelude::IndexedRandom, rngs::SmallRng};

struct MiniMaxEngine {
    rng: SmallRng,
}

impl MiniMaxEngine {
    fn new() -> Self {
        Self {
            rng: SmallRng::seed_from_u64(202),
        }
    }
}

impl UciEngine for MiniMaxEngine {
    fn name(&self) -> &'static str {
        "arena-minimax"
    }

    fn choose_move(
        &mut self,
        board: &Board,
        legal_moves: &[Move],
        ctx: SearchContext,
    ) -> Result<Move> {
        let side = board.side_to_move();
        let depth = if ctx.movetime_ms >= 500 { 3 } else { 2 };
        let mut best_score = i32::MIN;
        let mut best_moves = Vec::new();
        let ordered = ordered_moves(board, legal_moves.to_vec());

        for mv in ordered {
            let mut next = board.clone();
            next.play(mv);
            let score = -negamax(&next, depth - 1, i32::MIN / 2, i32::MAX / 2, !side);
            if score > best_score {
                best_score = score;
                best_moves.clear();
                best_moves.push(mv);
            } else if score == best_score {
                best_moves.push(mv);
            }
        }

        Ok(*best_moves
            .choose(&mut self.rng)
            .expect("best move set should be non-empty"))
    }
}

fn negamax(board: &Board, depth: u8, mut alpha: i32, beta: i32, side: Color) -> i32 {
    let moves = legal_moves(board);
    if depth == 0 || moves.is_empty() {
        return quiescence(board, alpha, beta, side, 2);
    }

    let mut best = i32::MIN / 2;
    for mv in ordered_moves(board, moves) {
        let mut next = board.clone();
        next.play(mv);
        let score = -negamax(&next, depth - 1, -beta, -alpha, !side);
        best = best.max(score);
        alpha = alpha.max(score);
        if alpha >= beta {
            break;
        }
    }
    best
}

fn quiescence(board: &Board, mut alpha: i32, beta: i32, side: Color, depth: u8) -> i32 {
    let stand_pat = evaluate(board, side);
    if stand_pat >= beta {
        return beta;
    }
    alpha = alpha.max(stand_pat);
    if depth == 0 {
        return alpha;
    }

    let capture_moves: Vec<_> = legal_moves(board)
        .into_iter()
        .filter(|mv| board.color_on(mv.to).is_some())
        .collect();
    for mv in ordered_moves(board, capture_moves) {
        let mut next = board.clone();
        next.play(mv);
        let score = -quiescence(&next, -beta, -alpha, !side, depth - 1);
        if score >= beta {
            return beta;
        }
        alpha = alpha.max(score);
    }
    alpha
}

fn ordered_moves(board: &Board, mut moves: Vec<Move>) -> Vec<Move> {
    moves.sort_by_key(|mv| {
        let capture_bonus = board.piece_on(mv.to).map(piece_value).unwrap_or_default();
        let promotion_bonus = if mv.promotion.is_some() { 900 } else { 0 };
        -(capture_bonus + promotion_bonus)
    });
    moves
}

fn evaluate(board: &Board, side: Color) -> i32 {
    let mobility = legal_moves(board).len() as i32;
    material_score(board, side) + mobility * 3 + square_bonus(board, side)
}

fn square_bonus(board: &Board, side: Color) -> i32 {
    let mut score = 0;
    for square in Square::ALL {
        if board.color_on(square) != Some(side) {
            continue;
        }
        let Some(piece) = board.piece_on(square) else {
            continue;
        };
        let center_distance = (square.file() as i32 - 3).abs() + (square.rank() as i32 - 3).abs();
        let piece_weight = match piece {
            Piece::Knight | Piece::Bishop => 6,
            Piece::Pawn => 3,
            Piece::Rook => 2,
            Piece::Queen => 1,
            Piece::King => -2,
        };
        score += (6 - center_distance) * piece_weight;
    }
    score
}

fn piece_value(piece: Piece) -> i32 {
    match piece {
        Piece::Pawn => 100,
        Piece::Knight => 320,
        Piece::Bishop => 330,
        Piece::Rook => 500,
        Piece::Queen => 900,
        Piece::King => 0,
    }
}

fn main() -> Result<()> {
    run_uci_loop(&mut MiniMaxEngine::new())
}
