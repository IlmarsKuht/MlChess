//! Tournament Runner for ML-chess
//!
//! This crate provides infrastructure for:
//! - Running matches between different engines
//! - Tracking Elo ratings across versions
//! - Generating reports for model improvement validation
//!
//! # Usage
//!
//! ```bash
//! # Run a match between classical and neural engine
//! cargo run -p tournament -- --engine1 classical --engine2 neural:v001 --games 100
//!
//! # Run a gauntlet (one engine vs many)
//! cargo run -p tournament -- gauntlet --challenger neural:v002 --games 50
//! ```

mod elo;
mod match_runner;
mod results;

pub use elo::*;
pub use match_runner::*;
pub use results::*;
