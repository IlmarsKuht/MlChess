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

use chess_core::{
    legal_moves_into, Engine, Move, Position, SearchLimits, SearchResult, TimeControl,
};
use std::path::PathBuf;

/// Neural network chess engine.
///
/// When no model is loaded, falls back to random move selection.
/// This allows testing the infrastructure before training models.
pub struct NeuralEngine {
    /// Path to the loaded model (if any)
    model_path: Option<PathBuf>,
    /// Model version string
    version: String,
    /// Cached name string for UCI identification (avoids allocation on every call)
    name: String,
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
        let version = "random-v0".to_string();
        let name = format!("Neural-{}", version);
        Self {
            model_path: None,
            version,
            name,
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

        let name = format!("Neural-{}", version);

        #[cfg(feature = "onnx")]
        {
            let model = onnx_engine::OnnxModel::load(&model_path)?;
            Ok(Self {
                model_path: Some(model_path),
                version: version.to_string(),
                name,
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
                name,
                nodes: 0,
            })
        }
    }

    /// Returns the currently loaded model version.
    pub fn model_version(&self) -> &str {
        &self.version
    }

    /// Returns the path to the currently loaded model, if any.
    pub fn model_path(&self) -> Option<&std::path::Path> {
        self.model_path.as_deref()
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

    /// Simple material evaluation as fallback, using bitboard popcount.
    fn material_eval(&self, pos: &Position) -> i32 {
        use chess_core::{Color, PieceKind};

        // Material values indexed by PieceKind::idx()
        const PIECE_VALUES: [i32; 6] = [100, 320, 330, 500, 900, 0];

        let mut score = 0i32;
        for kind in PieceKind::ALL {
            let value = PIECE_VALUES[kind.idx()];
            let white_count = pos.bitboards.pieces(Color::White, kind).popcount() as i32;
            let black_count = pos.bitboards.pieces(Color::Black, kind).popcount() as i32;
            score += value * (white_count - black_count);
        }

        if pos.side_to_move == Color::White {
            score
        } else {
            -score
        }
    }

    /// Simple search using NN evaluation with time control.
    ///
    /// Returns (best_move, score, stopped) where stopped indicates early termination.
    fn search_internal(
        &mut self,
        pos: &Position,
        depth: u8,
        tc: &TimeControl,
    ) -> (Option<(Move, i32)>, bool) {
        let mut tmp = pos.clone();
        let mut moves = Vec::with_capacity(64);
        legal_moves_into(&mut tmp, &mut moves);

        if moves.is_empty() {
            return (None, false);
        }

        if depth == 0 {
            // At depth 0, just pick best by static eval
            let mut best = moves[0];
            let mut best_score = i32::MIN + 1; // +1 to avoid overflow on negation
            for mv in moves {
                // Check time periodically
                if tc.should_check_time(self.nodes) && tc.check_time() {
                    return (Some((best, best_score)), true);
                }

                let undo = tmp.make_move(mv);
                self.nodes += 1;
                let score = -self.evaluate(&tmp);
                tmp.unmake_move(mv, undo);
                if score > best_score {
                    best_score = score;
                    best = mv;
                }
            }
            return (Some((best, best_score)), false);
        }

        // Simple 1-ply search with NN eval
        let mut best = moves[0];
        let mut best_score = i32::MIN + 1; // +1 to avoid overflow on negation

        for mv in moves {
            // Check time before each root move
            if tc.should_check_time(self.nodes) && tc.check_time() {
                return (Some((best, best_score)), true);
            }

            let undo = tmp.make_move(mv);
            self.nodes += 1;

            let (score, stopped) = if depth > 1 {
                // Recurse
                let (result, stopped) = self.search_internal(&tmp, depth - 1, tc);
                (-result.map(|(_, s)| s).unwrap_or(0), stopped)
            } else {
                (-self.evaluate(&tmp), false)
            };

            tmp.unmake_move(mv, undo);

            if stopped {
                return (Some((best, best_score)), true);
            }

            if score > best_score {
                best_score = score;
                best = mv;
            }
        }

        (Some((best, best_score)), false)
    }
}

impl Engine for NeuralEngine {
    fn search(&mut self, pos: &Position, limits: SearchLimits) -> SearchResult {
        self.nodes = 0;
        limits.start();

        let (result, stopped) = self.search_internal(pos, limits.depth, &limits.time_control);

        SearchResult {
            best_move: result.map(|(mv, _)| mv),
            score: result.map(|(_, s)| s).unwrap_or(0),
            depth: limits.depth,
            nodes: self.nodes,
            stopped,
        }
    }

    fn name(&self) -> &str {
        &self.name
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
#[path = "lib_tests.rs"]
mod lib_tests;

