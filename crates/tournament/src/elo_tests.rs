use super::*;

#[test]
fn test_elo_calculation() {
    let mut tracker = EloTracker::new();

    // Equal ratings should give 50% expected score
    let expected = tracker.expected_score("engine1", "engine2");
    assert!((expected - 0.5).abs() < 0.001);
}

#[test]
fn test_elo_update() {
    let mut tracker = EloTracker::new();

    // Engine1 wins all games
    let result = MatchResult {
        wins: 10,
        losses: 0,
        draws: 0,
    };
    tracker.update_ratings("engine1", "engine2", &result);

    assert!(tracker.get_rating("engine1") > DEFAULT_ELO);
    assert!(tracker.get_rating("engine2") < DEFAULT_ELO);
}
