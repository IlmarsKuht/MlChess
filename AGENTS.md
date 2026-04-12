# MlChess Codex Guide

## Architecture Map
- `crates/arena-core`: shared domain types, live protocol types, rating logic, pairing, and testing helpers.
- `crates/arena-runner`: engine adapter/runtime logic for UCI engines and match execution helpers.
- `crates/arena-server`: Axum API, websocket/live orchestration, SQLite persistence, runtime restore, and setup sync.
- `frontend`: Vite + React UI, including live watch/play flows, API calls, and websocket state.
- `setup`: backend-managed opening suites, pools, and event presets synced into SQLite.
- `engines`: runnable engine versions and manifests.
- `arena.db`: local SQLite database used by the server during local development.

## Where To Look First
- Backend API and HTTP/debug endpoints: `crates/arena-server/src/api.rs`
- Live websocket/replay flow: `crates/arena-server/src/live.rs`
- Runtime orchestration, timeouts, move handling, restore/finalization: `crates/arena-server/src/orchestration.rs`
- App state and in-memory runtime stores: `crates/arena-server/src/state.rs`
- Persistence helpers and debug bundle assembly: `crates/arena-server/src/storage.rs`
- SQLite schema/migrations-by-code: `crates/arena-server/src/db.rs`
- Frontend app shell and watch/play flows: `frontend/src/App.tsx`
- Frontend HTTP client/debug request capture: `frontend/src/app/api.ts`
- Frontend websocket client/debug live capture: `frontend/src/app/live.ts`

## Default Bug-Fix Workflow
1. Reproduce with the smallest targeted command, route, script, or debug bundle available.
2. Inspect the smallest likely area first instead of searching broadly.
3. Add a failing regression test when feasible before the fix.
4. Implement the smallest root-cause fix.
5. Run targeted verification first.
6. Run broader verification only if the targeted checks pass and the change touches shared behavior.
7. Summarize root cause, changed files, tests added or updated, commands run, and any remaining uncertainty.

## Debugging Rules
- Read this file and any nearer `AGENTS.md` before editing.
- Prefer targeted commands before broad ones.
- Prefer existing test modules nearest the behavior under change.
- Avoid speculative or cleanup-driven refactors during bug fixes.
- Preserve existing live, replay, and delayed-reveal semantics unless the bug explicitly requires changing them.
- Start from a provided debug bundle or request ID before doing broad repo search.
- Mirror the important workflow from the repo debugging skill here; do not assume the skill is always loaded automatically.

## Commands
- Backend setup/run: `cargo run -p arena-server`
- Targeted backend tests: `cargo test -p arena-server api::tests::name_of_test`
- Full backend tests: `cargo test -p arena-server`
- Broad Rust verification: `cargo check --workspace`
- Frontend dev: `cd frontend; npm run dev`
- Frontend build verification: `cd frontend; npm run build`
- Windows debug scripts: `scripts/debug/*.ps1`

## Repo Skill
- Preferred bug skill: `$debug-mlchess-bug`
- Repo skill path: `.agents/skills/debug-mlchess-bug/SKILL.md`
- Use the repo skill for MlChess bug reports instead of relying on pasted boilerplate prompts.

## Debugging Intent
- Every important debugging addition should help Codex reproduce a bug, find the right state quickly, create regression coverage, or verify a fix safely.
