//! Chess board widget rendering

use crate::game::GameState;
use crate::styles::{self, SQUARE_SIZE};
use chess_core::{Color as ChessColor, PieceKind};
use iced::widget::{button, column, container, row, svg, text, Svg};
use iced::{Color, Element, Length};

/// Message type for board interactions
#[derive(Debug, Clone)]
pub enum BoardMessage {
    SquareClicked(u8),
}

/// Renders the chess board
pub struct BoardView<'a> {
    game: &'a GameState,
    flipped: bool,
}

impl<'a> BoardView<'a> {
    pub fn new(game: &'a GameState, flipped: bool) -> Self {
        Self { game, flipped }
    }

    /// Create the board view element
    pub fn view(&self) -> Element<'a, BoardMessage> {
        let mut board_column = column![].spacing(0);

        for rank in 0..8 {
            let display_rank = if self.flipped { rank } else { 7 - rank };
            let mut rank_row = row![].spacing(0);

            for file in 0..8 {
                let display_file = if self.flipped { 7 - file } else { file };
                let sq = (display_rank * 8 + display_file) as u8;

                let square = self.render_square(sq, display_rank, display_file);
                rank_row = rank_row.push(square);
            }

            board_column = board_column.push(rank_row);
        }

        container(board_column)
            .style(|_theme| container::Style {
                border: iced::Border {
                    color: Color::from_rgb(0.3, 0.3, 0.3),
                    width: 2.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    /// Render a single square
    fn render_square(&self, sq: u8, rank: usize, file: usize) -> Element<'a, BoardMessage> {
        let is_light = (rank + file).is_multiple_of(2);
        let mut bg_color = if is_light {
            styles::LIGHT_SQUARE
        } else {
            styles::DARK_SQUARE
        };

        // Highlight selected square
        if self.game.selected_square == Some(sq) {
            bg_color = styles::SELECTED_SQUARE;
        }

        // Highlight last move
        if let Some((from, to)) = self.game.last_move {
            if sq == from || sq == to {
                bg_color = blend_colors(bg_color, styles::LAST_MOVE_SQUARE);
            }
        }

        // Get piece on this square
        let piece = self.game.position.piece_at(sq);

        // Legal move indicator
        let is_legal_target = self.game.legal_moves_from_selected.contains(&sq);

        let content: Element<'a, BoardMessage> = if let Some(p) = piece {
            // Use SVG piece
            let svg_handle = get_piece_svg(p.color, p.kind);
            container(
                svg_handle
                    .width(SQUARE_SIZE * 0.85)
                    .height(SQUARE_SIZE * 0.85),
            )
            .width(SQUARE_SIZE)
            .height(SQUARE_SIZE)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
        } else if is_legal_target {
            // Show dot for legal moves
            text("●")
                .size(SQUARE_SIZE * 0.3)
                .color(Color::from_rgba(0.0, 0.0, 0.0, 0.3))
                .center()
                .into()
        } else {
            text("").into()
        };

        button(
            container(content)
                .width(SQUARE_SIZE)
                .height(SQUARE_SIZE)
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        )
        .width(SQUARE_SIZE)
        .height(SQUARE_SIZE)
        .style(move |_theme, status| {
            let hover_overlay = match status {
                button::Status::Hovered => 0.1,
                button::Status::Pressed => 0.2,
                _ => 0.0,
            };
            button::Style {
                background: Some(iced::Background::Color(if hover_overlay > 0.0 {
                    blend_colors(bg_color, Color::from_rgba(1.0, 1.0, 1.0, hover_overlay))
                } else {
                    bg_color
                })),
                border: iced::Border::default(),
                text_color: Color::BLACK,
                ..Default::default()
            }
        })
        .on_press(BoardMessage::SquareClicked(sq))
        .into()
    }
}

/// Get the SVG handle for a piece
fn get_piece_svg(color: ChessColor, kind: PieceKind) -> Svg<'static> {
    let filename = match (color, kind) {
        (ChessColor::White, PieceKind::King) => "wk",
        (ChessColor::White, PieceKind::Queen) => "wq",
        (ChessColor::White, PieceKind::Rook) => "wr",
        (ChessColor::White, PieceKind::Bishop) => "wb",
        (ChessColor::White, PieceKind::Knight) => "wn",
        (ChessColor::White, PieceKind::Pawn) => "wp",
        (ChessColor::Black, PieceKind::King) => "bk",
        (ChessColor::Black, PieceKind::Queen) => "bq",
        (ChessColor::Black, PieceKind::Rook) => "br",
        (ChessColor::Black, PieceKind::Bishop) => "bb",
        (ChessColor::Black, PieceKind::Knight) => "bn",
        (ChessColor::Black, PieceKind::Pawn) => "bp",
    };

    // Load SVG from assets directory
    let path = format!("crates/gui/assets/pieces/{}.svg", filename);
    svg(svg::Handle::from_path(path))
}

/// Blend two colors together
fn blend_colors(base: Color, overlay: Color) -> Color {
    let alpha = overlay.a;
    Color::from_rgb(
        base.r * (1.0 - alpha) + overlay.r * alpha,
        base.g * (1.0 - alpha) + overlay.g * alpha,
        base.b * (1.0 - alpha) + overlay.b * alpha,
    )
}
