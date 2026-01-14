use super::*;

#[test]
fn test_extract_features_startpos() {
    let pos = Position::startpos();
    let features = extract_features(&pos);

    assert_eq!(features.len(), NUM_FEATURES);

    // Count non-zero features (should be 32 pieces)
    let non_zero: usize = features.iter().filter(|&&x| x > 0.0).count();
    assert_eq!(non_zero, 32);
}

#[test]
fn test_features_relative_symmetry() {
    // Starting position should have symmetric features
    let pos = Position::startpos();
    let features = extract_features_relative(&pos);

    // White pawns should be on rank 2 (squares 8-15)
    for sq in 8..16 {
        let pawn_plane = 0; // Pawn = index 0, friendly = offset 0
        let idx = pawn_plane * 64 + sq;
        assert_eq!(features[idx], 1.0, "Expected white pawn at square {}", sq);
    }
}

#[test]
fn test_extract_features_extended() {
    let pos = Position::startpos();
    let features = extract_features_extended(&pos);

    // Should have 768 base features + 6 additional (castling, ep, halfmove)
    assert_eq!(features.len(), NUM_FEATURES + 6);

    // Verify castling rights are all 1.0 at start
    assert_eq!(features[NUM_FEATURES], 1.0); // wk
    assert_eq!(features[NUM_FEATURES + 1], 1.0); // wq
    assert_eq!(features[NUM_FEATURES + 2], 1.0); // bk
    assert_eq!(features[NUM_FEATURES + 3], 1.0); // bq

    // No en passant at start
    assert_eq!(features[NUM_FEATURES + 4], -1.0);

    // Halfmove clock is 0
    assert_eq!(features[NUM_FEATURES + 5], 0.0);
}
