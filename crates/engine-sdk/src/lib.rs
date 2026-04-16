use std::io::{self, BufRead, Write};

use anyhow::{Result, anyhow, bail};
use arena_core::Variant;
use cozy_chess::{Board, Color, Move, Piece, util};

#[derive(Debug, Clone)]
pub struct SearchContext {
    pub movetime_ms: u64,
    pub variant: Variant,
    pub position_history_hashes: Vec<u64>,
}

pub trait UciEngine {
    fn name(&self) -> &'static str;
    fn author(&self) -> &'static str {
        "Rust Chess Arena"
    }

    fn new_game(&mut self, _variant: Variant) {}

    fn choose_move(
        &mut self,
        board: &Board,
        legal_moves: &[Move],
        ctx: SearchContext,
    ) -> Result<Move>;
}

pub fn legal_moves(board: &Board) -> Vec<Move> {
    let mut moves = Vec::new();
    board.generate_moves(|piece_moves| {
        moves.extend(piece_moves);
        false
    });
    moves
}

pub fn material_score(board: &Board, color: Color) -> i32 {
    let mut score = 0;
    for piece in [
        Piece::Pawn,
        Piece::Knight,
        Piece::Bishop,
        Piece::Rook,
        Piece::Queen,
    ] {
        let piece_score = match piece {
            Piece::Pawn => 100,
            Piece::Knight => 320,
            Piece::Bishop => 330,
            Piece::Rook => 500,
            Piece::Queen => 900,
            Piece::King => 0,
        };
        let ours = (board.colored_pieces(color, piece)).len() as i32;
        let theirs = (board.colored_pieces(!color, piece)).len() as i32;
        score += (ours - theirs) * piece_score;
    }
    score
}

pub fn run_uci_loop<E: UciEngine>(engine: &mut E) -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut board = Board::default();
    let mut variant = Variant::Standard;
    let mut position_history_hashes = vec![board.hash()];

    for line in stdin.lock().lines() {
        let line = line?;
        let command = line.trim();
        if command.is_empty() {
            continue;
        }

        if command == "uci" {
            writeln!(stdout, "id name {}", engine.name())?;
            writeln!(stdout, "id author {}", engine.author())?;
            writeln!(stdout, "option name UCI_Chess960 type check default false")?;
            writeln!(stdout, "uciok")?;
            stdout.flush()?;
            continue;
        }

        if command == "isready" {
            writeln!(stdout, "readyok")?;
            stdout.flush()?;
            continue;
        }

        if command == "ucinewgame" {
            board = match variant {
                Variant::Standard => Board::startpos(),
                Variant::Chess960 => Board::chess960_startpos(0),
            };
            position_history_hashes = vec![board.hash()];
            engine.new_game(variant);
            continue;
        }

        if let Some(rest) = command.strip_prefix("setoption name UCI_Chess960 value ") {
            variant = if rest.eq_ignore_ascii_case("true") {
                Variant::Chess960
            } else {
                Variant::Standard
            };
            continue;
        }

        if let Some(rest) = command.strip_prefix("position ") {
            let (next_board, next_history_hashes) = parse_position_command(rest, variant)?;
            board = next_board;
            position_history_hashes = next_history_hashes;
            continue;
        }

        if let Some(rest) = command.strip_prefix("go") {
            let movetime_ms = parse_movetime(rest).unwrap_or(250);
            let legal = legal_moves(&board);
            if legal.is_empty() {
                writeln!(stdout, "bestmove 0000")?;
                stdout.flush()?;
                continue;
            }

            let mv = engine.choose_move(
                &board,
                &legal,
                SearchContext {
                    movetime_ms,
                    variant,
                    position_history_hashes: position_history_hashes.clone(),
                },
            )?;
            if !board.is_legal(mv) {
                bail!("engine selected illegal move: {:?}", mv);
            }

            writeln!(stdout, "bestmove {}", format_uci_move(&board, mv, variant))?;
            stdout.flush()?;
            continue;
        }

        if command == "quit" {
            break;
        }
    }

    Ok(())
}

fn parse_movetime(command: &str) -> Option<u64> {
    let tokens: Vec<_> = command.split_whitespace().collect();
    tokens
        .windows(2)
        .find(|window| window[0] == "movetime")
        .and_then(|window| window[1].parse::<u64>().ok())
}

fn parse_position_command(command: &str, variant: Variant) -> Result<(Board, Vec<u64>)> {
    let parts: Vec<_> = command.split_whitespace().collect();
    if parts.is_empty() {
        bail!("missing position payload");
    }

    let mut board = if parts[0] == "startpos" {
        match variant {
            Variant::Standard => Board::startpos(),
            Variant::Chess960 => Board::chess960_startpos(0),
        }
    } else if parts[0] == "fen" {
        let moves_index = parts
            .iter()
            .position(|part| *part == "moves")
            .unwrap_or(parts.len());
        let fen = parts[1..moves_index].join(" ");
        Board::from_fen(&fen, variant.is_chess960())
            .map_err(|err| anyhow!("invalid position FEN: {err}"))?
    } else {
        bail!("unsupported position command: {command}");
    };
    let mut history_hashes = vec![board.hash()];

    if let Some(moves_index) = parts.iter().position(|part| *part == "moves") {
        for mv in &parts[moves_index + 1..] {
            let parsed = util::parse_uci_move(&board, mv)
                .map_err(|err| anyhow!("invalid UCI move: {err}"))?;
            board
                .try_play(parsed)
                .map_err(|err| anyhow!("illegal move in position command: {err}"))?;
            history_hashes.push(board.hash());
        }
    }

    Ok((board, history_hashes))
}

fn format_uci_move(board: &Board, mv: Move, variant: Variant) -> String {
    if variant.is_chess960() {
        mv.to_string()
    } else {
        util::display_uci_move(board, mv).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_castling_uses_standard_uci_king_destination() {
        let board: Board = "rnbqkb1r/ppp2ppp/4pn2/3p4/8/5NP1/PPPPPPBP/RNBQK2R w KQkq - 0 4"
            .parse()
            .unwrap();
        let castle: Move = "e1h1".parse().unwrap();

        assert_eq!(format_uci_move(&board, castle, Variant::Standard), "e1g1");
    }

    #[test]
    fn chess960_castling_uses_king_captures_rook_notation() {
        let (board, _) = parse_position_command(
            "fen qbbnnrkr/pppppppp/8/8/8/8/PPPPPPPP/QBBNNRKR w HFhf - 0 1 moves d1e3 e8f6 e1f3 d8e6 c2c3 f8e8 b1f5 d7d6 f1e1 g7g6 f5c2 c8d7 c2b3 e6c5 b3c4 d7c6 a1b1 c6e4 e3c2 d6d5 c4b5 c7c6 d2d4 c5e6 b5d3 e4d3 e2d3 b8d6 c1h6 a8d8 b1c1 d8a5 c1b1 a5a6 c2e3 e8d8 e1e2 d8e8",
            Variant::Chess960,
        )
        .unwrap();
        let castle: Move = "g1h1".parse().unwrap();

        assert!(board.is_legal(castle));
        assert_eq!(util::display_uci_move(&board, castle).to_string(), "g1g1");
        assert_eq!(format_uci_move(&board, castle, Variant::Chess960), "g1h1");
    }
}
