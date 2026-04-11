# Live Game Rebuild Plan  
## Practical v1 Spec for Stable Real-Time Sync

## Goal

Rebuild the **live game path** so it becomes stable, authoritative, and easy to reason about.

This is **not** a rewrite of the whole product.

The goal is to fix the real-time path across:

- backend live state ownership
- move processing
- clock handling
- transport between backend and frontend
- reconnect/recovery
- frontend confirmed state
- recovery after restart

The target outcome is:

- one authoritative source of live truth per match
- no more overlapping live state mutation paths
- clocks stay smooth and predictable
- frontend never invents final truth
- reconnects recover cleanly
- architecture is extendable for duels, arena games, spectators, and later richer live features

---

# Core Principles

## 1. One owner per live match
Every active match must have exactly **one authoritative runtime owner**.

That owner is the only component allowed to mutate live state for the match.

Everything else must either:

- send intents to the owner
- consume authoritative events
- read durable state for bootstrap/recovery

No API route, background poll, frontend effect, or database query should directly invent or overwrite live truth.

## 2. The server is authoritative
The frontend may render smoothly and optimistically when needed, but it does **not** decide:

- timeout
- result
- legal move truth
- final clock truth
- terminal state

Only the server owner decides those.

## 3. Ordered seq-based state
Every authoritative live update for a match must carry a strictly increasing `seq`.

`seq` is the identity of ordered live truth.

Do not rely on:
- ply
- move count alone
- last-updated timestamps alone

Those are not enough once non-move events exist.

## 4. WebSocket is the live transport
Use WebSocket as the long-term live transport.

Reason:
- bidirectional by nature
- better fit for intents + events on one live channel
- easier long-term path for premoves, draw offers, presence, latency handling, richer spectator features, and room subscriptions

WebSocket does not solve correctness by itself. It only carries the protocol. Correctness still comes from:
- owner authority
- seq ordering
- replay
- recovery
- dedupe
- clear frontend state ownership

## 5. Separate truth from presentation
The system must distinguish between:

- **confirmed live truth**
- **presentation state**

Examples of presentation state:
- delayed spectator reveal
- scrubber position
- selected square
- move highlights
- follow-live toggle

Presentation state must never overwrite or redefine confirmed truth.

---

# Scope

## In scope
This rebuild covers the live-game path:

- active match runtime ownership
- live move processing
- live clocks
- authoritative live events
- frontend watch/live game state
- reconnect/replay
- restart recovery
- removal of overlapping legacy live mutation paths

## Out of scope for v1
Not required for the first pass:

- full distributed ownership
- full event sourcing for every transport update
- rich matchmaking redesign
- full tournament redesign
- advanced presence features
- draw-offer/premove implementation details
- large UI redesign

Those can build on this foundation later.

---

# High-Level Architecture

## Backend live path
For each active match:

- create or restore one live match owner
- route intents to that owner
- owner validates and applies state changes
- owner persists durable state
- owner broadcasts authoritative live events
- owner schedules timeout deadlines

## Frontend live path
For each watched match:

- one live module/store owns the live connection state for that match
- bootstrap from authoritative snapshot
- consume ordered events
- apply seq checks
- recover on reconnect if seq gap appears
- render smooth clocks locally from authoritative anchors

## Persistence
Use persistence for:

- recovery after restart
- durable transitions
- final archival records

Do **not** use persistence as the main per-frame live clock driver.

---

# Authoritative Runtime Model

## Match owner
Each active `match_id` should have one authoritative runtime owner.

This owner should control:

- current board/FEN
- move list
- clock state
- side to move
- status/result/termination
- live `seq`
- timeout scheduling
- live event emission

Implementation style is flexible:
- actor
- task
- manager-owned state machine
- match runtime object

The exact Rust structure can fit your codebase.

The important rule is ownership, not the class name.

## Ownership contract
Must hold:

- only one owner per active `match_id`
- duplicate owner startup must fail closed
- in v1 single-node, ownership may be enforced by an in-process registry
- future scale-out should be possible without changing protocol semantics

## Runtime responsibilities
The owner is responsible for:

- applying legal move transitions
- deciding timeout
- deciding terminal result
- scheduling deadlines
- generating seq-ordered events
- updating durable checkpoint
- serving as the sole writer of live truth

---

# State Model

## Match state fields
Authoritative live state should be able to represent at least:

- `match_id`
- `seq`
- `status`
- `result`
- `termination`
- `fen`
- `moves`
- `white_remaining_ms`
- `black_remaining_ms`
- `side_to_move`
- `turn_started_server_unix_ms`
- `updated_at`

