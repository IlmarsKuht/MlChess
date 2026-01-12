use super::*;

#[test]
fn test_startpos_moves() {
    let pos = Position::startpos();
    let moves = legal_moves(&pos);
    // Starting position has 20 legal moves
    assert_eq!(moves.len(), 20);
}

#[test]
fn test_kiwipete_moves() {
    // Kiwipete position - complex with many move types
    let pos =
        Position::from_fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -");
    let moves = legal_moves(&pos);
    assert_eq!(moves.len(), 48);
}
