//! Feature extraction for neural network input
//!
//! Converts a chess position into a tensor suitable for NN input.
//! The default encoding uses a 8x8x12 representation (one plane per piece type).
//!
//! These functions are used when the `onnx` feature is enabled, and in tests.

#[cfg(any(feature = "onnx", test))]
use chess_core::{Color, Position};

/// Number of feature planes in the default encoding.
/// 12 planes: 6 piece types × 2 colors
#[cfg(any(feature = "onnx", test))]
pub const NUM_PLANES: usize = 12;

/// Total number of features: 8 × 8 × 12 = 768
#[cfg(any(feature = "onnx", test))]
pub const NUM_FEATURES: usize = 64 * NUM_PLANES;

/// Extracts features from a position for neural network input.
///
/// Returns a flat array of f32 values representing the board state.
/// The encoding is:
/// - Planes 0-5: White pieces (Pawn, Knight, Bishop, Rook, Queen, King)
/// - Planes 6-11: Black pieces (Pawn, Knight, Bishop, Rook, Queen, King)
///
/// Each plane is 64 squares (8×8), with 1.0 where the piece exists, 0.0 otherwise.
/// Board is always encoded from white's perspective (a1 = index 0).
#[cfg(any(feature = "onnx", test))]
pub fn extract_features(pos: &Position) -> Vec<f32> {
    let mut features = vec![0.0f32; NUM_FEATURES];

    for sq in 0..64u8 {
        if let Some(piece) = pos.piece_at(sq) {
            let piece_idx = piece.kind.idx();
            let color_offset = if piece.color == Color::White { 0 } else { 6 };
            let plane = piece_idx + color_offset;
            let idx = (plane * 64) + sq as usize;
            features[idx] = 1.0;
        }
    }

    // Optionally add side-to-move as additional feature
    // For now, we flip the board if black to move (common approach)

    features
}

/// Extracts features with the board flipped for black's perspective.
///
/// When it's black's turn, we flip the board so the NN always sees
/// the position from the perspective of the side to move.
#[cfg(any(feature = "onnx", test))]
pub fn extract_features_relative(pos: &Position) -> Vec<f32> {
    let mut features = vec![0.0f32; NUM_FEATURES];
    let flip = pos.side_to_move == Color::Black;

    for sq in 0..64u8 {
        if let Some(piece) = pos.piece_at(sq) {
            // Determine square index (flip if black to move)
            let target_sq = if flip { 63 - sq } else { sq };

            // Determine piece plane (swap colors if black to move)
            let piece_idx = piece.kind.idx();
            let is_friendly = piece.color == pos.side_to_move;
            let color_offset = if is_friendly { 0 } else { 6 };

            let plane = piece_idx + color_offset;
            let idx = (plane * 64) + target_sq as usize;
            features[idx] = 1.0;
        }
    }

    features
}

/// Extended feature extraction including additional game state.
///
/// Adds extra planes for:
/// - Castling rights (4 planes)
/// - En passant square (1 plane)
/// - Move counters (normalized)
#[cfg(any(feature = "onnx", test))]
pub fn extract_features_extended(pos: &Position) -> Vec<f32> {
    let mut features = extract_features_relative(pos);

    // Add castling rights as 4 additional values
    features.push(if pos.castling.wk { 1.0 } else { 0.0 });
    features.push(if pos.castling.wq { 1.0 } else { 0.0 });
    features.push(if pos.castling.bk { 1.0 } else { 0.0 });
    features.push(if pos.castling.bq { 1.0 } else { 0.0 });

    // Add en passant (as a single normalized square index, or -1)
    features.push(pos.en_passant.map(|ep| ep as f32 / 63.0).unwrap_or(-1.0));

    // Add halfmove clock (normalized to 0-1 range, capped at 100)
    features.push((pos.halfmove_clock as f32 / 100.0).min(1.0));

    features
}

#[cfg(test)]
mod tests {
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
}