## Suggested enums

### status
- `running`
- `finished`
- `aborted`

### result
- `white_win`
- `black_win`
- `draw`
- `none`

### termination
- `checkmate`
- `timeout`
- `resignation`
- `abort`
- `stalemate`
- `repetition`
- `insufficient_material`
- `fifty_move_rule`
- `illegal_move`
- `move_limit`
- `engine_failure`
- `none`

### side_to_move
- `white`
- `black`
- `none`

For terminal states, `side_to_move` should be `none`.

## Move representation
Moves should be represented as an ordered array/list of **UCI strings**.

That keeps wire payloads consistent and easy to parse.

## FEN semantics
`fen` should always represent the authoritative position **after** the event’s transition.

For snapshots, `fen` is the current authoritative position at the snapshot seq.

---

# Clock Model

## Server-owned truth
The server owns live clock truth.

The protocol should expose enough information for the client to render clocks smoothly without becoming authoritative.

## Recommended clock anchor fields
Each authoritative live state should include:

- `white_remaining_ms`
- `black_remaining_ms`
- `side_to_move`
- `server_now_unix_ms`
- `turn_started_server_unix_ms`

The server may use monotonic timing internally, but wire data should use stable Unix-ms fields so recovery, debugging, persistence, and cross-process reasoning stay simple.

## Client render rules
Client clock rendering should:

- estimate server offset from connection traffic
- use `performance.now()` for smooth local countdown rendering
- derive visible clock from the latest authoritative anchor
- clamp visible values at `0`
- never finalize timeout locally

If the visual clock reaches zero before a terminal event arrives, the UI may show `0`, but must wait for authoritative server state before declaring timeout or game end.

## Clock sync events
Optional periodic `clock_sync` events may be used to improve visual accuracy in low time.

These are for **render accuracy**, not correctness.

A practical policy might be:
- none above comfortable time
- more frequent near low time
- immediate sync after reconnect or lag spike

Exact intervals can be tuned later.

---

# Atomic Move Processing

## Rule
A legal move must be processed as one logical transition.

Do not split move handling across separate mutation paths.

## Logical flow
When handling a move intent, the owner should conceptually do:

1. receive the intent
2. capture one authoritative handling timestamp
3. verify the game is still active
4. verify the submitting side/player is allowed to act
5. verify the move is legal in the current authoritative position
6. compute elapsed time from the active turn anchor
7. subtract elapsed from the mover’s clock
8. resolve timeout edge case using the same authoritative timestamp
9. if still valid, apply the move
10. apply increment if applicable
11. switch side to move
12. set a new turn anchor
13. increment `seq`
14. persist the resulting durable state
15. broadcast the resulting event
16. schedule the next timeout deadline

This should be implemented in whatever form best fits the existing codebase, but the transition should remain logically atomic.

## Persist-before-broadcast rule
Use correctness-first ordering:

- update in-memory owner state
- persist durable transition/checkpoint
- broadcast authoritative event

Do not broadcast state that cannot be recovered after crash.

---

# Timeout Model

## Deadline-driven, not polling-driven
Timeout handling should be deadline-based.

When a turn starts:
- schedule one timeout deadline for the side to move

When a legal move is applied:
- cancel the old deadline
- compute new clocks
- schedule the next deadline

If the deadline fires first:
- the owner finalizes timeout
- updates durable state
- emits terminal event

## Important rule
Timeout must not depend on:
- someone hitting an API endpoint
- a spectator viewing the game
- frontend timers
- polling loops to discover expiration

Timeout must be owner-driven.

---

# Protocol Design

## Protocol style
Use a small, stable event protocol.

The protocol should be flexible enough to fit your current code without forcing a giant framework rewrite.

## Event names
Recommended live event names:

- `snapshot`
- `move_committed`
- `clock_sync`
- `game_finished`

That is enough for v1.

More event types can be added later.

## Common event envelope
Every live event should include at least:

- `protocol_version`
- `match_id`
- `seq`
- `event_type`
- `server_now_unix_ms`
- `status`

Additional fields depend on event type.

## Snapshot
A snapshot should be:
- render-complete
- recovery-complete

It should contain enough data for the frontend to fully reconstruct confirmed state.

Suggested fields:
- `match_id`
- `seq`
- `status`
- `result`
- `termination`
- `fen`
- `moves`
- `white_remaining_ms`
- `black_remaining_ms`
- `side_to_move`
- `turn_started_server_unix_ms`
- `server_now_unix_ms`

## Move committed event
Should include enough data to update confirmed state without ambiguity.

