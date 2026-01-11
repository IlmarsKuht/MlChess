<!-- Short, actionable guidance for AI coding agents working on this repo -->
# Copilot / AI agent instructions — ML-chess

Purpose
- Help contributors and AI agents be productive quickly when editing or extending this Rust ML chess engine workspace.
- This is a machine learning chess engine project with infrastructure for training, evaluating, and iterating on neural network models.

## ⚠️ IMPORTANT: Code Quality Rules

**Before running tests or considering work complete:**
1. Run `cargo build --workspace` and fix ALL warnings
2. Run `cargo clippy --workspace` if available
3. Only then run `cargo test --workspace`

Warnings are treated as blockers. Do not leave unused imports, dead code warnings, or other compiler warnings unfixed.

## Quick architecture overview

Workspace crates:
- `crates/chess_core` — Core game logic: board representation, move generation, perft, UCI helpers. NO engine logic here.
- `crates/classical_engine` — Classical alpha-beta search with material evaluation. Implements `Engine` trait.
- `crates/ml_engine` — Neural network engine with ONNX inference. Implements `Engine` trait.
- `crates/tournament` — Match runner for engine vs engine games with Elo tracking.
- `crates/uci_engine` — UCI protocol binary, supports switching between engines at runtime.

Key files:
- `chess_core/src/lib.rs` — Defines the `Engine` trait that all engines implement.
- `chess_core/src/board.rs`, `types.rs` — Position representation and core types.
- `chess_core/src/movegen.rs` — Legal move generation.
- `classical_engine/src/search.rs` — Negamax with alpha-beta pruning.
- `ml_engine/src/lib.rs` — NeuralEngine with ONNX model loading.
- `tournament/src/elo.rs` — Elo rating calculations.

External directories:
- `models/` — Versioned neural network models (v001/, v002/, etc.) with metadata.toml
- `training/` — Python scripts for training neural networks (PyTorch → ONNX)

## Developer workflows / useful commands

```bash
# Build and check for warnings (ALWAYS do this first)
cargo build --workspace

# Run all tests
cargo test --workspace

# Run perft tests only
cargo test -p chess_core --test perft_tests

# Run the UCI engine
cargo run -p uci_engine --release

# Run tournament match
cargo run -p tournament -- match classical neural --games 10 --depth 4

# Show Elo leaderboard
cargo run -p tournament -- leaderboard
```

## Engine trait pattern

All engines implement this trait from `chess_core`:
```rust
pub trait Engine: Send {
    fn search(&mut self, pos: &Position, depth: u8) -> SearchResult;
    fn name(&self) -> &str;
    fn author(&self) -> &str { "ML-chess" }
    fn new_game(&mut self) {}
    fn set_option(&mut self, name: &str, value: &str) -> bool { false }
}
```

## Model versioning

Models are stored in `models/vNNN/` directories:
- `model.onnx` — The trained ONNX model
- `metadata.toml` — Training params, parent version, metrics, match results

Training workflow:
1. Train in Python: `cd training && python train.py --output ../models/v002/`
2. Test in Rust: `cargo run -p tournament -- match classical neural:v002`
3. Track Elo in `tournament_elo.json`

## Project-specific patterns & conventions

- The `Engine` trait is the abstraction for swappable engines.
- Perft is the canonical correctness check for move generation.
- `Position::from_fen(...)` creates positions; `legal_moves_into()` generates moves.
- Mutability convention: many core functions accept `&mut Position`.
- Integration tests live in `crates/*/tests/` directories.

## What to look for when triaging bugs

- Perft mismatch → check `movegen.rs`
- Search/eval issues → check `classical_engine/src/search.rs` or `ml_engine/src/lib.rs`
- UCI protocol issues → check `uci_engine/src/main.rs`
- Model loading issues → check `ml_engine/src/onnx_engine.rs`

## Search tips (code patterns to grep)

- `impl Engine for` — Engine implementations
- `Position::from_fen` — Position creation
- `SearchResult` — Search return type
- `perft(` — Perft tests
