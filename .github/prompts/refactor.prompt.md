You are a senior Rust engineer and chess engine developer.

I have a Rust chess project that has grown organically. The code works, but now I want to:
- clean it up
- improve structure and readability
- refactor for performance
- split responsibilities properly
- modularize and name things idiomatically
- prepare the codebase for future ML integration and scaling

This is a real chess engine project (core rules, move generation, search, UCI interface, ML modules).

IMPORTANT RULES:
1) Do NOT start writing or changing code immediately.
2) First, carefully analyze the codebase conceptually and propose a refactoring PLAN.
3) The plan must be explicit and ordered, with clear goals per step.
4) Identify:
   - architectural problems
   - performance issues
   - unnecessary allocations or clones
   - unclear ownership or lifetimes
   - modules that are doing too much
   - missing abstractions or over-abstractions
5) Propose a clean module structure suitable for a professional Rust project.
6) Call out tradeoffs explicitly (performance vs clarity, flexibility vs simplicity).

AFTER THE PLAN:
- immediately start implementing the plan step by step.
- Each step must compile and preserve behavior.
- No preference over small or big refactors, best refactors chosen first even if they are big.
- Explain *why* each change is made (especially performance-related ones).

PROJECT CONTEXT:
- Language: Rust
- Domain: Chess engine + UCI + ML evaluation
- Performance matters (search speed, allocations, cache-friendliness)
- Code will be profiled and benchmarked
- Future goals include GPU ML inference and tournaments

When ready, begin by presenting the refactoring plan ONLY.
