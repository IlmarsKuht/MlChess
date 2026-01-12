<!-- Short, actionable guidance for AI coding agents working on this repo -->
# Copilot / AI agent instructions — ML-chess

Purpose
- Help contributors and AI agents be productive quickly when editing or extending this Rust ML chess engine workspace.
- This is a machine learning chess engine project with infrastructure for training, evaluating, and iterating on neural network models.

## ⚠️ CRITICAL: Code Quality Standards

**This project demands NASA-level code quality. All changes must be complete, well-tested, and production-ready.**

### Absolute Rules

1. **NO incomplete refactors.** Every refactor must be fully complete. Do not leave TODO comments, half-implemented features, or "we'll fix this later" code. If you start a change, finish it entirely.

2. **NO dead code.** Do not add `#[allow(dead_code)]` unless there is an extremely compelling reason (e.g., conditional compilation for platform-specific code, or trait implementations required by external interfaces). If code isn't used, remove it. Do not add "just in case" code.

3. **NO warnings.** All compiler warnings are treated as errors. Fix every warning before considering work complete.

4. **Complete implementations only.** When adding new features, implement them fully. Do not create stub functions, placeholder implementations, or scaffolding that doesn't work.

5. **FIX ALL WARNINGS AND ERRORS — NO EXCEPTIONS.** When you run `cargo build`, `cargo clippy`, or `cargo test`, you MUST fix EVERY warning and error you see — regardless of whether you caused it or it was pre-existing. Do NOT say "this warning is unrelated to my changes" or "this was already there". If you see it, you fix it. Period. This applies to:
   - Clippy lints (large_enum_variant, unused variables, etc.)
   - Compiler warnings (dead code, unused imports, etc.)
   - Test failures
   - Any other diagnostic output
   
   **The codebase must be warning-free at all times.**

6. **NO comments in code unless absolutely essential.** Rely on clear, self-documenting code; only keep comments that convey critical rationale that cannot be expressed in code.

### Before Completing Any Work

1. Run `cargo build --workspace` — fix ALL warnings
2. Run `cargo clippy --workspace` — fix ALL lints  
3. Run `cargo test --workspace` — ensure ALL tests pass
4. Review your changes — would a senior engineer approve this for a mission-critical system?

### Quality Checklist

- [ ] All code paths are implemented (no `todo!()`, `unimplemented!()`, or panic placeholders)
- [ ] All imports are used
- [ ] All functions/structs/fields are used
- [ ] All match arms handle their cases completely
- [ ] Error handling is explicit and complete
- [ ] No `#[allow(...)]` attributes unless absolutely necessary with documented justification

## Quick architecture overview

Workspace crates:
- `crates/chess_core` — Core rules, bitboard representation, move generation, perft harness, zobrist hashing, UCI helpers, `SearchLimits`/`TimeControl`, and the shared `Engine` trait. No engine-specific logic lives here.
- `crates/classical_engine` — Deterministic negamax with alpha-beta pruning and lightweight material evaluation. Obeys `TimeControl` for early stop and serves as the baseline engine implementation.
- `crates/ml_engine` — NeuralEngine with optional `onnx` feature for loading ONNX models; falls back to material/random play when no model is loaded. Encodes positions via `features` and respects `SearchLimits`.
- `crates/uci_engine` — Binary that speaks the UCI protocol and can switch between Classical and Neural engines at runtime; handles depth/movetime options and iterative deepening.
- `crates/tournament` — CLI for running matches/gauntlets between engines, aggregating results, and tracking Elo.
- `crates/gui` — Iced desktop UI for playing games, running local tournaments, and viewing Elo history.

Key files:
- `chess_core/src/lib.rs` — Re-exports core types and defines `Engine` and `SearchLimits`.
- `chess_core/src/board.rs`, `types.rs` — Position representation and core enums.
- `chess_core/src/movegen.rs` — Legal move generation and perft helpers.
- `chess_core/src/time_control.rs` — Time control and search limit helpers.
- `classical_engine/src/search.rs` — Negamax with alpha-beta pruning and draw detection.
- `ml_engine/src/lib.rs` — NeuralEngine setup, model loading (when `onnx` is enabled), and search glue.
- `tournament/src/match_runner.rs`, `tournament/src/elo.rs` — Match orchestration and Elo calculations.
- `uci_engine/src/main.rs` — UCI command loop and engine selection.
- `gui/src/app.rs` — Iced app state, routing, and board/tournament views.

External directories:
- `models/` — Versioned neural network models (v001/, v002/, etc.) with `metadata.toml` per version.
- `training/` — Python training/export pipeline (PyTorch → ONNX) and dataset utilities.
- `scripts/` — Utility scripts (e.g., profiling helpers).

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
    fn search(&mut self, pos: &Position, limits: SearchLimits) -> SearchResult;
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
