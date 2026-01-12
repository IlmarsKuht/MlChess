//! Match runner for playing games between engines

use chess_core::{legal_moves_into, Engine, Position, SearchLimits};
use std::time::Duration;

use crate::elo::{GameResult, MatchResult};

/// Configuration for a match
#[derive(Debug, Clone)]
pub struct MatchConfig {
    /// Number of games to play
    pub num_games: u32,
    /// Search depth for engines
    pub depth: u8,
    /// Maximum time per move (None = no limit)
    pub time_per_move: Option<Duration>,
    /// Maximum moves per game before declaring draw
    pub max_moves: u32,
    /// Whether to alternate colors each game
    pub alternate_colors: bool,
    /// Print progress during match
    pub verbose: bool,
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self {
            num_games: 10,
            depth: 4,
            time_per_move: None,
            max_moves: 200,
            alternate_colors: true,
            verbose: true,
        }
    }
}

impl MatchConfig {
    /// Create search limits based on this config
    fn search_limits(&self) -> SearchLimits {
        match self.time_per_move {
            Some(time) => SearchLimits::depth_and_time(self.depth, time),
            None => SearchLimits::depth(self.depth),
        }
    }
}

/// Runs matches between two engines
pub struct MatchRunner {
    config: MatchConfig,
}

impl MatchRunner {
    pub fn new(config: MatchConfig) -> Self {
        Self { config }
    }

    /// Run a match between two engines
    ///
    /// Returns the result from engine1's perspective
    pub fn run_match(
        &self,
        engine1: &mut dyn Engine,
        engine2: &mut dyn Engine,
    ) -> MatchResult {
        let mut result = MatchResult::new();

        for game_num in 0..self.config.num_games {
            // Alternate colors if configured
            let engine1_white = !self.config.alternate_colors || game_num % 2 == 0;

            let game_result = if engine1_white {
                self.play_game(engine1, engine2)
            } else {
                // Flip result since engine1 is black
                match self.play_game(engine2, engine1) {
                    GameResult::Win => GameResult::Loss,
                    GameResult::Loss => GameResult::Win,
                    GameResult::Draw => GameResult::Draw,
                }
            };

            match game_result {
                GameResult::Win => result.wins += 1,
                GameResult::Loss => result.losses += 1,
                GameResult::Draw => result.draws += 1,
            }

            if self.config.verbose {
                let color = if engine1_white { "W" } else { "B" };
                let outcome = match game_result {
                    GameResult::Win => "1-0",
                    GameResult::Loss => "0-1",
                    GameResult::Draw => "1/2",
                };
                println!(
                    "Game {}/{}: {} ({}) - Score: {}-{}-{}",
                    game_num + 1,
                    self.config.num_games,
                    outcome,
                    color,
                    result.wins,
                    result.losses,
                    result.draws
                );
            }
        }

        result
    }

    /// Play a single game, returns result from white's perspective
    fn play_game(
        &self,
        white: &mut dyn Engine,
        black: &mut dyn Engine,
    ) -> GameResult {
        let mut pos = Position::startpos();
        white.new_game();
        black.new_game();

        for _move_num in 0..self.config.max_moves {
            // Create fresh search limits for each move (resets the clock)
            let limits = self.config.search_limits();

            let result = if pos.side_to_move == chess_core::Color::White {
                white.search(&pos, limits)
            } else {
                black.search(&pos, limits)
            };

            match result.best_move {
                Some(mv) => {
                    pos.make_move(mv);
                }
                None => {
                    // No legal moves - checkmate or stalemate
                    let mut moves = Vec::new();
                    legal_moves_into(&mut pos, &mut moves);
                    if moves.is_empty() {
                        if pos.in_check(pos.side_to_move) {
                            // Checkmate - current side loses
                            return if pos.side_to_move == chess_core::Color::White {
                                GameResult::Loss // White is mated, white loses
                            } else {
                                GameResult::Win // Black is mated, white wins
                            };
                        } else {
                            return GameResult::Draw; // Stalemate
                        }
                    }
                }
            }

            // Check for draws
            if pos.halfmove_clock >= 100 {
                return GameResult::Draw; // 50-move rule
            }

            // Simple repetition check (would need proper implementation)
            // For now, rely on 50-move rule and max moves limit
        }

        // Max moves reached
        GameResult::Draw
    }
}

/// Quick utility to run a single match
pub fn quick_match(
    engine1: &mut dyn Engine,
    engine2: &mut dyn Engine,
    num_games: u32,
    depth: u8,
) -> MatchResult {
    let config = MatchConfig {
        num_games,
        depth,
        ..Default::default()
    };
    let runner = MatchRunner::new(config);
    runner.run_match(engine1, engine2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use classical_engine::ClassicalEngine;

    #[test]
    fn test_self_play() {
        let mut engine1 = ClassicalEngine::new();
        let mut engine2 = ClassicalEngine::new();

        let config = MatchConfig {
            num_games: 2,
            depth: 2,
            max_moves: 50,
            verbose: false,
            ..Default::default()
        };

        let runner = MatchRunner::new(config);
        let result = runner.run_match(&mut engine1, &mut engine2);

        // Self-play should complete without panic
        assert_eq!(result.total_games(), 2);
    }
}
