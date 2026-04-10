use std::collections::HashMap;

use arena_core::{
    DEFAULT_TEST_RATING, LeaderboardEntry, MatchPair, PairEloConfig, PairRatingUpdate,
    apply_match_pair,
};
use uuid::Uuid;

pub(crate) fn default_entry(agent_version_id: Uuid) -> LeaderboardEntry {
    LeaderboardEntry {
        agent_version_id,
        rating: DEFAULT_TEST_RATING,
        games_played: 0,
        wins: 0,
        draws: 0,
        losses: 0,
    }
}

pub(crate) fn build_pair_rating_update(
    existing_entries: &HashMap<Uuid, LeaderboardEntry>,
    pair: &MatchPair,
) -> PairRatingUpdate {
    apply_match_pair(existing_entries, pair, PairEloConfig::default())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use arena_core::{GameRecord, GameResult, GameTermination, MatchPair, Variant};
    use chrono::Utc;
    use uuid::Uuid;

    use super::{build_pair_rating_update, default_entry};

    #[test]
    fn updates_both_entries_after_pair() {
        let engine_a = Uuid::new_v4();
        let engine_b = Uuid::new_v4();
        let pair = MatchPair {
            engine_a,
            engine_b,
            games: vec![
                GameRecord {
                    id: Uuid::new_v4(),
                    tournament_id: Uuid::new_v4(),
                    match_id: Uuid::new_v4(),
                    pool_id: Uuid::new_v4(),
                    variant: Variant::Standard,
                    opening_id: None,
                    white_version_id: engine_a,
                    black_version_id: engine_b,
                    result: GameResult::WhiteWin,
                    termination: GameTermination::Checkmate,
                    start_fen: "startpos".to_string(),
                    pgn: String::new(),
                    moves_uci: Vec::new(),
                    white_time_left_ms: 0,
                    black_time_left_ms: 0,
                    logs: Vec::new(),
                    started_at: Utc::now(),
                    completed_at: Utc::now(),
                },
                GameRecord {
                    id: Uuid::new_v4(),
                    tournament_id: Uuid::new_v4(),
                    match_id: Uuid::new_v4(),
                    pool_id: Uuid::new_v4(),
                    variant: Variant::Standard,
                    opening_id: None,
                    white_version_id: engine_b,
                    black_version_id: engine_a,
                    result: GameResult::Draw,
                    termination: GameTermination::Unknown,
                    start_fen: "startpos".to_string(),
                    pgn: String::new(),
                    moves_uci: Vec::new(),
                    white_time_left_ms: 0,
                    black_time_left_ms: 0,
                    logs: Vec::new(),
                    started_at: Utc::now(),
                    completed_at: Utc::now(),
                },
            ],
        };

        let entries = HashMap::from([
            (engine_a, default_entry(engine_a)),
            (engine_b, default_entry(engine_b)),
        ]);
        let update = build_pair_rating_update(&entries, &pair);

        assert_eq!(update.engine_a.games_played, 2);
        assert_eq!(update.engine_b.games_played, 2);
        assert_eq!(update.engine_a.wins, 1);
        assert_eq!(update.engine_b.losses, 1);
        assert!(update.engine_a.rating > update.engine_b.rating);
    }
}
