# Arena Server Codex Guide

## Ownership
- This crate owns the API, orchestration, live runtime, persistence, restore flow, and debug bundle APIs.

## Likely Files To Inspect
- `src/api.rs`
- `src/live.rs`
- `src/orchestration.rs`
- `src/state.rs`
- `src/storage.rs`
- `src/db.rs`

## Bug Guidance
- Websocket/live gap issues:
  - Inspect replay bootstrap and gap fallback in `src/api.rs` and replay buffering in `src/live.rs`.
  - Preserve snapshot-vs-replay behavior unless the bug proves it is wrong.
- Snapshot vs replay issues:
  - Check `initial_stream_events`, `LiveMatchStore::replay_since`, and persisted `live_runtime_events`.
- Timeout and clock drift:
  - Inspect `process_human_move`, `process_engine_turn`, `emit_human_clock_sync`, and related checkpoint publishing in `src/orchestration.rs`.
- Human move rejection or duplicate issues:
  - Inspect websocket message handling in `src/api.rs`, intent dedupe in `src/orchestration.rs`, and any request/debug correlation fields.
- Restore/runtime restart issues:
  - Inspect `restore_live_runtime` in `src/lib.rs`, runtime restore helpers in `src/orchestration.rs`, and checkpoint/event persistence in `src/storage.rs`.

## Testing
- Add or extend regression tests in the nearest existing test module when possible.
- Prefer targeted `cargo test -p arena-server <module>::tests::<name>` before running the whole crate.
- If direct regression coverage is hard, add the nearest reliable storage/api/runtime test and explain the gap.
