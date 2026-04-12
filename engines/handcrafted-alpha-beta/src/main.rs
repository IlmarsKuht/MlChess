use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use anyhow::Result;
use cozy_chess::{
    BitBoard, Board, Color, File, GameStatus, Move, Piece, Rank, Square, get_bishop_moves,
    get_king_moves, get_knight_moves, get_pawn_attacks, get_rook_moves,
};
use engine_sdk::{SearchContext, UciEngine, legal_moves, run_uci_loop};

const MAX_DEPTH: i32 = 32;
const MAX_PLY: usize = 96;
const MATE_SCORE: i32 = 30_000;
const DRAW_SCORE: i32 = 0;
const PHASE_MAX: i32 = 24;
const TIME_CHECK_INTERVAL: u64 = 64;

const MG_VALUE: [i32; 6] = [82, 337, 365, 477, 1025, 0];
const EG_VALUE: [i32; 6] = [94, 281, 297, 512, 936, 0];
const PHASE_VALUE: [i32; 6] = [0, 1, 1, 2, 4, 0];

const FILE_MASKS: [u64; 8] = [
    0x0101_0101_0101_0101,
    0x0202_0202_0202_0202,
    0x0404_0404_0404_0404,
    0x0808_0808_0808_0808,
    0x1010_1010_1010_1010,
    0x2020_2020_2020_2020,
    0x4040_4040_4040_4040,
    0x8080_8080_8080_8080,
];

const MG_PAWN_PST: [i32; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 98, 134, 61, 95, 68, 126, 34, -11, -6, 7, 26, 31, 65, 56, 25, -20, -14,
    13, 6, 21, 23, 12, 17, -23, -27, -2, -5, 12, 17, 6, 10, -25, -26, -4, -4, -10, 3, 3, 33, -12,
    -35, -1, -20, -23, -15, 24, 38, -22, 0, 0, 0, 0, 0, 0, 0, 0,
];

const EG_PAWN_PST: [i32; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0, 178, 173, 158, 134, 147, 132, 165, 187, 94, 100, 85, 67, 56, 53, 82,
    84, 32, 24, 13, 5, -2, 4, 17, 17, 13, 9, -3, -7, -7, -8, 3, -1, 4, 7, -6, 1, 0, -5, -1, -8, 13,
    8, 8, 10, 13, 0, 2, -7, 0, 0, 0, 0, 0, 0, 0, 0,
];

const MG_KNIGHT_PST: [i32; 64] = [
    -167, -89, -34, -49, 61, -97, -15, -107, -73, -41, 72, 36, 23, 62, 7, -17, -47, 60, 37, 65, 84,
    129, 73, 44, -9, 17, 19, 53, 37, 69, 18, 22, -13, 4, 16, 13, 28, 19, 21, -8, -23, -9, 12, 10,
    19, 17, 25, -16, -29, -53, -12, -3, -1, 18, -14, -19, -105, -21, -58, -33, -17, -28, -19, -23,
];

const EG_KNIGHT_PST: [i32; 64] = [
    -58, -38, -13, -28, -31, -27, -63, -99, -25, -8, -25, -2, -9, -25, -24, -52, -24, -20, 10, 9,
    -1, -9, -19, -41, -17, 3, 22, 22, 22, 11, 8, -18, -18, -6, 16, 25, 16, 17, 4, -18, -23, -3, -1,
    15, 10, -3, -20, -22, -42, -20, -10, -5, -2, -20, -23, -44, -29, -51, -23, -15, -22, -18, -50,
    -64,
];

const MG_BISHOP_PST: [i32; 64] = [
    -29, 4, -82, -37, -25, -42, 7, -8, -26, 16, -18, -13, 30, 59, 18, -47, -16, 37, 43, 40, 35, 50,
    37, -2, -4, 5, 19, 50, 37, 37, 7, -2, -6, 13, 13, 26, 34, 12, 10, 4, 0, 15, 15, 15, 14, 27, 18,
    10, 4, 15, 16, 0, 7, 21, 33, 1, -33, -3, -14, -21, -13, -12, -39, -21,
];

