use super::*;

#[test]
fn random_engine_returns_legal_move() {
    let mut engine = RandomEngine::new();
    let pos = Position::startpos();
    let limits = SearchLimits::depth(1);

    let result = engine.search(&pos, limits);

    assert!(result.best_move.is_some());

    let mut pos_copy = pos.clone();
    let mut legal_moves = Vec::new();
    legal_moves_into(&mut pos_copy, &mut legal_moves);
    assert!(legal_moves.contains(&result.best_move.unwrap()));
}

#[test]
fn random_engine_handles_checkmate() {
    let mut engine = RandomEngine::new();
    let pos =
        Position::from_fen("r1bqkbnr/pppp1Qpp/2n5/4p3/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 0 1");
    let limits = SearchLimits::depth(1);

    let result = engine.search(&pos, limits);

    assert!(result.best_move.is_none());
}

#[test]
fn random_engine_handles_stalemate() {
    let mut engine = RandomEngine::new();
    let pos = Position::from_fen("k7/8/1Q6/8/8/8/8/1K6 b - - 0 1");
    let limits = SearchLimits::depth(1);

    let result = engine.search(&pos, limits);

    assert!(result.best_move.is_none());
}
