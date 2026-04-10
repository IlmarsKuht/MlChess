mod game_logic;
mod match_runner;
mod uci;

pub use game_logic::{
    calculate_move_budget, classify_position, classify_terminal_board, insufficient_material,
    pgn_from_moves, starting_board,
};
pub use match_runner::{
    AgentAdapter, MatchBundleRequest, MatchPairRequest, MatchRequest, build_adapter,
    play_match_bundle, play_match_pair,
    play_single_game,
};
