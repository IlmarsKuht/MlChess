# MlChess System Map

## Architecture Map
- `arena-core`: shared domain types, logs, live protocol, pairings, rating
- `arena-runner`: engine adapter and move execution helpers
- `arena-server`: API, live websocket flow, persistence, restore, debug bundles
- `frontend`: UI routes, watch/play flows, polling, websocket client, debug drawer
- SQLite: `arena.db` in repo root by default

## If Bug Is X, Inspect Y First
- API error or missing data:
  - `crates/arena-server/src/api.rs`
  - `crates/arena-server/src/storage.rs`
- Live replay gap or stale watch board:
  - `crates/arena-server/src/live.rs`
  - `crates/arena-server/src/api.rs`
  - `frontend/src/app/live.ts`
- Timeout, clock drift, or illegal human move handling:
  - `crates/arena-server/src/orchestration.rs`
  - `frontend/src/App.tsx`
- Restore after restart mismatch:
  - `crates/arena-server/src/lib.rs`
  - `crates/arena-server/src/orchestration.rs`
  - `crates/arena-server/src/storage.rs`
- Selected match, fullscreen watch, or delayed reveal bug:
  - `frontend/src/App.tsx`

## Common IDs
- `request_id`: one API request
- `client_action_id`: one user-triggered action in the UI
- `ws_connection_id`: one websocket connection lifecycle
- `intent_id`: one submitted human move intent
- `match_id`: one live or persisted match series
- `game_id`: one finished persisted game
- `tournament_id`: one tournament or human-game wrapper tournament

## Debugging Priority
Start from the most specific thing available:
1. copied debug bundle
2. `$debug-mlchess-bug`
3. request ID or entity ID
4. targeted script or test
5. narrow file inspection
6. broad repo search only if the above do not localize the issue
