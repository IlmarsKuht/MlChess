# Match Runtime Ownership

`arena-server` is the authoritative owner of match runtime execution.
`arena-runner` is limited to low-level engine adapter/process/protocol concerns and pure chess-runtime helper functions.

The project does not keep two authoritative match execution paths. There must be exactly one runtime loop implementation, and it must live under `crates/arena-server/src/match_runtime/**`.

## Ownership Rules

- `arena-server` owns persisted match state, live checkpoints/events, replay bootstrap, human move submission, runtime restore, terminal persistence, tournament lifecycle integration, and rating application.
- `arena-runner` owns UCI process/session management, engine adapter construction, engine communication, and pure helpers such as move-budget calculation, board classification, PGN generation, and variant start positions.
- No match loop belongs in `arena-runner`.
- No persistence belongs in `arena-runner`.
- No live publication logic belongs outside `arena-server`.
- Runtime types and direct `MatchRuntime` mutation belong in `arena-server/src/match_runtime/**`.

## Module Shape

The runtime package should expose a narrow surface: runtime builders, owner entrypoints, and restore entrypoints. Internal helpers should stay private or `pub(crate)` inside focused modules.

Generic catch-all modules such as `orchestration.rs`, `state.rs`, and `utils.rs` should not be reintroduced for runtime responsibilities.
