# Codex Local Debugging Rules

## Purpose
This repo is optimized for local Windows Codex tasks. Most bug work should stay local-first and should not require network access.

## Recommended Safe Commands
- `cargo check --workspace`
- `cargo test -p arena-server`
- `cargo test -p arena-server <module>::tests::<name>`
- `cargo fmt`
- `cargo clippy --workspace`
- `cd frontend; npm run build`
- `powershell -ExecutionPolicy Bypass -File scripts/debug/dev.ps1`
- `powershell -ExecutionPolicy Bypass -File scripts/debug/reset-db.ps1`
- `powershell -ExecutionPolicy Bypass -File scripts/debug/export-match-bundle.ps1 -MatchId <id>`
- `powershell -ExecutionPolicy Bypass -File scripts/debug/dump-match.ps1 -MatchId <id>`
- `powershell -ExecutionPolicy Bypass -File scripts/debug/repro-human-timeout.ps1`
- `powershell -ExecutionPolicy Bypass -File scripts/debug/repro-live-gap.ps1`

## Canonical Commands
- Backend startup: `cargo run -p arena-server`
- Frontend startup: `cd frontend; npm run dev`
- Targeted backend verification: `cargo test -p arena-server api::tests::name_of_test`
- Broad Rust verification: `cargo check --workspace`
- Frontend verification: `cd frontend; npm run build`

## Notes
- Repo code can document these recommended commands, but actual approval and allowlist behavior still needs to be configured in the Codex app or project settings.
- Prefer targeted commands before broad ones during bug fixes.
