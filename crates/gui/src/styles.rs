//! Styling constants and theme configuration

use iced::Color;

// Board colors
pub const LIGHT_SQUARE: Color = Color::from_rgb(0.94, 0.85, 0.71); // Wheat
pub const DARK_SQUARE: Color = Color::from_rgb(0.71, 0.53, 0.39); // Sienna
pub const SELECTED_SQUARE: Color = Color::from_rgb(0.68, 0.85, 0.37); // Yellow-green
pub const LAST_MOVE_SQUARE: Color = Color::from_rgba(0.9, 0.9, 0.0, 0.4); // Yellow overlay

// Evaluation bar colors
pub const EVAL_WHITE: Color = Color::from_rgb(0.95, 0.95, 0.95);
pub const EVAL_BLACK: Color = Color::from_rgb(0.15, 0.15, 0.15);

// Dimensions
pub const SQUARE_SIZE: f32 = 70.0;
pub const PANEL_WIDTH: f32 = 320.0;
pub const EVAL_BAR_WIDTH: f32 = 30.0;
