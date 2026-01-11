//! Styling constants and theme configuration

use iced::Color;

// Board colors
pub const LIGHT_SQUARE: Color = Color::from_rgb(0.94, 0.85, 0.71); // Wheat
pub const DARK_SQUARE: Color = Color::from_rgb(0.71, 0.53, 0.39); // Sienna
pub const SELECTED_SQUARE: Color = Color::from_rgb(0.68, 0.85, 0.37); // Yellow-green
pub const LAST_MOVE_SQUARE: Color = Color::from_rgba(0.9, 0.9, 0.0, 0.4); // Yellow overlay

// Dimensions
pub const SQUARE_SIZE: f32 = 70.0;
pub const PANEL_WIDTH: f32 = 300.0;

// Unicode chess pieces
pub fn piece_char(color: chess_core::Color, kind: chess_core::PieceKind) -> char {
    use chess_core::{Color, PieceKind};
    match (color, kind) {
        (Color::White, PieceKind::King) => '♔',
        (Color::White, PieceKind::Queen) => '♕',
        (Color::White, PieceKind::Rook) => '♖',
        (Color::White, PieceKind::Bishop) => '♗',
        (Color::White, PieceKind::Knight) => '♘',
        (Color::White, PieceKind::Pawn) => '♙',
        (Color::Black, PieceKind::King) => '♚',
        (Color::Black, PieceKind::Queen) => '♛',
        (Color::Black, PieceKind::Rook) => '♜',
        (Color::Black, PieceKind::Bishop) => '♝',
        (Color::Black, PieceKind::Knight) => '♞',
        (Color::Black, PieceKind::Pawn) => '♟',
    }
}
