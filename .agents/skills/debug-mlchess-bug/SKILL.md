---
name: debug-mlchess-bug
description: Debug MlChess bugs end-to-end in this repository. Use when a user reports a frontend, live websocket, runtime/orchestration, persistence, or engine behavior bug in MlChess and Codex needs to reproduce it, start from a copied debug bundle or request/entity IDs, add regression coverage when feasible, make the smallest root-cause fix, and run targeted verification.
---

# Debug MlChess Bug

Read the active `AGENTS.md` chain first, especially the repo root file and any nearer file under `crates/arena-server` or `frontend`.

## Inputs To Extract
- route or page
- `match_id`, `tournament_id`, `game_id`
- `request_id`, `client_action_id`, `ws_connection_id`, `intent_id`
- `move_uci`
- timing details
- observed behavior
- expected behavior

## First Moves
1. If a copied debug bundle is present, start there instead of broad repo search.
2. Classify the bug as one of:
   - frontend rendering/state
   - websocket/live sync
   - orchestration/runtime
   - persistence/db
   - engine/adapter behavior
3. Inspect the smallest likely file set first.

## Working Rules
- Prefer targeted commands before broad ones.
- Preserve live/replay semantics unless the bug requires changing them.
- Avoid speculative refactors during bug fixes.
- Add a failing regression test whenever feasible before the fix.
- If direct reproduction is hard, use the nearest reliable request ID, debug bundle, script, or existing regression test.

## Repo Seams
- Backend API/debug endpoints: `crates/arena-server/src/api.rs`
- Live replay/runtime store: `crates/arena-server/src/live.rs`
- Runtime, timeouts, restore, move handling: `crates/arena-server/src/orchestration.rs`
- Persistence/schema/journaling: `crates/arena-server/src/storage.rs`, `crates/arena-server/src/db.rs`
- Frontend app shell/watch flow: `frontend/src/App.tsx`
- Frontend API request capture: `frontend/src/app/api.ts`
- Frontend websocket capture: `frontend/src/app/live.ts`

## References
- Read `references/system-map.md` for bug-to-file lookup and ID flow.
- Read `references/codex-rules.md` for safe commands and Windows local workflow.
- Read `references/bug-report-template.md` when you need the preferred user-facing bug report format.

## Output
Finish with a short summary containing:
- root cause
- files changed
- test added or updated
- commands run
- remaining uncertainty
