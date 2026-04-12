# Frontend Codex Guide

## Likely Files To Inspect
- `src/App.tsx`
- `src/app/live.ts`
- `src/app/api.ts`

## Bug Guidance
- Polling vs websocket issues:
  - Inspect periodic refresh logic in `src/App.tsx` and confirmed-live websocket state in `src/app/live.ts`.
- Fullscreen watch bugs:
  - Inspect route parsing, selected live match state, and watch-only rendering branches in `src/App.tsx`.
- Stale selected match or game state:
  - Inspect route syncing, `selectedLiveMatchId`, replay selection, and refresh side effects in `src/App.tsx`.
- Human move submission flow:
  - Inspect board selection logic in `src/App.tsx` and websocket submit handling in `src/app/live.ts`.
- Live follow / delayed reveal bugs:
  - Inspect `displayedLiveFrameCount`, reveal timers, and `isLiveFollowing` behavior in `src/App.tsx`.

## Debugging Rules
- Preserve current UX behavior unless the change intentionally adjusts it.
- Prefer using existing `fetchJson` and live websocket helpers instead of adding one-off network code.
- Keep debug UI hidden by default and easy to use in local development.
- Favor state visibility and reproducibility over adding generic console logs.
