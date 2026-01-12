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