const EG_BISHOP_PST: [i32; 64] = [
    -14, -21, -11, -8, -7, -9, -17, -24, -8, -4, 7, -12, -3, -13, -4, -14, 2, -8, 0, -1, -2, 6, 0,
    4, -3, 9, 12, 9, 14, 10, 3, 2, -6, 3, 13, 19, 7, 10, -3, -9, -12, -3, 8, 10, 13, 3, -7, -15,
    -14, -18, -7, -1, 4, -9, -15, -27, -23, -9, -23, -5, -9, -16, -5, -17,
];

const MG_ROOK_PST: [i32; 64] = [
    32, 42, 32, 51, 63, 9, 31, 43, 27, 32, 58, 62, 80, 67, 26, 44, -5, 19, 26, 36, 17, 45, 61, 16,
    -24, -11, 7, 26, 24, 35, -8, -20, -36, -26, -12, -1, 9, -7, 6, -23, -45, -25, -16, -17, 3, 0,
    -5, -33, -44, -16, -20, -9, -1, 11, -6, -71, -19, -13, 1, 17, 16, 7, -37, -26,
];

const EG_ROOK_PST: [i32; 64] = [
    13, 10, 18, 15, 12, 12, 8, 5, 11, 13, 13, 11, -3, 3, 8, 3, 7, 7, 7, 5, 4, -3, -5, -3, 4, 3, 13,
    1, 2, 1, -1, 2, 3, 5, 8, 4, -5, -6, -8, -11, -4, 0, -5, -1, -7, -12, -8, -16, -6, -6, 0, 2, -9,
    -9, -11, -3, -9, 2, 3, -1, -5, -13, 4, -20,
];

const MG_QUEEN_PST: [i32; 64] = [
    -28, 0, 29, 12, 59, 44, 43, 45, -24, -39, -5, 1, -16, 57, 28, 54, -13, -17, 7, 8, 29, 56, 47,
    57, -27, -27, -16, -16, -1, 17, -2, 1, -9, -26, -9, -10, -2, -4, 3, -3, -14, 2, -11, -2, -5, 2,
    14, 5, -35, -8, 11, 2, 8, 15, -3, 1, -1, -18, -9, 10, -15, -25, -31, -50,
];

const EG_QUEEN_PST: [i32; 64] = [
    -9, 22, 22, 27, 27, 19, 10, 20, -17, 20, 32, 41, 58, 25, 30, 0, -20, 6, 9, 49, 47, 35, 19, 9,
    3, 22, 24, 45, 57, 40, 57, 36, -18, 28, 19, 47, 31, 34, 39, 23, -16, -27, 15, 6, 9, 17, 10, 5,
    -22, -23, -30, -16, -16, -23, -36, -32, -33, -28, -22, -43, -5, -32, -20, -41,
];

const MG_KING_PST: [i32; 64] = [
    -65, 23, 16, -15, -56, -34, 2, 13, 29, -1, -20, -7, -8, -4, -38, -29, -9, 24, 2, -16, -20, 6,
    22, -22, -17, -20, -12, -27, -30, -25, -14, -36, -49, -1, -27, -39, -46, -44, -33, -51, -14,
    -14, -22, -46, -44, -30, -15, -27, 1, 7, -8, -64, -43, -16, 9, 8, -15, 36, 12, -54, 8, -28, 24,
    14,
];

