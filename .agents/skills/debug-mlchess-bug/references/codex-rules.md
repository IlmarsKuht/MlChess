# MlChess Codex Rules

## Windows Local Commands
- `cargo run -p arena-server`
- `cargo test -p arena-server`
- `cargo test -p arena-server api::tests::name_of_test`
- `cargo check --workspace`
- `cargo fmt`
- `cd frontend; npm run dev`
- `cd frontend; npm run build`
- `powershell -ExecutionPolicy Bypass -File scripts/debug/dev.ps1`
- `powershell -ExecutionPolicy Bypass -File scripts/debug/reset-db.ps1`
- `powershell -ExecutionPolicy Bypass -File scripts/debug/export-match-bundle.ps1 -MatchId <id>`
- `powershell -ExecutionPolicy Bypass -File scripts/debug/dump-match.ps1 -MatchId <id>`
- `powershell -ExecutionPolicy Bypass -File scripts/debug/repro-human-timeout.ps1`
- `powershell -ExecutionPolicy Bypass -File scripts/debug/repro-live-gap.ps1`

## Workflow
- Prefer targeted commands before broad ones.
- Start from a debug bundle or ID if one is available.
- Add a failing regression test when feasible before the fix.
- Preserve live/replay semantics unless the bug requires changing them.
