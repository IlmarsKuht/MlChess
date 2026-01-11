//! Neural Network Chess Engine
//!
//! This crate provides a neural network-based chess engine that uses
//! ONNX models for position evaluation and/or move prediction.
//!
//! # Architecture
//!
//! The engine supports two modes:
//! 1. **Eval-only**: Uses NN to evaluate positions, classical alpha-beta search
//! 2. **Policy+Value**: Uses NN for both move policy and position value (AlphaZero-style)
//!
//! # Model Loading
//!
//! Models are loaded from the `models/` directory with versioned subdirectories:
//! ```text
//! models/
//!   v001/
//!     model.onnx
//!     metadata.toml
//!   v002/
//!     model.onnx
//!     metadata.toml
//! ```

mod features;

#[cfg(feature = "onnx")]
mod onnx_engine;

use chess_core::{legal_moves_into, Engine, Move, Position, SearchResult};
use std::path::PathBuf;

/// Neural network chess engine.
///
/// When no model is loaded, falls back to random move selection.
/// This allows testing the infrastructure before training models.
pub struct NeuralEngine {
    /// Path to the loaded model (if any)
    #[allow(dead_code)]
    model_path: Option<PathBuf>,
    /// Model version string
    version: String,
    /// Node counter for statistics
    nodes: u64,
    /// Internal ONNX model (when feature enabled)
    #[cfg(feature = "onnx")]
    model: Option<onnx_engine::OnnxModel>,
}

impl Default for NeuralEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl NeuralEngine {
    /// Creates a new neural engine without a loaded model.
    /// Will use random move selection until a model is loaded.
    pub fn new() -> Self {
        Self {
            model_path: None,
            version: "random-v0".to_string(),
            nodes: 0,
            #[cfg(feature = "onnx")]
            model: None,
        }
    }

    /// Creates a neural engine with a specific model version.
    ///
    /// # Arguments
    /// * `models_dir` - Base directory containing model versions (e.g., "models/")
    /// * `version` - Version string (e.g., "v001")
    ///
    /// # Example
    /// ```ignore
    /// let engine = NeuralEngine::with_model("models/", "v001")?;
    /// ```
    pub fn with_model(models_dir: &str, version: &str) -> Result<Self, String> {
        let model_path = PathBuf::from(models_dir).join(version).join("model.onnx");

        if !model_path.exists() {
            return Err(format!("Model not found: {}", model_path.display()));
        }

        #[cfg(feature = "onnx")]
        {
            let model = onnx_engine::OnnxModel::load(&model_path)?;
            Ok(Self {
                model_path: Some(model_path),
                version: version.to_string(),
                nodes: 0,
                model: Some(model),
            })
        }

        #[cfg(not(feature = "onnx"))]
        {
            // Without ONNX feature, we just note the path but can't load
            Ok(Self {
                model_path: Some(model_path),
                version: version.to_string(),
                nodes: 0,
            })
        }
    }

    /// Returns the currently loaded model version.
    pub fn model_version(&self) -> &str {
        &self.version
    }

    /// Evaluate position using neural network (or fallback).
    fn evaluate(&self, pos: &Position) -> i32 {
        #[cfg(feature = "onnx")]
        if let Some(ref model) = self.model {
            let features = features::extract_features(pos);
            return model.evaluate(&features);
        }

        // Fallback: simple material count (same as classical)
        self.material_eval(pos)
    }

    /// Simple material evaluation as fallback.
    fn material_eval(&self, pos: &Position) -> i32 {
        use chess_core::{Color, PieceKind};
        let mut score = 0i32;
        for sq in 0..64u8 {
            if let Some(pc) = pos.piece_at(sq) {
                let v = match pc.kind {
                    PieceKind::Pawn => 100,
                    PieceKind::Knight => 320,
                    PieceKind::Bishop => 330,
                    PieceKind::Rook => 500,
                    PieceKind::Queen => 900,
                    PieceKind::King => 0,
                };
                score += if pc.color == Color::White { v } else { -v };
            }
        }
        if pos.side_to_move == Color::White {
            score
        } else {
            -score
        }
    }

    /// Simple search using NN evaluation.
    fn search_internal(&mut self, pos: &Position, depth: u8) -> Option<(Move, i32)> {
        let mut tmp = pos.clone();
        let mut moves = Vec::with_capacity(64);
        legal_moves_into(&mut tmp, &mut moves);

        if moves.is_empty() {
            return None;
        }

        if depth == 0 {
            // At depth 0, just pick best by static eval
            let mut best = moves[0];
            let mut best_score = i32::MIN;
            for mv in moves {
                let undo = tmp.make_move(mv);
                self.nodes += 1;
                let score = -self.evaluate(&tmp);
                tmp.unmake_move(mv, undo);
                if score > best_score {
                    best_score = score;
                    best = mv;
                }
            }
            return Some((best, best_score));
        }

        // Simple 1-ply search with NN eval
        let mut best = moves[0];
        let mut best_score = i32::MIN;

        for mv in moves {
            let undo = tmp.make_move(mv);
            self.nodes += 1;

            let score = if depth > 1 {
                // Recurse
                -self
                    .search_internal(&tmp, depth - 1)
                    .map(|(_, s)| s)
                    .unwrap_or(0)
            } else {
                -self.evaluate(&tmp)
            };

            tmp.unmake_move(mv, undo);

            if score > best_score {
                best_score = score;
                best = mv;
            }
        }

        Some((best, best_score))
    }
}

impl Engine for NeuralEngine {
    fn search(&mut self, pos: &Position, depth: u8) -> SearchResult {
        self.nodes = 0;
        let result = self.search_internal(pos, depth);

        SearchResult {
            best_move: result.map(|(mv, _)| mv),
            score: result.map(|(_, s)| s).unwrap_or(0),
            depth,
            nodes: self.nodes,
        }
    }

    fn name(&self) -> &str {
        // Include version in name for UCI identification
        Box::leak(format!("Neural-{}", self.version).into_boxed_str())
    }

    fn author(&self) -> &str {
        "ML-chess"
    }

    fn new_game(&mut self) {
        self.nodes = 0;
    }

    fn set_option(&mut self, name: &str, value: &str) -> bool {
        match name.to_lowercase().as_str() {
            "modelversion" | "model" => {
                // Try to load a different model version
                match NeuralEngine::with_model("models/", value) {
                    Ok(new_engine) => {
                        *self = new_engine;
                        true
                    }
                    Err(_) => false,
                }
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neural_engine_fallback() {
        let mut engine = NeuralEngine::new();
        let pos = Position::startpos();
        let result = engine.search(&pos, 2);
        assert!(result.best_move.is_some());
    }

    #[test]
    fn test_engine_trait_implementation() {
        let engine = NeuralEngine::new();
        assert!(engine.name().contains("Neural"));
        assert_eq!(engine.author(), "ML-chess");
    }
}
