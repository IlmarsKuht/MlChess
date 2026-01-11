# ML-chess Models Directory

This directory contains versioned neural network models for the chess engine.

## Structure

```
models/
├── README.md           # This file
├── v001/
│   ├── metadata.toml   # Model metadata and training history
│   ├── model.onnx      # ONNX model file (not tracked in git)
│   └── .gitkeep
├── v002/
│   └── ...
└── ...
```

## Versioning Scheme

- Each version gets its own directory: `v001`, `v002`, etc.
- `metadata.toml` contains all information about the model
- Model files (`.onnx`) are typically not tracked in git (add to `.gitignore`)
- Use Git LFS or external storage for large model files

## Creating a New Version

1. Create a new directory: `vNNN/`
2. Copy `metadata.toml` template from an existing version
3. Update the `parent_version` field to reference the previous version
4. Train the model using `training/train.py`
5. Export to ONNX and place in the version directory
6. Run evaluation matches to fill in metrics

## Model Lineage

Track parent-child relationships using `parent_version` in metadata:

```
v001 (first model)
  └── v002 (improved architecture)
        ├── v003 (more training data)
        └── v004 (hyperparameter tuning)
```

## Integration with Engine

Load a specific version in the UCI engine:
```
setoption name ModelVersion value v001
```

Or programmatically:
```rust
let engine = NeuralEngine::with_model("models/", "v001")?;
```
