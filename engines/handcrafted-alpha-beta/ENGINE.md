## Overview

This engine is the current single baseline in the arena. It is a handcrafted classical searcher built around alpha-beta search rather than a neural policy/value stack. The goal is to keep one engine family that is understandable, tunable, and easy to evolve step by step.

## Search stack

- Board representation uses `cozy-chess`, which internally works with bitboards and fast move generation.
- Root move selection uses iterative deepening, so the engine repeatedly searches depth 1, 2, 3, and so on until the time budget expires.
- The main tree search is negamax with principal variation search (PVS), which searches the first move with a full window and later moves with a narrow scout window before re-searching when needed.
- Alpha-beta pruning cuts branches that cannot improve the current result.
- Quiescence search extends leaf nodes through tactical captures and promotion captures so the engine does not stop in the middle of an unstable exchange.
- A transposition table stores hash-keyed search results, best moves, depths, and bound types so repeated positions can be reused across branches and across moves.
- Repetition handling uses the known position-hash history from the current line so repeated positions are scored as draws instead of being over-pushed.

## Time management

The engine currently uses a straightforward move-time budget from the UCI `go movetime` command. It reserves a small safety margin, deepens while time remains, and falls back to the last fully completed iteration if the clock expires mid-search.

## Move ordering

Move ordering is one of the biggest strength multipliers in this engine and is intentionally layered.

- Transposition-table move first.
- Winning and forcing captures ordered with MVV-LVA style capture scores.
- Promotions pushed upward.
- Killer moves retained per ply.
- Quiet move history heuristic updated on beta cutoffs.

This makes the engine much closer to a serious classical baseline than a naive minimax implementation, even before deeper evaluation tuning.

## Evaluation model

The evaluation is tapered, meaning the engine keeps both middlegame and endgame scores and blends them according to the remaining material phase.

The score currently includes these components.

- Material values for pawns, knights, bishops, rooks, and queens.
- Piece-square tables for all pieces, with separate king tables for middlegame and endgame.
- Mobility bonuses for knights, bishops, rooks, and queens based on reachable squares after friendly blockers are removed.
- Pawn-structure terms including doubled pawns, isolated pawns, passed pawns, and connected support.
- Bishop-pair bonus.
- King safety built from pawn shield coverage, open-file exposure around the king, and enemy attack pressure inside the king zone.

The final evaluation is always converted into the side-to-move perspective so the negamax search can stay simple.

## Pawn structure details

- Doubled pawns are penalized because they block each other and reduce file coverage.
- Isolated pawns are penalized because they lack pawn support on adjacent files.
- Passed pawns are rewarded with larger bonuses as they advance.
- Friendly neighboring pawns give a small connected-pawn bonus.

These terms are intentionally simple and local for now. They are strong enough to shape style without making the code hard to reason about.

## King safety details

- The engine builds a king zone from the king square plus surrounding king moves.
- Enemy attacks into that zone are counted with heavier weights for stronger attackers.
- Friendly pawns in front of the king provide shield bonuses.
- Missing shield pawns and open files around the king create penalties.

This is a practical, readable king-safety model rather than a huge handcrafted attack-table system.

## Terminal scoring

- Checkmate is scored with mate-distance style values so faster mates are preferred and slower losses are resisted.
- Stalemate, fifty-move draws, and repetition are treated as draws.

## Current limitations

- No aspiration windows yet.
- No null-move pruning.
- No late-move reductions.
- No SEE-based capture pruning.
- No singular extensions.
- No opening book or endgame tablebases.
- No NNUE or learned evaluation.

This is deliberate. The engine is meant to be a clean baseline first.

## Best next upgrades

- Tune piece-square tables and eval weights.
- Add aspiration windows around iterative deepening.
- Add null-move pruning and late-move reductions.
- Improve king safety with attack unit scaling and safe-check bonuses.
- Split pawn evaluation into a pawn hash.
- Add stronger time management based on remaining clock and increment.
- Add better reporting from search to the UI if we later expose PV, depth, and node counts.

## How to read the code

- `Engine::choose_move` is the entry point from the UCI loop.
- `Searcher::search_root` controls iterative deepening and root ordering.
- `Searcher::pvs` is the main principal variation search.
- `Searcher::quiescence` handles tactical leaf stabilization.
- `evaluate` and helper functions implement the tapered handcrafted eval.

If you want to evolve the engine in the order suggested by the design notes, the clean path is:

- Keep the current material plus PST backbone stable.
- Tune mobility and pawn structure.
- Improve king safety.
- Add stronger move-ordering and pruning.
- Only then consider more advanced search reductions or learned evals.
