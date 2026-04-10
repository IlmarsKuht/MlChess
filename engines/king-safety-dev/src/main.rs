use anyhow::Result;
use cozy_chess::{Board, Color, Move, Piece, Square};
use engine_sdk::{SearchContext, UciEngine, legal_moves, material_score, run_uci_loop};
use rand::{SeedableRng, prelude::IndexedRandom, rngs::SmallRng};

struct KingSafetyEngine {
    rng: SmallRng,
}

impl KingSafetyEngine {
    fn new() -> Self {
        Self {
            rng: SmallRng::seed_from_u64(303),
        }
    }
}

impl UciEngine for KingSafetyEngine {
    fn name(&self) -> &'static str {
        "arena-king-safety"
    }

    fn choose_move(&mut self, board: &Board, moves: &[Move], _ctx: SearchContext) -> Result<Move> {
        let side = board.side_to_move();
        let mut best_score = i32::MIN;
        let mut best_moves = Vec::new();

        for mv in moves {
            let mut next = board.clone();
            next.play(*mv);
            let reply_scores = legal_moves(&next)
                .into_iter()
                .map(|reply| {
                    let mut reply_board = next.clone();
                    reply_board.play(reply);
                    evaluate(&reply_board, side)
                })
                .min()
                .unwrap_or_else(|| evaluate(&next, side));

            if reply_scores > best_score {
                best_score = reply_scores;
                best_moves.clear();
                best_moves.push(*mv);
            } else if reply_scores == best_score {
                best_moves.push(*mv);
            }
        }

        Ok(*best_moves
            .choose(&mut self.rng)
            .expect("best move set should be non-empty"))
    }
}

fn evaluate(board: &Board, side: Color) -> i32 {
    material_score(board, side)
        + king_shelter(board, side) * 14
        + development(board, side) * 10
        + center_control(board, side) * 10
        + passed_pawns(board, side) * 16
}

fn king_shelter(board: &Board, side: Color) -> i32 {
    let king = board.king(side);
    let file = king.file() as i32;
    let rank = king.rank() as i32;
    let forward = if side == Color::White { 1 } else { -1 };
    let mut score = 0;

    for square in Square::ALL {
        if board.color_on(square) != Some(side) || board.piece_on(square) != Some(Piece::Pawn) {
            continue;
        }
        let pawn_file = square.file() as i32;
        let pawn_rank = square.rank() as i32;
        if (pawn_file - file).abs() <= 1 && pawn_rank == rank + forward {
            score += 2;
        } else if (pawn_file - file).abs() <= 1 && pawn_rank == rank + forward * 2 {
            score += 1;
        }
    }
    score
}

fn development(board: &Board, side: Color) -> i32 {
    let back_rank = if side == Color::White { 0 } else { 7 };
    Square::ALL
        .into_iter()
        .filter(|square| board.color_on(*square) == Some(side))
        .filter_map(|square| board.piece_on(square).map(|piece| (square, piece)))
        .map(|(square, piece)| match piece {
            Piece::Knight | Piece::Bishop if square.rank() as i32 != back_rank => 2,
            Piece::Rook if square.file() as i32 != 0 && square.file() as i32 != 7 => 1,
            _ => 0,
        })
        .sum()
}

fn center_control(board: &Board, side: Color) -> i32 {
    [
        Square::C3,
        Square::D3,
        Square::E3,
        Square::F3,
        Square::C4,
        Square::D4,
        Square::E4,
        Square::F4,
        Square::C5,
        Square::D5,
        Square::E5,
        Square::F5,
        Square::C6,
        Square::D6,
        Square::E6,
        Square::F6,
    ]
    .into_iter()
    .map(
        |square| match (board.piece_on(square), board.color_on(square)) {
            (Some(Piece::Pawn | Piece::Knight | Piece::Bishop), Some(color)) if color == side => 2,
            (Some(_), Some(color)) if color == side => 1,
            _ => 0,
        },
    )
    .sum()
}

fn passed_pawns(board: &Board, side: Color) -> i32 {
    let mut score = 0;
    for square in Square::ALL {
        if board.color_on(square) != Some(side) || board.piece_on(square) != Some(Piece::Pawn) {
            continue;
        }
        if is_passed_pawn(board, side, square) {
            score += 1;
        }
    }
    score
}

fn is_passed_pawn(board: &Board, side: Color, pawn_square: Square) -> bool {
    let file = pawn_square.file() as i32;
    let rank = pawn_square.rank() as i32;
    for square in Square::ALL {
        if board.color_on(square) != Some(!side) || board.piece_on(square) != Some(Piece::Pawn) {
            continue;
        }
        let other_file = square.file() as i32;
        let other_rank = square.rank() as i32;
        let same_lane = (other_file - file).abs() <= 1;
        let ahead = if side == Color::White {
            other_rank > rank
        } else {
            other_rank < rank
        };
        if same_lane && ahead {
            return false;
        }
    }
    true
}

fn main() -> Result<()> {
    run_uci_loop(&mut KingSafetyEngine::new())
}
