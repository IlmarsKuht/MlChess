//! ML-chess GUI Application
//!
//! A graphical interface for:
//! - Playing chess against engines
//! - Running tournaments between engines
//! - Tracking Elo ratings
//! - Comparing model versions

mod app;
mod board;
mod game;
mod styles;
mod tournament_view;

use app::ChessApp;
use iced::application;

fn main() -> iced::Result {
    application("ML-chess", ChessApp::update, ChessApp::view)
        .subscription(ChessApp::subscription)
        .theme(ChessApp::theme)
        .window_size((1200.0, 800.0))
        .run_with(ChessApp::new)
}
