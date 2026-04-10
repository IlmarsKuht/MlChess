use std::io::{self, BufRead, Write};

use anyhow::{Result, anyhow, bail};
use arena_core::Variant;
use cozy_chess::{Board, Color, Move, Piece, util};

#[derive(Debug, Clone, Copy)]
pub struct SearchContext {
    pub movetime_ms: u64,
    pub variant: Variant,
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
            board = parse_position_command(rest, variant)?;
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
                },
            )?;
            if !board.is_legal(mv) {
                bail!("engine selected illegal move: {:?}", mv);
            }

            writeln!(stdout, "bestmove {}", util::display_uci_move(&board, mv))?;
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

fn parse_position_command(command: &str, variant: Variant) -> Result<Board> {
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

    if let Some(moves_index) = parts.iter().position(|part| *part == "moves") {
        for mv in &parts[moves_index + 1..] {
            let parsed = util::parse_uci_move(&board, mv)
                .map_err(|err| anyhow!("invalid UCI move: {err}"))?;
            board
                .try_play(parsed)
                .map_err(|err| anyhow!("illegal move in position command: {err}"))?;
        }
    }

    Ok(board)
}
