<!-- Short, actionable guidance for AI coding agents working on this repo -->
# Copilot / AI agent instructions — ML-chess

Purpose
- Help contributors and AI agents be productive quickly when editing or extending this Rust chess engine workspace.

Quick architecture overview
- Workspace with two crates: `crates/chess_core` (engine core) and `crates/uci_engine` (UCI binary).
- `crates/chess_core/src/` responsibilities:
  - `board.rs` and `types.rs`: canonical position representation and related types.
  - `movegen.rs`: move generation and rules enforcement (core of correctness).
  - `perft.rs`: perft implementation used by tests to validate move generation.
  - `search.rs` / `eval.rs`: search algorithm and position evaluation.
  - `uci.rs`: helpers for UCI integration used by the engine binary.
- `crates/uci_engine/src/main.rs` wires the UCI protocol to the engine APIs in `chess_core`.

Developer workflows / useful commands
- Build whole workspace: `cargo build --workspace`.
- Run all tests: `cargo test --workspace`.
- Run perft tests only (integration test file at `crates/chess_core/tests/perft_tests.rs`):
  - `cargo test -p chess_core --test perft_tests`
- Run the UCI engine (interactive): `cargo run -p uci_engine` (add `--release` for optimized binary).
- Incremental build artifacts and compiled crates appear under `target/`.

Project-specific patterns & conventions
- Perft is the canonical correctness check for move generation. Tests invoke `Position::from_fen(...)` and `perft(&mut pos, depth)` (see `crates/chess_core/tests/perft_tests.rs`).
- Public surface: `chess_core::Position` and functions exported from `lib.rs` are the integration points used by `uci_engine`.
- Mutability convention: many core functions accept `&mut Position` (e.g., `perft`, move application). Preserve this pattern when adding helpers.
- Tests: integration-style tests live in `crates/chess_core/tests/` (not `src/tests`). Add perft or regression tests there when fixing generator/search bugs.

Editing guidance and examples
- To add a new move-generation edge-case test, copy style from `perft_tests.rs`:
  - Construct `Position` via `Position::from_fen(fen_str)` and call `perft(&mut pos, depth)`.
- To change the public API used by `uci_engine`, update `crates/chess_core/src/lib.rs` and then run `cargo test --workspace` to catch compile errors across crates.
- When implementing search/eval changes, run a focused test run: `cargo test -p chess_core`.

Integration & cross-crate notes
- `uci_engine` depends on `chess_core` via workspace path. Keep `chess_core` public API stable for `uci_engine` unless both crates are adjusted together.
- UCI parsing and orchestration live in `crates/uci_engine/src/main.rs` and may import helpers from `chess_core::uci`.

What to look for when triaging bugs
- If perft mismatch occurs, start at `crates/chess_core/src/movegen.rs` and `perft.rs`.
- For search/evaluation regressions, inspect `search.rs` and `eval.rs` and re-run targeted tests.

Search tips (code patterns to grep)
- `Position::from_fen` — where positions are created.
- `perft(` — perft implementation and tests.
- `uci::` and `uci_engine` — UCI integration points.

If you're unsure
- Run the perft tests first — they catch move generation issues quickly.
- Ask to run CI or provide failing `cargo test` output; include the failing test name and FEN used.

Please review this guidance and tell me if any areas need more detail (build flags, CI steps, or uncommon local scripts).
