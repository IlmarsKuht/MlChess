## Overview

This engine keeps the same classical search stack as the handcrafted alpha-beta baseline, but the evaluation weights come from a tunable parameter profile rather than being fixed entirely by intuition. The point of this engine is not a different search family. The point is to turn evaluation development into a measurable loop.

## What is shared with the handcrafted engine

- Bitboard-based board representation through `cozy-chess`.
- Iterative deepening at the root.
- Principal variation search on top of alpha-beta pruning.
- Quiescence search for tactical leaf stabilization.
- Transposition table reuse.
- Repetition-aware draw handling.
- The same feature families in evaluation: material, PSTs, mobility, pawn structure, king safety, and bishop pair.

## What is different

The handcrafted engine bakes most evaluation values directly into the source as hand-chosen numbers.

This engine loads a structured tuned profile from `weights.json` at compile time and uses that profile to scale the major evaluation terms.

The tuned profile currently controls:

- Middlegame and endgame material values.
- Mobility weights by piece type.
- Doubled pawn penalties.
- Isolated pawn penalties.
- Connected pawn bonuses.
- Passed pawn base bonuses and rank scaling.
- Bishop-pair bonus.
- King shield bonuses and penalties.
- King attack pressure scaling.

## Tuning model

Right now the engine ships with one tuned parameter set, but the code is organized so the tuning target is explicit and externalized.

The intended pipeline is:

- Manual seed weights to get the engine stable.
- Local hill-climbing to test whether small changes help.
- SPSA for noisy large-scale tuning.
- Tournament-based parameter tuning for stronger validation.

## Why this engine exists

This engine is for the moment where “I think this value feels better” stops being enough.

It makes evaluation tuning into a data problem:

- expose the parameters
- run matches
- measure Elo movement
- update the profile
- rerun

That lets the project grow from handcrafted experimentation into reproducible engine optimization.

## Current implementation boundaries

- Search is still the same classical baseline.
- PST tables are still fixed tables from the baseline.
- The tuned profile currently scales the main scalar terms instead of tuning every last table entry.
- There is not yet an in-repo SPSA runner or tournament optimizer.

So this is a real auto-tuned-ready engine, but not yet a full tuning platform by itself.

## Best next upgrades

- Add a local tuning tool that mutates `weights.json` and runs arena matches automatically.
- Add SPSA batch generation and result ingestion.
- Split out more tuneable terms, including PST scaling buckets or individual PST offsets.
- Persist tuning runs and scores so you can compare profiles historically.
- Add separate tuned profiles for different time controls if needed.

## How to read the code

- `weights.json` is the tuning target.
- `load_weights` maps the JSON profile into the engine.
- Search stays almost identical to the handcrafted engine, which makes A/B comparison easier.
- Evaluation helpers consume the tuned profile so feature design and parameter tuning stay separate.