const EG_KING_PST: [i32; 64] = [
    -74, -35, -18, -18, -11, 15, 4, -17, -12, 17, 14, 17, 17, 38, 23, 11, 10, 17, 23, 15, 20, 45,
    44, 13, -8, 22, 24, 27, 26, 33, 26, 3, -18, -4, 21, 24, 27, 23, 9, -11, -19, -3, 11, 21, 23,
    16, 7, -9, -27, -11, 4, 13, 14, 4, -5, -17, -53, -34, -21, -11, -28, -14, -24, -43,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bound {
    Exact,
    Lower,
    Upper,
}

#[derive(Clone, Copy, Debug)]
struct TranspositionEntry {
    depth: i32,
    score: i32,
    bound: Bound,
    best_move: Option<Move>,
}

struct HandcraftedAlphaBetaEngine {
    tt: HashMap<u64, TranspositionEntry>,
    killer_moves: [[Option<Move>; 2]; MAX_PLY],
    history: [[[i32; 64]; 64]; 2],
}

impl HandcraftedAlphaBetaEngine {
    fn new() -> Self {
        Self {
            tt: HashMap::new(),
            killer_moves: [[None; 2]; MAX_PLY],
            history: [[[0; 64]; 64]; 2],
        }
    }
}

impl UciEngine for HandcraftedAlphaBetaEngine {
    fn name(&self) -> &'static str {
        "arena-handcrafted-alpha-beta"
    }

    fn choose_move(&mut self, board: &Board, legal: &[Move], ctx: SearchContext) -> Result<Move> {
        let safety_margin = ctx.movetime_ms.min(30);
        let budget_ms = ctx.movetime_ms.saturating_sub(safety_margin).max(20);
        let deadline = Instant::now() + Duration::from_millis(budget_ms);
        let mut repetition = HashMap::<u64, u8>::new();
        for hash in ctx.position_history_hashes {
            *repetition.entry(hash).or_insert(0) += 1;
        }

        let mut searcher = Searcher {
            engine: self,
            deadline,
            stopped: false,
            node_count: 0,
            repetition,
        };

        let mut best_move = legal[0];
        let mut best_score = i32::MIN / 4;

        for depth in 1..=MAX_DEPTH {
            if Instant::now() >= deadline {
                break;
            }

            if let Some((candidate, score)) = searcher.search_root(board, legal, depth) {
                best_move = candidate;
                best_score = score;
            }

            if searcher.stopped || is_forced_mate_score(best_score) {
                break;
            }
        }

        Ok(best_move)
    }
}

struct Searcher<'a> {
    engine: &'a mut HandcraftedAlphaBetaEngine,
    deadline: Instant,
    stopped: bool,
    node_count: u64,
    repetition: HashMap<u64, u8>,
}

