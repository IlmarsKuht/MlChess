# Rust Chess Arena

Local chess benchmarking platform for UCI engines, written in Rust with a React dashboard.

## Workspace

- `crates/arena-core`: shared domain model, pairing, opening parsing, and Elo logic
- `crates/arena-runner`: UCI process orchestration and match execution
- `crates/arena-server`: Axum API, SQLite persistence, tournament coordinator, and static asset hosting
- `crates/engine-sdk`: helpers for building Rust UCI engines
- `engines`: runnable engines discovered from workspace crates or command manifests
- `setup`: code-managed opening suites and benchmark pool manifests
- `frontend`: React dashboard

## Run The Backend

```powershell
cargo run -p arena-server
```

The API starts on `http://127.0.0.1:4000` and creates `arena.db` in the workspace root by default.
The server serves prebuilt frontend files from `frontend/dist`, so frontend source changes need a frontend build first.

## Build And Run Everything

```powershell
./run-all.ps1
```

This rebuilds the frontend, builds the Rust workspace, and then starts `arena-server`.

## Build Everything

```powershell
./build-all.ps1
```

This rebuilds the React frontend into `frontend/dist` and then runs `cargo build --workspace`.

## Run The Frontend

Install Node.js first, then:

```powershell
cd frontend
npm install
npm run dev
```

Vite proxies `/api` requests to the Rust server during development.

## Code-Managed Setup

- `handcrafted-alpha-beta/v1`: the current single classical baseline with iterative deepening, PVS alpha-beta, quiescence, a transposition table, and a tapered handcrafted evaluation
- `auto-tuned-classical/v1`: the same classical search family, but with evaluation weights sourced from an auto-tuned parameter profile instead of being chosen manually
- long-form engine documentation can live next to each engine in an `ENGINE.md` file and be referenced with `documentation_file = "ENGINE.md"` in the engine manifest so the UI can show a deep clickable dossier for that engine

Rust engines live under `engines/*` and are discovered from `Cargo.toml` plus `[package.metadata.arena]`.
Use a stable `agent_key` for an engine family and a unique `version_key` like `v1`, `v2`, or `dev`
for each runnable version. Released versions should stay immutable; only `dev` should be edited.
Command or ML engines can be added with `engines/<slug>/arena-engine.toml`. Keep a tiny
placeholder `Cargo.toml` in that folder as well so the workspace glob remains valid.

```powershell
cargo build -p handcrafted-alpha-beta
cargo build -p auto-tuned-classical
```

Opening suites live in `setup/openings/*.toml`, pools live in `setup/pools/*.toml`, and event presets
live in `setup/events/*.toml`. The server syncs these manifests into SQLite on startup and when setup
files change, pruning removed registry entries automatically.

## Verification

- `cargo check --workspace`
- `cargo test --workspace`

## Starter Openings

The repo includes a small starter FEN suite at `data/openings/starter.fens`, referenced by
`setup/openings/starter-benchmark-suite.toml`.
