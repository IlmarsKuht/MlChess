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

## Run The Frontend

Install Node.js first, then:

```powershell
cd frontend
npm install
npm run dev
```

Vite proxies `/api` requests to the Rust server during development.

## Code-Managed Setup

- `material-plus-v1` / `material-plus-dev`: frozen release plus mutable development line for the Material Plus family
- `minimax-v1` / `minimax-dev`: frozen release plus mutable development line for the MiniMax family
- `king-safety-v1` / `king-safety-dev`: frozen release plus mutable development line for the King Safety family

Rust engines live under `engines/*` and are discovered from `Cargo.toml` plus `[package.metadata.arena]`.
Use a stable `agent_key` for an engine family and a unique `version_key` like `v1`, `v2`, or `dev`
for each runnable version. Released versions should stay immutable; only `dev` should be edited.
Command or ML engines can be added with `engines/<slug>/arena-engine.toml`. Keep a tiny
placeholder `Cargo.toml` in that folder as well so the workspace glob remains valid.

```powershell
cargo build -p material-plus-v1 -p material-plus-dev -p minimax-v1 -p minimax-dev -p king-safety-v1 -p king-safety-dev
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