impl Searcher<'_> {
    fn search_root(&mut self, board: &Board, legal: &[Move], depth: i32) -> Option<(Move, i32)> {
        let tt_move = self
            .engine
            .tt
            .get(&board.hash())
            .and_then(|entry| entry.best_move);
        let ordered = self.order_moves(board, legal.to_vec(), tt_move, 0);
        let mut best_move = None;
        let mut best_score = i32::MIN / 4;
        let mut alpha = i32::MIN / 4;
        let beta = i32::MAX / 4;

        for (index, mv) in ordered.into_iter().enumerate() {
            if self.should_stop() {
                return best_move.map(|candidate| (candidate, best_score));
            }

            let mut next = board.clone();
            next.play(mv);
            self.push_repetition(next.hash());

            let mut score = if index == 0 {
                -self.pvs(&next, depth - 1, 1, -beta, -alpha)
            } else {
                let scout = -self.pvs(&next, depth - 1, 1, -alpha - 1, -alpha);
                if scout > alpha && scout < beta {
                    -self.pvs(&next, depth - 1, 1, -beta, -alpha)
                } else {
                    scout
                }
            };

            self.pop_repetition(next.hash());

            if self.stopped {
                return best_move.map(|candidate| (candidate, best_score));
            }

            score = score.clamp(-MATE_SCORE, MATE_SCORE);
            if score > best_score {
                best_score = score;
                best_move = Some(mv);
            }
            alpha = alpha.max(score);
        }

        if let Some(best_move) = best_move {
            self.engine.tt.insert(
                board.hash(),
                TranspositionEntry {
                    depth,
                    score: best_score,
                    bound: Bound::Exact,
                    best_move: Some(best_move),
                },
            );
            Some((best_move, best_score))
        } else {
            None
        }
    }

    fn pvs(&mut self, board: &Board, depth: i32, ply: usize, mut alpha: i32, beta: i32) -> i32 {
        if self.should_stop() {
            return DRAW_SCORE;
        }

        if self.is_repetition(board.hash()) {
            return DRAW_SCORE;
        }

        match board.status() {
            GameStatus::Won => return -MATE_SCORE + ply as i32,
            GameStatus::Drawn => return DRAW_SCORE,
            GameStatus::Ongoing => {}
        }

        if depth <= 0 {
            return self.quiescence(board, ply, alpha, beta);
        }

        let original_alpha = alpha;
        if let Some(entry) = self.engine.tt.get(&board.hash()).copied() {
            if entry.depth >= depth {
                match entry.bound {
                    Bound::Exact => return entry.score,
                    Bound::Lower => alpha = alpha.max(entry.score),
                    Bound::Upper => {}
                }
                if matches!(entry.bound, Bound::Upper) && entry.score <= alpha {
                    return entry.score;
                }
                if alpha >= beta {
                    return entry.score;
                }
            }
        }

        let moves = legal_moves(board);
        if moves.is_empty() {
            return if board.checkers().is_empty() {
                DRAW_SCORE
            } else {
                -MATE_SCORE + ply as i32
            };
        }

        let tt_move = self
            .engine
            .tt
            .get(&board.hash())
            .and_then(|entry| entry.best_move);
        let ordered = self.order_moves(board, moves, tt_move, ply);
        let mut best_move = None;
        let mut best_score = i32::MIN / 4;

        for (index, mv) in ordered.into_iter().enumerate() {
            if self.should_stop() {
                return DRAW_SCORE;
            }

            let mut next = board.clone();
            next.play(mv);
            self.push_repetition(next.hash());

            let score = if index == 0 {
                -self.pvs(&next, depth - 1, ply + 1, -beta, -alpha)
            } else {
                let scout = -self.pvs(&next, depth - 1, ply + 1, -alpha - 1, -alpha);
                if scout > alpha && scout < beta {
                    -self.pvs(&next, depth - 1, ply + 1, -beta, -alpha)
                } else {
                    scout
                }
            };

            self.pop_repetition(next.hash());

            if self.stopped {
                return DRAW_SCORE;
            }

            if score > best_score {
                best_score = score;
                best_move = Some(mv);
            }
            if score > alpha {
                alpha = score;
            }
            if alpha >= beta {
                if is_quiet(board, mv) {
                    self.store_killer(ply, mv);
                    self.bump_history(board.side_to_move(), mv, depth);
                }
                break;
            }
        }

        let bound = if best_score <= original_alpha {
            Bound::Upper
        } else if best_score >= beta {
            Bound::Lower
        } else {
            Bound::Exact
        };
        self.engine.tt.insert(
            board.hash(),
            TranspositionEntry {
                depth,
                score: best_score,
                bound,
                best_move,
            },
        );
        best_score
    }

    fn quiescence(&mut self, board: &Board, ply: usize, mut alpha: i32, beta: i32) -> i32 {
        if self.should_stop() {
            return DRAW_SCORE;
        }

        if self.is_repetition(board.hash()) {
            return DRAW_SCORE;
        }

        let stand_pat = evaluate(board);
        if stand_pat >= beta {
            return beta;
        }
        alpha = alpha.max(stand_pat);

        let captures: Vec<_> = legal_moves(board)
            .into_iter()
            .filter(|mv| board.color_on(mv.to).is_some() || mv.promotion.is_some())
            .collect();
        let ordered = self.order_moves(board, captures, None, ply);

        for mv in ordered {
            if self.should_stop() {
                return alpha;
            }

            let mut next = board.clone();
            next.play(mv);
            self.push_repetition(next.hash());
            let score = -self.quiescence(&next, ply + 1, -beta, -alpha);
            self.pop_repetition(next.hash());

            if self.stopped {
                return DRAW_SCORE;
            }

            if score >= beta {
                return beta;
            }
            alpha = alpha.max(score);
        }

        alpha
    }

    fn order_moves(
        &self,
        board: &Board,
        mut moves: Vec<Move>,
        tt_move: Option<Move>,
        ply: usize,
    ) -> Vec<Move> {
        moves.sort_by_cached_key(|mv| -self.move_score(board, *mv, tt_move, ply));
        moves
    }

    fn move_score(&self, board: &Board, mv: Move, tt_move: Option<Move>, ply: usize) -> i32 {
        if tt_move == Some(mv) {
            return 2_000_000;
        }

        let attacker = board.piece_on(mv.from).unwrap_or(Piece::Pawn);
        let victim = board.piece_on(mv.to);
        let promotion_bonus = mv.promotion.map(piece_value).unwrap_or(0);

        if let Some(victim) = victim {
            return 1_000_000 + piece_value(victim) * 16 - piece_value(attacker) + promotion_bonus;
        }

        if self.engine.killer_moves[ply.min(MAX_PLY - 1)][0] == Some(mv) {
            return 900_000;
        }
        if self.engine.killer_moves[ply.min(MAX_PLY - 1)][1] == Some(mv) {
            return 850_000;
        }

        let side_index = color_index(board.side_to_move());
        let history = self.engine.history[side_index][square_index(mv.from)][square_index(mv.to)];
        history + promotion_bonus
    }

    fn store_killer(&mut self, ply: usize, mv: Move) {
        let killers = &mut self.engine.killer_moves[ply.min(MAX_PLY - 1)];
        if killers[0] != Some(mv) {
            killers[1] = killers[0];
            killers[0] = Some(mv);
        }
    }

    fn bump_history(&mut self, color: Color, mv: Move, depth: i32) {
        let entry = &mut self.engine.history[color_index(color)][square_index(mv.from)]
            [square_index(mv.to)];
        *entry += depth * depth;
        *entry = (*entry).min(50_000);
    }

    fn should_stop(&mut self) -> bool {
        self.node_count += 1;
        if self.node_count % TIME_CHECK_INTERVAL == 0 && Instant::now() >= self.deadline {
            self.stopped = true;
        }
        self.stopped
    }

    fn is_repetition(&self, hash: u64) -> bool {
        self.repetition.get(&hash).copied().unwrap_or_default() >= 3
    }

    fn push_repetition(&mut self, hash: u64) {
        *self.repetition.entry(hash).or_insert(0) += 1;
    }

    fn pop_repetition(&mut self, hash: u64) {
        if let Some(count) = self.repetition.get_mut(&hash) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.repetition.remove(&hash);
            }
        }
    }
}

