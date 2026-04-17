# Module Boundaries

MlChess uses feature modules for orchestration, transport modules for HTTP and websocket edges, and repository modules for persistence. New behavior should go into the owning layer instead of legacy transition files.

## Backend

- `crates/arena-server/src/api/**` owns transport only: routes, extractors, DTOs, websocket upgrades, serialization, and status mapping.
- `crates/arena-server/src/bootstrap/**` owns server assembly, middleware, startup restore, and reconciliation.
- `crates/arena-server/src/storage/**` owns persistence only: SQL, row decoding, inserts, updates, and repository-style queries.
- `crates/arena-server/src/match_runtime/**` owns runtime state-machine behavior, live publication from runtime turns, finalization, and runtime-owned types.
- `crates/arena-server/src/human_games/**` and `crates/arena-server/src/tournaments/**` own feature orchestration. They may call storage and runtime services, but should not become transport or persistence modules.
- Legacy transition modules such as `lib.rs`, `api.rs`, `storage.rs`, and `state.rs` must not receive new behavior. Move new code into the focused module tree first.

## Frontend

- `frontend/src/shared/**` must be pure/shared and must not import from `frontend/src/app/**` or `frontend/src/features/**`.
- `frontend/src/app/**` owns app wiring, route-level providers, instrumentation, and debug integration.
- `frontend/src/features/**` owns feature workflows and can import from `app/**` and `shared/**`.
- Watch/live protocol reduction should stay pure and testable; websocket lifecycle and debug side effects belong in the hook layer.
