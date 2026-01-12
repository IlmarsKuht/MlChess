use super::*;

#[test]
fn test_neural_engine_fallback() {
    let mut engine = NeuralEngine::new();
    let pos = Position::startpos();
    let result = engine.search(&pos, SearchLimits::depth(2));
    assert!(result.best_move.is_some());
}

#[test]
fn test_engine_trait_implementation() {
    let engine = NeuralEngine::new();
    assert!(engine.name().contains("Neural"));
    assert_eq!(engine.author(), "ML-chess");
}