fn evaluate(board: &Board) -> i32 {
    let phase = game_phase(board);
    let (white_mg, white_eg) = score_side(board, Color::White);
    let (black_mg, black_eg) = score_side(board, Color::Black);
    let mg_score = white_mg - black_mg;
    let eg_score = white_eg - black_eg;
    let blended = (mg_score * phase + eg_score * (PHASE_MAX - phase)) / PHASE_MAX;
    if board.side_to_move() == Color::White {
        blended
    } else {
        -blended
    }
}

fn score_side(board: &Board, color: Color) -> (i32, i32) {
    let mut mg = 0;
    let mut eg = 0;
    let occupied = board.colors(Color::White) | board.colors(Color::Black);
    let own = board.colors(color);

    for piece in Piece::ALL {
        for square in board.colored_pieces(color, piece) {
            let index = pst_index(square, color);
            let piece_idx = piece_index(piece);
            mg += MG_VALUE[piece_idx] + mg_pst(piece)[index];
            eg += EG_VALUE[piece_idx] + eg_pst(piece)[index];

            let mobility = mobility_for_piece(board, color, piece, square, occupied, own);
            mg += mobility.0;
            eg += mobility.1;
        }
    }

    let pawn_terms = pawn_structure(board, color);
    mg += pawn_terms.0;
    eg += pawn_terms.1;

    let king_terms = king_safety(board, color);
    mg += king_terms.0;
    eg += king_terms.1;

    if board.colored_pieces(color, Piece::Bishop).len() >= 2 {
        mg += 30;
        eg += 42;
    }

    (mg, eg)
}

