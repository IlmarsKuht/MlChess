//! Game state management with clock support

use chess_core::{legal_moves_into, Color, Move, PieceKind, Position};
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// Represents the current state of a chess game
#[derive(Debug, Clone)]
pub struct GameState {
    /// Current position
    pub position: Position,
    /// Move history
    pub moves: Vec<MoveRecord>,
    /// Position hash history for threefold repetition detection
    pub position_history: Vec<u64>,
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
    /// Clock state
    pub clock: ChessClock,
    /// Current evaluation (in centipawns, positive = white advantage)
    pub evaluation: i32,
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
    WhiteTimeout,
    BlackTimeout,
}

/// Time control settings
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeControl {
    /// Initial time in seconds
    pub initial_time: u64,
    /// Increment per move in seconds
    pub increment: u64,
}

impl TimeControl {
    pub fn new(minutes: u64, increment_secs: u64) -> Self {
        Self {
            initial_time: minutes * 60,
            increment: increment_secs,
        }
    }

    /// Unlimited time
    pub fn unlimited() -> Self {
        Self {
            initial_time: 0,
            increment: 0,
        }
    }

    pub fn is_unlimited(&self) -> bool {
        self.initial_time == 0
    }
}

impl Default for TimeControl {
    fn default() -> Self {
        Self::new(10, 5) // Rapid 10+5
    }
}

impl std::fmt::Display for TimeControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_unlimited() {
            write!(f, "Unlimited")
        } else {
            write!(f, "{}+{}", self.initial_time / 60, self.increment)
        }
    }
}

/// Chess clock for both players
#[derive(Debug, Clone)]
pub struct ChessClock {
    /// Time control settings
    pub time_control: TimeControl,
    /// White's remaining time in milliseconds
    pub white_time_ms: u64,
    /// Black's remaining time in milliseconds
    pub black_time_ms: u64,
    /// When the current player's clock started (if running)
    pub started_at: Option<Instant>,
    /// Which side's clock is running
    pub running_for: Option<Color>,
    /// Is the clock enabled?
    pub enabled: bool,
}

impl Default for ChessClock {
    fn default() -> Self {
        Self::new(TimeControl::default())
    }
}

impl ChessClock {
    pub fn new(time_control: TimeControl) -> Self {
        let initial_ms = time_control.initial_time * 1000;
        Self {
            time_control,
            white_time_ms: initial_ms,
            black_time_ms: initial_ms,
            started_at: None,
            running_for: None,
            enabled: !time_control.is_unlimited(),
        }
    }

    /// Start the clock for a player
    pub fn start(&mut self, color: Color) {
        if self.enabled {
            self.started_at = Some(Instant::now());
            self.running_for = Some(color);
        }
    }

    /// Stop the clock (after a move) and add increment
    pub fn stop_and_increment(&mut self) {
        if !self.enabled {
            return;
        }

        if let (Some(started), Some(color)) = (self.started_at, self.running_for) {
            let elapsed_ms = started.elapsed().as_millis() as u64;
            let increment_ms = self.time_control.increment * 1000;

            match color {
                Color::White => {
                    self.white_time_ms =
                        self.white_time_ms.saturating_sub(elapsed_ms) + increment_ms;
                }
                Color::Black => {
                    self.black_time_ms =
                        self.black_time_ms.saturating_sub(elapsed_ms) + increment_ms;
                }
            }
        }

        self.started_at = None;
        self.running_for = None;
    }

    /// Get current remaining time for a player (accounting for running clock)
    pub fn remaining_time(&self, color: Color) -> Duration {
        let base_ms = match color {
            Color::White => self.white_time_ms,
            Color::Black => self.black_time_ms,
        };

        let elapsed_ms = if self.running_for == Some(color) {
            self.started_at
                .map(|s| s.elapsed().as_millis() as u64)
                .unwrap_or(0)
        } else {
            0
        };

        Duration::from_millis(base_ms.saturating_sub(elapsed_ms))
    }

    /// Check if a player has timed out
    pub fn is_timeout(&self, color: Color) -> bool {
        self.enabled && self.remaining_time(color).is_zero()
    }

