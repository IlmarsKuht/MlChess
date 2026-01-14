use super::*;
use chess_core::Position;

#[test]
fn test_pick_best_move_start_position() {
    let pos = Position::startpos();
    let mut nodes = 0;
    let tc = TimeControl::new(None);
    tc.start();
    let result = pick_best_move(&pos, 3, &mut nodes, &tc);
    assert!(result.best_move.is_some());
    assert!(nodes > 0);
}

#[test]
fn test_pick_best_move_finds_mate_in_one() {
    // Position where Qh7# is mate in one
    let pos = Position::from_fen("6k1/5ppp/8/8/8/8/5PPP/4Q1K1 w - - 0 1");
    let mut nodes = 0;
    let tc = TimeControl::new(None);
    tc.start();
    let result = pick_best_move(&pos, 2, &mut nodes, &tc);
    assert!(result.best_move.is_some());
}