fn pawn_structure(board: &Board, color: Color) -> (i32, i32) {
    let pawns = board.colored_pieces(color, Piece::Pawn);
    let enemy_pawns = board.colored_pieces(!color, Piece::Pawn);
    let mut file_counts = [0_u8; 8];
    let mut mg = 0;
    let mut eg = 0;

    for pawn in pawns {
        file_counts[pawn.file() as usize] += 1;
    }

    for pawn in pawns {
        let file = pawn.file() as usize;
        let rank = relative_rank(pawn, color);

        if file_counts[file] > 1 {
            mg -= 11;
            eg -= 16;
        }

        let has_left = file > 0 && file_counts[file - 1] > 0;
        let has_right = file < 7 && file_counts[file + 1] > 0;
        if !has_left && !has_right {
            mg -= 14;
            eg -= 12;
        }
        if has_left || has_right {
            mg += 6;
            eg += 8;
        }

        if is_passed_pawn(pawn, color, enemy_pawns) {
            mg += 12 + rank * 6;
            eg += 24 + rank * 10;
        }
    }

    (mg, eg)
}

fn king_safety(board: &Board, color: Color) -> (i32, i32) {
    let king = board.king(color);
    let zone = get_king_moves(king) | king.bitboard();
    let mut mg = 0;
    let mut eg = 0;

    let rank_step = if color == Color::White { 1_i32 } else { -1_i32 };
    let king_rank = king.rank() as i32;
    let king_file = king.file() as i32;
    let pawns = board.colored_pieces(color, Piece::Pawn);

    for file_delta in -1..=1 {
        let file = king_file + file_delta;
        if !(0..=7).contains(&file) {
            continue;
        }

        let front_rank = king_rank + rank_step;
        let second_rank = king_rank + rank_step * 2;
        let front_square = square_from_coords(file as usize, front_rank);
        let second_square = square_from_coords(file as usize, second_rank);

        let front_has_pawn = front_square.map(|sq| pawns.has(sq)).unwrap_or(false);
        let second_has_pawn = second_square.map(|sq| pawns.has(sq)).unwrap_or(false);

        if front_has_pawn {
            mg += 14;
            eg += 4;
        } else {
            mg -= 12;
        }

        if second_has_pawn {
            mg += 6;
        }
    }

    let pressure = enemy_attack_pressure(board, !color, zone);
    mg -= pressure;
    eg -= pressure / 4;

    (mg, eg)
}

fn enemy_attack_pressure(board: &Board, attacker: Color, zone: BitBoard) -> i32 {
    let occupied = board.colors(Color::White) | board.colors(Color::Black);
    let mut pressure = 0;

    for square in board.colored_pieces(attacker, Piece::Pawn) {
        if !(get_pawn_attacks(square, attacker) & zone).is_empty() {
            pressure += 6;
        }
    }
    for square in board.colored_pieces(attacker, Piece::Knight) {
        if !(get_knight_moves(square) & zone).is_empty() {
            pressure += 14;
        }
    }
    for square in board.colored_pieces(attacker, Piece::Bishop) {
        if !(get_bishop_moves(square, occupied) & zone).is_empty() {
            pressure += 16;
        }
    }
    for square in board.colored_pieces(attacker, Piece::Rook) {
        if !(get_rook_moves(square, occupied) & zone).is_empty() {
            pressure += 22;
        }
    }
    for square in board.colored_pieces(attacker, Piece::Queen) {
        let queen_attacks = get_bishop_moves(square, occupied) | get_rook_moves(square, occupied);
        if !(queen_attacks & zone).is_empty() {
            pressure += 30;
        }
    }

    pressure
}