    /// Format time as MM:SS
    pub fn format_time(duration: Duration) -> String {
        let total_secs = duration.as_secs();
        let mins = total_secs / 60;
        let secs = total_secs % 60;

        if duration.as_millis() < 10_000 {
            // Show tenths when under 10 seconds
            let tenths = (duration.as_millis() % 1000) / 100;
            format!("{}:{:02}.{}", mins, secs, tenths)
        } else {
            format!("{}:{:02}", mins, secs)
        }
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

impl GameState {
    pub fn new() -> Self {
        let position = Position::startpos();
        let initial_hash = position.position_hash();
        Self {
            position,
            moves: Vec::new(),
            position_history: vec![initial_hash],
            selected_square: None,
            legal_moves_from_selected: HashSet::new(),
            last_move: None,
            result: GameResult::InProgress,
            engine_thinking: false,
            clock: ChessClock::default(),
            evaluation: 0,
        }
    }

    /// Create with specific time control
    pub fn with_time_control(time_control: TimeControl) -> Self {
        Self {
            clock: ChessClock::new(time_control),
            ..Self::new()
        }
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

        self.selected_square = None;
        self.legal_moves_from_selected.clear();
    }

    /// Update legal moves from selected square
    fn update_legal_moves(&mut self) {
        self.legal_moves_from_selected.clear();

        if let Some(from) = self.selected_square {
            let mut pos = self.position.clone();
            let mut moves = Vec::new();
            legal_moves_into(&mut pos, &mut moves);

            for mv in moves {
                if mv.from() == from {
                    self.legal_moves_from_selected.insert(mv.to());
                }
            }
        }
    }

    /// Make a move from one square to another
    fn make_move_from_to(&mut self, from: u8, to: u8) {
        let mut pos = self.position.clone();
        let mut moves = Vec::new();
        legal_moves_into(&mut pos, &mut moves);

        // Find the matching move (handle promotions - default to queen)
        let mv = moves.iter().find(|m| {
            m.from() == from && m.to() == to && (m.promo().is_none() || m.promo() == Some(PieceKind::Queen))
        });

        if let Some(&mv) = mv {
            self.apply_move(mv);
        }
    }

    /// Apply a move to the game state
    pub fn apply_move(&mut self, mv: Move) {
        // Stop clock for current player and add increment
        self.clock.stop_and_increment();

        // Generate SAN before making the move
        let san = self.generate_san(mv);

        self.position.make_move(mv);
        self.moves.push(MoveRecord { san });
        self.last_move = Some((mv.from(), mv.to()));
        self.selected_square = None;
        self.legal_moves_from_selected.clear();

        // Add position hash to history for repetition detection
        self.position_history.push(self.position.position_hash());

        // Start clock for next player
        self.clock.start(self.position.side_to_move);

        // Check for game end
        self.check_game_end();
    }

    /// Generate SAN notation for a move
    fn generate_san(&self, mv: Move) -> String {
        let piece = self.position.piece_at(mv.from());
        if piece.is_none() {
            return format!("{}{}", sq_name(mv.from()), sq_name(mv.to()));
        }
        let piece = piece.unwrap();

        // Castling
        if mv.is_castle() {
            if mv.to() > mv.from() {
                return "O-O".to_string();
            } else {
                return "O-O-O".to_string();
            }
        }

        let mut san = String::new();

        // Piece letter (except for pawns)
        match piece.kind {
            PieceKind::King => san.push('K'),
            PieceKind::Queen => san.push('Q'),
            PieceKind::Rook => san.push('R'),
            PieceKind::Bishop => san.push('B'),
            PieceKind::Knight => san.push('N'),
            PieceKind::Pawn => {}
        }

        // Capture indicator
        let is_capture = self.position.piece_at(mv.to()).is_some() || mv.is_en_passant();
        if is_capture {
            if piece.kind == PieceKind::Pawn {
                san.push((b'a' + (mv.from() % 8)) as char);
            }
            san.push('x');
        }

        // Destination square
        san.push_str(&sq_name(mv.to()));

        // Promotion
        if let Some(promo) = mv.promo() {
            san.push('=');
            san.push(match promo {
                PieceKind::Queen => 'Q',
                PieceKind::Rook => 'R',
                PieceKind::Bishop => 'B',
                PieceKind::Knight => 'N',
                _ => '?',
            });
        }

        san
    }

    /// Check if the current position has occurred at least 3 times (threefold repetition)
    fn is_threefold_repetition(&self) -> bool {
        let current_hash = self.position.position_hash();
        let count = self
            .position_history
            .iter()
            .filter(|&&h| h == current_hash)
            .count();
        count >= 3
    }

    /// Check if the game has ended
    fn check_game_end(&mut self) {
        // Check for timeout
        if self.clock.is_timeout(Color::White) {
            self.result = GameResult::WhiteTimeout;
            return;
        }
        if self.clock.is_timeout(Color::Black) {
            self.result = GameResult::BlackTimeout;
            return;
        }

        // Check for checkmate/stalemate
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
            return;
        }

        // Check for fifty-move rule
        if self.position.is_fifty_move_draw() {
            self.result = GameResult::Draw;
            return;
        }

        // Check for threefold repetition
        if self.is_threefold_repetition() {
            self.result = GameResult::Draw;
            return;
        }

        // Check for insufficient material
        if self.position.is_insufficient_material() {
            self.result = GameResult::Draw;
        }
    }

    /// Set the evaluation (from engine analysis)
    pub fn set_evaluation(&mut self, eval_centipawns: i32) {
        self.evaluation = eval_centipawns;
    }
}

/// Convert square index to algebraic notation
fn sq_name(sq: u8) -> String {
    let file = (b'a' + sq % 8) as char;
    let rank = (b'1' + sq / 8) as char;
    format!("{}{}", file, rank)
}