Suggested fields:
- `match_id`
- `seq`
- `move_uci`
- `fen`
- `moves`
- `white_remaining_ms`
- `black_remaining_ms`
- `side_to_move`
- `turn_started_server_unix_ms`
- `server_now_unix_ms`
- `status`

## Clock sync event
Optional event for visual accuracy.

Suggested fields:
- `match_id`
- `seq`
- `white_remaining_ms`
- `black_remaining_ms`
- `side_to_move`
- `turn_started_server_unix_ms`
- `server_now_unix_ms`
- `status`

## Game finished event
Should include final authoritative terminal state.

Suggested fields:
- `match_id`
- `seq`
- `status`
- `result`
- `termination`
- `fen`
- `moves`
- `white_remaining_ms`
- `black_remaining_ms`
- `side_to_move`
- `turn_started_server_unix_ms`
- `server_now_unix_ms`

---

# Intent Model

## Intents
Clients send intents, not final state.

Examples:
- `submit_move`
- `resign`
- `abort`

Future:
- `offer_draw`
- `accept_draw`
- `premove`

## Move intent shape
Suggested move intent fields:
- `intent_id`
- `move_uci`

`intent_id` should be client-generated.

## Intent acknowledgement
Intent acknowledgement is useful for UX, but is **not** authoritative match truth.

Suggested acknowledgement result kinds:
- `accepted`
- `duplicate`
- `rejected_illegal`
- `rejected_not_your_turn`
- `rejected_game_finished`

Exact naming can adapt to your codebase.

## Deduplication
Repeated delivery of the same intent should be handled safely.

Recommended dedupe scope:
- `match_id + submitting player + intent_id`

That avoids accidental duplicate move application during reconnect/retry situations.

---

# WebSocket Transport

## Why WebSocket here
Use WebSocket as the main live transport because it is a better fit for:
- live subscriptions
- bidirectional move flow
- reconnect recovery
- future richer live features

You can implement this with:
- raw WebSocket
- a WebSocket library
- Socket.IO style transport if that fits your stack

The protocol rules matter more than the exact library.

## Transport rules
Recommended behavior:

- client opens one live connection
- client subscribes to one or more `match_id`s
- server sends authoritative seq-based events
- connection should support heartbeat/ping/pong
- reconnect should include last seen seq per subscribed match
- if missed events are available, replay them
- otherwise send a fresh snapshot

## Replay buffer
Keep a short replay buffer of protocol events.

Recommended behavior:
- keyed by `match_id` and `seq`
- enough recent history for normal reconnects
- retain terminal event until cleanup is complete

The exact storage can be adapted:
- in-memory in v1
- Redis or shared layer later if needed

---

# Persistence and Recovery

## Durable checkpoint
Store one latest checkpoint per active game.

This should contain enough state to restore the live owner after restart.

Suggested contents:
- `match_id`
- `seq`
- `status`
- `result`
- `termination`
- `fen`
- `moves`
- `white_remaining_ms`
- `black_remaining_ms`
- `side_to_move`
- `turn_started_server_unix_ms`
- `updated_at`

## Durable transitions
Persist domain-significant transitions.

Examples:
- game created
- move committed
- resignation
- abort
- agreed draw
- game finished

Do not durably persist every transport-level `clock_sync` by default.

## Recovery model
Use different recovery paths for different problems.

### Reconnect recovery
For temporary client disconnects:
- use replay buffer first

### Restart recovery
For server/process restart:
- restore from durable checkpoint
- recreate the owner
- resume deadline scheduling

### Final archival record
Completed-game archival remains the long-term completed-game source of truth.

---

# Frontend State Rules

## One confirmed live store/module
The watch/live page should have one dedicated live module or store that alone is responsible for:

- opening/closing the live connection
- subscribing/unsubscribing to matches
- loading bootstrap snapshot
- applying ordered events
- handling replay/recovery
- exposing confirmed live state

The exact React structure can fit your app:
- Zustand
- reducer/store module
- context + reducer
- other local architecture

What matters is **single ownership of confirmed live truth** on the frontend.

## Seq apply rules
The frontend should apply strict seq rules.

Recommended behavior:

- reject any snapshot/event with `seq < last_applied_seq`
- accept bootstrap snapshot with `seq == last_applied_seq` as equivalent
- equal-seq non-bootstrap events are duplicates and no-op
- if an event arrives with `seq > last_applied_seq + 1`, enter recovery mode and request replay or fresh snapshot

## Forbidden paths
These should not be allowed once the new live path is enabled:

- overview polling mutating watch-page confirmed state
- ad hoc fetches overwriting confirmed live state unless explicitly recovery/bootstrap and seq-validated
- UI components recomputing authoritative truth from mixed sources
- component-local effects trying to “fix” clocks, results, or status

