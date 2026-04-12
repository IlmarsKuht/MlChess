use std::collections::HashMap;

use arena_core::{GameResult, GameTermination, OpeningPosition, Variant};
use cozy_chess::{Board, Color, GameStatus, Piece};

pub fn starting_board(
    variant: Variant,
    opening: Option<&OpeningPosition>,
    seed: Option<u64>,
) -> Board {
    if let Some(opening) = opening {
        return Board::from_fen(&opening.fen, opening.variant.is_chess960())
            .expect("opening FEN should already be normalized");
    }

    match variant {
        Variant::Standard => Board::startpos(),
        Variant::Chess960 => Board::chess960_startpos(seed.unwrap_or(0) as u32 % 960),
    }
}

pub fn calculate_move_budget(remaining_ms: u64, increment_ms: u64) -> u64 {
    let base = (remaining_ms / 30).max(50);
    base.saturating_add(increment_ms).min(remaining_ms.max(50))
}

pub fn classify_position(
    board: &Board,
    repetitions: &HashMap<u64, u8>,
) -> Option<(GameResult, GameTermination)> {
    if board.halfmove_clock() >= 100 {
        return Some((GameResult::Draw, GameTermination::FiftyMoveRule));
    }

    if repetitions
        .get(&board.hash_without_ep())
        .copied()
        .unwrap_or(0)
        >= 3
    {
        return Some((GameResult::Draw, GameTermination::Repetition));
    }

    if insufficient_material(board) {
        return Some((GameResult::Draw, GameTermination::InsufficientMaterial));
    }

    None
}

pub fn classify_terminal_board(board: &Board) -> (GameResult, GameTermination) {
    match board.status() {
        GameStatus::Won => {
            if board.side_to_move() == Color::White {
                (GameResult::BlackWin, GameTermination::Checkmate)
            } else {
                (GameResult::WhiteWin, GameTermination::Checkmate)
            }
        }
        GameStatus::Drawn => (GameResult::Draw, GameTermination::Stalemate),
        GameStatus::Ongoing => (GameResult::Draw, GameTermination::Unknown),
    }
}

pub fn insufficient_material(board: &Board) -> bool {
    let major_minor = [Piece::Queen, Piece::Rook, Piece::Pawn];
    if major_minor
        .iter()
        .any(|piece| (board.pieces(*piece)).len() > 0)
    {
        return false;
    }

    let bishops = (board.pieces(Piece::Bishop)).len();
    let knights = (board.pieces(Piece::Knight)).len();
    bishops + knights <= 1
}

pub fn pgn_from_moves(
    event_name: &str,
    variant: Variant,
    start_fen: &str,
    moves: &[String],
    result: GameResult,
) -> String {
    let result_token = match result {
        GameResult::WhiteWin => "1-0",
        GameResult::BlackWin => "0-1",
        GameResult::Draw => "1/2-1/2",
    };
    let mut movetext = String::new();
    for (index, mv) in moves.iter().enumerate() {
        if index % 2 == 0 {
            let move_number = index / 2 + 1;
            movetext.push_str(&format!("{move_number}. "));
        }
        movetext.push_str(mv);
        movetext.push(' ');
    }
    movetext.push_str(result_token);

    format!(
        "[Event \"{}\"]\n[Site \"Rust Chess Arena\"]\n[Variant \"{}\"]\n[FEN \"{}\"]\n[Result \"{}\"]\n\n{}",
        event_name,
        match variant {
            Variant::Standard => "Standard",
            Variant::Chess960 => "Chess960",
        },
        start_fen,
        result_token,
        movetext.trim()
    )
}
