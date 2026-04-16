//! Low-level engine process adapters and pure chess helper functions.
//!
//! Authoritative match runtime execution lives in `arena-server`. This crate
//! intentionally does not expose match-loop APIs.

pub mod adapter;
mod game_logic;
mod uci;

pub use adapter::{AgentAdapter, build_adapter};
pub use game_logic::{
    calculate_move_budget, classify_position, classify_terminal_board, fen_for_variant,
    insufficient_material, pgn_from_moves, starting_board,
};

#[cfg(test)]
mod tests {
    #[test]
    fn public_surface_does_not_reintroduce_high_level_match_runner() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(
            !manifest_dir.join("src").join("match_runner.rs").exists(),
            "arena-runner must not own a high-level match runner"
        );

        let lib_rs = std::fs::read_to_string(manifest_dir.join("src").join("lib.rs")).unwrap();
        let public_lib_rs = lib_rs
            .split("#[cfg(test)]")
            .next()
            .expect("lib.rs should have public source before tests");
        for forbidden in [
            "MatchRequest",
            "MatchBundleRequest",
            "MatchPairRequest",
            "play_single_game",
            "play_match_bundle",
            "play_match_pair",
        ] {
            assert!(
                !public_lib_rs.contains(forbidden),
                "arena-runner public API must not expose {forbidden}"
            );
        }
    }
}