## Presentation split
Keep separate:

### Confirmed truth state
Contains:
- seq
- fen
- moves
- clocks
- side to move
- result/status/termination

### Presentation state
Contains:
- selected square
- scrubber position
- delayed spectator reveal
- move highlights
- follow-live toggle

Presentation state must never redefine confirmed truth.

---

# Invariants

These should always hold:

- `seq` strictly increases per game
- exactly one owner exists per active `match_id`
- server is the only authority for timeout and final result
- exactly one side is to move unless terminal
- authoritative clocks are never negative
- stale snapshot/event never overwrites newer applied state
- duplicate event application is a no-op
- terminal state is immutable once emitted
- move processing and resulting state update share one atomic `seq`

These invariants are more important than any one implementation detail.

---

# Observability

Track enough data to debug live problems quickly.

Recommended metrics/logging:

- current seq per game
- active owner count
- duplicate-owner prevention count
- reconnect count
- replay success count
- replay buffer miss rate
- snapshot fallback count
- stale snapshot rejected count
- duplicate event apply count
- move intent rejection count by reason
- move intent to broadcast latency
- timeout deadline fire to terminal event latency
- client clock correction magnitude after sync
- seq gap size on reconnect/recovery

You do not need perfect dashboards on day one, but you do need visibility.

---

# Test Plan

## Low-time correctness
Test:
- 10s, 5s, 3s, 1s games
- move submitted right before timeout
- move submitted near zero
- timeout firing during validation pressure

## Ordering and duplication
Test:
- duplicate move intent delivery
- duplicate live event delivery
- out-of-order event arrival
- stale snapshot after newer events
- equal-seq bootstrap handling
- terminal event applied twice

## Recovery
Test:
- reconnect after short disconnect
- replay success after missed events
- replay miss causing snapshot recovery
- server restart during active match
- server restart during low time
- old snapshot arriving after terminal event

## Browser/runtime weirdness
Test:
- background tab
- sleep/wake
- throttled timers
- local system time changes while watching a live game

## Frontend isolation
Test:
- overview polling cannot corrupt watch-page state
- delayed spectator mode cannot affect authoritative clocks/result
- UI-only state cannot mutate confirmed truth

---

# Delivery Plan

## Phase 1 — backend owner runtime
Implement the authoritative live match owner and deadline-driven timeout model.

Focus on:
- single owner per match
- owner-held live state
- atomic move flow
- timeout ownership

## Phase 2 — protocol and seq
Introduce:
- per-match `seq`
- event shapes
- clock anchors
- intent model
- authoritative snapshots/events

## Phase 3 — persistence and replay
Add:
- durable live checkpoint
- durable domain transitions
- replay buffer for reconnects

## Phase 4 — WebSocket transport
Add:
- live subscriptions
- intent send path
- authoritative event push
- reconnect with last seen seq
- replay or snapshot fallback

## Phase 5 — frontend confirmed live store
Build one dedicated live state owner on the frontend.

Focus on:
- bootstrap
- event apply rules
- seq handling
- recovery flow
- monotonic local clock rendering

## Phase 6 — migration cleanup
Remove overlapping legacy live mutation paths.

This is mandatory.
Do not keep the old overlapping watch-state logic alive “temporarily” longer than needed.

## Phase 7 — hardening
Add:
- torture tests
- low-time tests
- reconnect tests
- restart recovery tests
- metrics/observability improvements

## Phase 8 — extension
Once stable, extend the same model to:
- duels
- human live games
- tournament live views
- spectators
- future richer live features

---

# Implementation Guidance for Copilot

This plan is intentionally strict on **architecture** and flexible on **code shape**.

That means Copilot should help implement:

- modules that preserve single ownership
- event types that preserve seq semantics
- recovery flow that prefers replay then snapshot
- frontend store boundaries that prevent conflicting state writes

But it should **not** force:
- one exact Rust actor library
- one exact database schema naming pattern
- one exact frontend state library
- one exact folder structure

The rules matter more than the exact file names.

## Safe implementation interpretation
A Copilot-friendly interpretation of this spec is:

- keep live match runtime isolated
- keep move processing centralized
- keep protocol small and explicit
- keep replay and checkpoints simple
- keep frontend confirmed state single-owned
- remove old overlapping real-time paths as part of migration

---

# Final Rule

If any implementation choice conflicts with this question:

> “Does this introduce another place that can silently invent or overwrite live truth?”

then that implementation choice is probably wrong.

That is the rule that should guide the rebuild.