fn mobility_for_piece(
    _board: &Board,
    color: Color,
    piece: Piece,
    square: Square,
    occupied: BitBoard,
    own: BitBoard,
) -> (i32, i32) {
    let attacks = match piece {
        Piece::Knight => get_knight_moves(square),
        Piece::Bishop => get_bishop_moves(square, occupied),
        Piece::Rook => get_rook_moves(square, occupied),
        Piece::Queen => get_bishop_moves(square, occupied) | get_rook_moves(square, occupied),
        Piece::King => get_king_moves(square),
        Piece::Pawn => get_pawn_attacks(square, color),
    } - own;

    let count = attacks.len() as i32;
    match piece {
        Piece::Knight => (count * 4, count * 4),
        Piece::Bishop => (count * 5, count * 5),
        Piece::Rook => (count * 2, count * 4),
        Piece::Queen => (count, count * 2),
        _ => (0, 0),
    }
}

fn game_phase(board: &Board) -> i32 {
    let mut phase = 0;
    for piece in Piece::ALL {
        let piece_idx = piece_index(piece);
        phase += PHASE_VALUE[piece_idx] * board.pieces(piece).len() as i32;
    }
    phase.clamp(0, PHASE_MAX)
}

fn is_passed_pawn(square: Square, color: Color, enemy_pawns: BitBoard) -> bool {
    for enemy in enemy_pawns {
        let file_distance = (enemy.file() as i32 - square.file() as i32).abs();
        if file_distance > 1 {
            continue;
        }
        let ahead = match color {
            Color::White => (enemy.rank() as i32) > square.rank() as i32,
            Color::Black => (enemy.rank() as i32) < square.rank() as i32,
        };
        if ahead {
            return false;
        }
    }
    true
}

fn relative_rank(square: Square, color: Color) -> i32 {
    match color {
        Color::White => square.rank() as i32,
        Color::Black => 7 - square.rank() as i32,
    }
}

fn pst_index(square: Square, color: Color) -> usize {
    let file = square.file() as usize;
    let rank = square.rank() as usize;
    let oriented_rank = match color {
        Color::White => rank,
        Color::Black => 7 - rank,
    };
    oriented_rank * 8 + file
}

fn square_from_coords(file: usize, rank: i32) -> Option<Square> {
    if !(0..=7).contains(&rank) {
        return None;
    }
    Some(Square::new(File::index(file), Rank::index(rank as usize)))
}

fn mg_pst(piece: Piece) -> &'static [i32; 64] {
    match piece {
        Piece::Pawn => &MG_PAWN_PST,
        Piece::Knight => &MG_KNIGHT_PST,
        Piece::Bishop => &MG_BISHOP_PST,
        Piece::Rook => &MG_ROOK_PST,
        Piece::Queen => &MG_QUEEN_PST,
        Piece::King => &MG_KING_PST,
    }
}

fn eg_pst(piece: Piece) -> &'static [i32; 64] {
    match piece {
        Piece::Pawn => &EG_PAWN_PST,
        Piece::Knight => &EG_KNIGHT_PST,
        Piece::Bishop => &EG_BISHOP_PST,
        Piece::Rook => &EG_ROOK_PST,
        Piece::Queen => &EG_QUEEN_PST,
        Piece::King => &EG_KING_PST,
    }
}

fn piece_index(piece: Piece) -> usize {
    match piece {
        Piece::Pawn => 0,
        Piece::Knight => 1,
        Piece::Bishop => 2,
        Piece::Rook => 3,
        Piece::Queen => 4,
        Piece::King => 5,
    }
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

fn square_index(square: Square) -> usize {
    square as usize
}

fn color_index(color: Color) -> usize {
    match color {
        Color::White => 0,
        Color::Black => 1,
    }
}

fn is_quiet(board: &Board, mv: Move) -> bool {
    board.color_on(mv.to).is_none() && mv.promotion.is_none()
}

fn is_forced_mate_score(score: i32) -> bool {
    score.abs() >= MATE_SCORE - MAX_PLY as i32
}

fn main() -> Result<()> {
    let _ = FILE_MASKS;
    run_uci_loop(&mut HandcraftedAlphaBetaEngine::new())
}
