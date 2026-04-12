# MlChess System Map

## Main Areas
- `crates/arena-core`: shared domain and live protocol types
- `crates/arena-server`: API, live websocket flow, persistence, restore, debug bundles
- `crates/arena-runner`: engine adapters and low-level game execution helpers
- `frontend`: watch/play UI, route state, polling, websocket client, debug drawer

## If Bug Is X, Inspect Y First
- API/debug bundle/request context issue:
  - `crates/arena-server/src/api.rs`
  - `crates/arena-server/src/lib.rs`
  - `crates/arena-server/src/storage.rs`
- Live replay gap or stale live board:
  - `crates/arena-server/src/live.rs`
  - `crates/arena-server/src/api.rs`
  - `frontend/src/app/live.ts`
- Human move rejection, timeout, clock drift, or restore:
  - `crates/arena-server/src/orchestration.rs`
  - `crates/arena-server/src/state.rs`
- Watch-page UI state or delayed reveal bug:
  - `frontend/src/App.tsx`
  - `frontend/src/app/api.ts`
  - `frontend/src/app/live.ts`

## Key IDs
- `request_id`: one API request
- `client_action_id`: one UI action
- `ws_connection_id`: one websocket connection
- `intent_id`: one human move intent
- `match_id`: one live or persisted match series
- `game_id`: one finished game
- `tournament_id`: one tournament
