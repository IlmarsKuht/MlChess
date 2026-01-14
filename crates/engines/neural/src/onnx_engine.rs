//! ONNX model loading and inference
//!
//! This module handles loading ONNX models and running inference.
//! Requires the `onnx` feature to be enabled.

use std::path::Path;
use tract_onnx::prelude::*;

/// Wrapper around an ONNX model for chess position evaluation.
pub struct OnnxModel {
    model: SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>,
    input_size: usize,
}

impl OnnxModel {
    /// Load an ONNX model from the given path.
    pub fn load(path: &Path) -> Result<Self, String> {
        let model = tract_onnx::onnx()
            .model_for_path(path)
            .map_err(|e| format!("Failed to load ONNX model: {}", e))?
            .into_optimized()
            .map_err(|e| format!("Failed to optimize model: {}", e))?
            .into_runnable()
            .map_err(|e| format!("Failed to make model runnable: {}", e))?;

        // Determine input size from model
        let input_fact = model
            .model()
            .input_fact(0)
            .map_err(|e| format!("Failed to get input fact: {}", e))?;

        let input_size = input_fact
            .shape
            .iter()
            .filter_map(|d| d.to_i64().ok())
            .product::<i64>() as usize;

        Ok(Self { model, input_size })
    }

    /// Evaluate a position given its feature vector.
    ///
    /// Returns the evaluation in centipawns.
    pub fn evaluate(&self, features: &[f32]) -> i32 {
        // Ensure features match expected input size
        if features.len() != self.input_size {
            // Pad or truncate if necessary
            let mut input = vec![0.0f32; self.input_size];
            let copy_len = features.len().min(self.input_size);
            input[..copy_len].copy_from_slice(&features[..copy_len]);
            return self.run_inference(&input);
        }

        self.run_inference(features)
    }

    fn run_inference(&self, features: &[f32]) -> i32 {
        // Create input tensor
        let input: Tensor = tract_ndarray::Array::from_shape_vec(
            (1, self.input_size),
            features.to_vec(),
        )
        .expect("Failed to create input array")
        .into();

        // Run inference
        match self.model.run(tvec!(input.into())) {
            Ok(result) => {
                // Extract scalar output
                if let Ok(output) = result[0].to_array_view::<f32>() {
                    // Convert to centipawns (assuming output is in range [-1, 1])
                    let raw_value = output.iter().next().copied().unwrap_or(0.0);
                    (raw_value * 1000.0) as i32
                } else {
                    0
                }
            }
            Err(_) => 0,
        }
    }

    /// Get the expected input size for this model.
    pub fn input_size(&self) -> usize {
        self.input_size
    }
}
