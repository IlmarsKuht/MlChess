//! Game state management

use chess_core::{legal_moves_into, Color, Move, PieceKind, Position};
use std::collections::HashSet;

/// Represents the current state of a chess game
#[derive(Debug, Clone)]
pub struct GameState {
    /// Current position
    pub position: Position,
    /// Move history
    pub moves: Vec<MoveRecord>,
    /// Currently selected square (for move input)
    pub selected_square: Option<u8>,
    /// Legal moves from selected square
    pub legal_moves_from_selected: HashSet<u8>,
    /// Last move (for highlighting)
    pub last_move: Option<(u8, u8)>,
    /// Game result
    pub result: GameResult,
    /// Is engine thinking?
    pub engine_thinking: bool,
}

/// A recorded move with SAN notation
#[derive(Debug, Clone)]
pub struct MoveRecord {
    /// Standard Algebraic Notation representation
    pub san: String,
}

/// Game result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameResult {
    InProgress,
    WhiteWins,
    BlackWins,
    Draw,
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

impl GameState {
    pub fn new() -> Self {
        Self {
            position: Position::startpos(),
            moves: Vec::new(),
            selected_square: None,
            legal_moves_from_selected: HashSet::new(),
            last_move: None,
            result: GameResult::InProgress,
            engine_thinking: false,
        }
    }

    /// Reset to starting position
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Select a square (for piece selection)
    pub fn select_square(&mut self, sq: u8) {
        // If clicking on a piece of the current side, select it
        if let Some(piece) = self.position.piece_at(sq) {
            if piece.color == self.position.side_to_move {
                self.selected_square = Some(sq);
                self.update_legal_moves();
                return;
            }
        }

        // If a piece is selected and clicking on a legal destination, make the move
        if let Some(from) = self.selected_square {
            if self.legal_moves_from_selected.contains(&sq) {
                self.make_move_from_to(from, sq);
            }
        }

        // Deselect
        self.selected_square = None;
        self.legal_moves_from_selected.clear();
    }

    /// Update legal moves from the selected square
    fn update_legal_moves(&mut self) {
        self.legal_moves_from_selected.clear();
        
        if let Some(from) = self.selected_square {
            let mut pos = self.position.clone();
            let mut moves = Vec::new();
            legal_moves_into(&mut pos, &mut moves);
            
            for mv in moves {
                if mv.from == from {
                    self.legal_moves_from_selected.insert(mv.to);
                }
            }
        }
    }

    /// Make a move from one square to another
    fn make_move_from_to(&mut self, from: u8, to: u8) {
        let mut pos = self.position.clone();
        let mut moves = Vec::new();
        legal_moves_into(&mut pos, &mut moves);

        // Find the matching move (handle promotions)
        let mv = moves.iter().find(|m| {
            m.from == from && m.to == to && m.promo.is_none()
        }).or_else(|| {
            // Default to queen promotion
            moves.iter().find(|m| {
                m.from == from && m.to == to && m.promo == Some(PieceKind::Queen)
            })
        });

        if let Some(&mv) = mv {
            self.apply_move(mv);
        }
    }

    /// Apply a move to the game state
    pub fn apply_move(&mut self, mv: Move) {
        let san = self.move_to_san(mv);
        let _undo = self.position.make_move(mv);
        
        self.moves.push(MoveRecord { san });
        
        self.last_move = Some((mv.from, mv.to));
        self.selected_square = None;
        self.legal_moves_from_selected.clear();
        
        // Check for game end
        self.update_result();
    }

    /// Convert move to SAN notation (simplified)
    fn move_to_san(&self, mv: Move) -> String {
        let piece = self.position.piece_at(mv.from);
        let is_capture = self.position.piece_at(mv.to).is_some() || mv.is_en_passant;
        
        let piece_char = match piece.map(|p| p.kind) {
            Some(PieceKind::King) => if mv.is_castle { "" } else { "K" },
            Some(PieceKind::Queen) => "Q",
            Some(PieceKind::Rook) => "R",
            Some(PieceKind::Bishop) => "B",
            Some(PieceKind::Knight) => "N",
            Some(PieceKind::Pawn) | None => "",
        };

        if mv.is_castle {
            return if mv.to % 8 > mv.from % 8 { "O-O".to_string() } else { "O-O-O".to_string() };
        }

        let from_file = (b'a' + mv.from % 8) as char;
        let to_file = (b'a' + mv.to % 8) as char;
        let to_rank = (b'1' + mv.to / 8) as char;

        let capture = if is_capture { "x" } else { "" };
        let file_prefix = if piece_char.is_empty() && is_capture {
            from_file.to_string()
        } else {
            String::new()
        };

        let promo = mv.promo.map(|k| {
            match k {
                PieceKind::Queen => "=Q",
                PieceKind::Rook => "=R",
                PieceKind::Bishop => "=B",
                PieceKind::Knight => "=N",
                _ => "",
            }
        }).unwrap_or("");

        format!("{}{}{}{}{}{}", piece_char, file_prefix, capture, to_file, to_rank, promo)
    }

    /// Update game result based on current position
    fn update_result(&mut self) {
        let mut pos = self.position.clone();
        let mut moves = Vec::new();
        legal_moves_into(&mut pos, &mut moves);

        if moves.is_empty() {
            if self.position.in_check(self.position.side_to_move) {
                // Checkmate
                self.result = match self.position.side_to_move {
                    Color::White => GameResult::BlackWins,
                    Color::Black => GameResult::WhiteWins,
                };
            } else {
                // Stalemate
                self.result = GameResult::Draw;
            }
        } else if self.position.halfmove_clock >= 100 {
            self.result = GameResult::Draw;
        }
    }

}
