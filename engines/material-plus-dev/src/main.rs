use anyhow::Result;
use cozy_chess::{Board, Color, Move, Piece, Square};
use engine_sdk::{SearchContext, UciEngine, legal_moves, material_score, run_uci_loop};
use rand::{SeedableRng, prelude::IndexedRandom, rngs::SmallRng};

struct MaterialPlusEngine {
    rng: SmallRng,
}

impl MaterialPlusEngine {
    fn new() -> Self {
        Self {
            rng: SmallRng::seed_from_u64(101),
        }
    }
}

impl UciEngine for MaterialPlusEngine {
    fn name(&self) -> &'static str {
        "arena-material-plus"
    }

    fn choose_move(
        &mut self,
        board: &Board,
        legal_moves: &[Move],
        _ctx: SearchContext,
    ) -> Result<Move> {
        let side = board.side_to_move();
        let mut best_score = i32::MIN;
        let mut best_moves = Vec::new();

        for mv in legal_moves {
            let mut next = board.clone();
            next.play(*mv);
            let score = evaluate_board(&next, side);
            if score > best_score {
                best_score = score;
                best_moves.clear();
                best_moves.push(*mv);
            } else if score == best_score {
                best_moves.push(*mv);
            }
        }

        Ok(*best_moves
            .choose(&mut self.rng)
            .expect("best move set should be non-empty"))
    }
}

fn evaluate_board(board: &Board, side: Color) -> i32 {
    let mobility = legal_moves(board).len() as i32;
    let center = center_bonus(board, side);
    let development = development_bonus(board, side);
    material_score(board, side) + mobility * 4 + center * 12 + development * 8
}

fn center_bonus(board: &Board, side: Color) -> i32 {
    [Square::D4, Square::E4, Square::D5, Square::E5]
        .into_iter()
        .map(
            |square| match (board.piece_on(square), board.color_on(square)) {
                (Some(Piece::Pawn | Piece::Knight | Piece::Bishop), Some(color))
                    if color == side =>
                {
                    2
                }
                (Some(_), Some(color)) if color == side => 1,
                (Some(_), Some(_)) => -1,
                _ => 0,
            },
        )
        .sum()
}

fn development_bonus(board: &Board, side: Color) -> i32 {
    let back_rank = if side == Color::White { 0 } else { 7 };
    let mut score = 0;
    for square in Square::ALL {
        if board.color_on(square) != Some(side) {
            continue;
        }
        let Some(piece) = board.piece_on(square) else {
            continue;
        };
        let rank = square.rank() as i32;
        if matches!(piece, Piece::Knight | Piece::Bishop) && rank != back_rank {
            score += 1;
        }
        if piece == Piece::Pawn {
            score += if side == Color::White {
                rank.saturating_sub(1)
            } else {
                6_i32.saturating_sub(rank)
            };
        }
    }
    score
}

fn main() -> Result<()> {
    run_uci_loop(&mut MaterialPlusEngine::new())
}
