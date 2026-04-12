# MlChess Bug Report Template

## What To Tell Codex
- Where you saw the bug: page, route, or watch URL
- What you did right before it happened
- What you expected
- What actually happened
- Any IDs you can see: `match_id`, `tournament_id`, `game_id`, `request_id`
- Timing details if relevant
- Paste the copied debug bundle if available

## Best Daily Prompt
Use `$debug-mlchess-bug`. Start from this bug bundle or report, reproduce the issue, add a failing regression test if feasible, make the smallest root-cause fix, run targeted verification, and summarize root cause, changed files, and remaining uncertainty.

## How To Paste A Debug Bundle
1. Open the hidden debug drawer in the frontend.
2. Click `Copy Debug Bundle`.
3. In Codex, invoke `$debug-mlchess-bug`.
4. Paste the full bundle into your Codex task.
5. Add one or two plain-language sentences describing what you saw.

## Good Report
`On the watch page for match 123, I submitted e2e4 as White and the board never updated. Expected the move to appear immediately. Actual result: spinner stopped, websocket stayed connected, and the copied debug bundle is below.`

## Also Good
`Bug on /#/watch/123. Around 21:15 local time the clocks jumped backward after reconnect. request_id was abc-123 and the bundle is pasted below.`

## Bad Report
- `The site is broken`
- `Moves weird`
- `Please fix live stuff`

Those reports force Codex to guess instead of starting from real state.
