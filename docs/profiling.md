# Performance Profiling Guide for ML-Chess

This document explains how to profile the chess engine to identify performance bottlenecks.

## Quick Start

```powershell
# Install flamegraph (one-time)
cargo install flamegraph

# Run perft profiling
cargo flamegraph --example perft_bench -p chess_core

# View the flamegraph
start flamegraph.svg
```

## Tools Overview

### 1. cargo-flamegraph (Primary Tool)

Generates interactive SVG flame graphs showing where CPU time is spent.

**Installation:**
```powershell
cargo install flamegraph
```

**Windows Requirements:**
- Run PowerShell as Administrator
- Windows 10/11 with ETW (Event Tracing for Windows) enabled

**Usage:**
```powershell
# Profile perft at default depth (5)
cargo flamegraph --example perft_bench -p chess_core

# Profile perft at custom depth
cargo flamegraph --example perft_bench -p chess_core -- 6

# Profile with specific position (Kiwipete)
cargo flamegraph --example perft_bench -p chess_core -- 5 "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -"

# Profile move generation specifically
cargo flamegraph --example movegen_bench -p chess_core
```

**Understanding Flamegraphs:**
- Width = time spent (wider = more time)
- Height = call stack depth
- Click to zoom into a section
- Look for wide bars - those are your bottlenecks!

### 2. Using the Profiling Script

```powershell
# Show help
.\scripts\profile.ps1 help

# Install tools
.\scripts\profile.ps1 install

# Profile perft
.\scripts\profile.ps1 perft
.\scripts\profile.ps1 perft 6

# Profile movegen
.\scripts\profile.ps1 movegen

# Open last flamegraph
.\scripts\profile.ps1 open
```

## Benchmark Examples

### perft_bench

Tests perft node counting at various depths:
- Measures overall move generation + make/unmake performance
- Uses multiple test positions for comprehensive coverage
- Reports nodes per second (NPS)

### movegen_bench

Focuses specifically on legal move generation:
- Runs many iterations on each position
- Measures positions processed per second
- Tests various position types (opening, middlegame, endgame)

## Profiling Tips

### What to Look For

1. **Wide bars at the bottom** = functions consuming most time
2. **Common bottlenecks in chess engines:**
   - `legal_moves_into` - move generation
   - `in_check` - check detection
   - `make_move` / `unmake_move` - position updates
   - `pseudo_moves` - pseudo-legal move generation
   - `piece_at` - board access patterns

### Performance Optimization Areas

Based on typical chess engine profiles:

1. **Move Generation:**
   - Consider bitboard representation instead of mailbox
   - Pre-compute attack tables
   - Use SIMD for parallel operations

2. **Check Detection:**
   - Maintain incremental attack information
   - Use slider attack lookup tables

3. **Make/Unmake Move:**
   - Minimize memory operations
   - Use copy-make vs make-unmake tradeoff analysis

4. **Memory Access:**
   - Improve cache locality
   - Reduce position cloning

## Custom Profiling Profile

The workspace includes a custom `profiling` profile for more accurate measurements:

```powershell
cargo flamegraph --profile profiling --example perft_bench -p chess_core
```

This enables:
- Link-time optimization (LTO)
- Single codegen unit
- Debug symbols for stack traces

## Alternative Tools

### cargo-criterion (Microbenchmarks)

For precise timing comparisons:
```powershell
cargo install cargo-criterion
cargo criterion -p chess_core
```

### Tracy (Real-time Profiler)

For detailed frame-by-frame analysis:
1. Download Tracy from https://github.com/wolfpld/tracy
2. Add `tracy-client` dependency
3. Instrument code with `tracy_client::span!()`

### Windows Performance Analyzer

For deep Windows-specific analysis:
1. Use Windows Performance Recorder (WPR)
2. Analyze with Windows Performance Analyzer (WPA)

## Interpreting Results

### Good Performance Indicators
- NPS > 10M nodes/second at depth 6
- Move generation < 100ns per position
- Balanced distribution across functions

### Red Flags
- Single function taking >50% of time
- Excessive memory allocation in hot paths
- Deeply nested call stacks

## Comparing Versions

Track performance across changes:

```powershell
# Before changes
cargo run --release --example perft_bench -p chess_core > before.txt

# After changes
cargo run --release --example perft_bench -p chess_core > after.txt

# Compare results
```

## Profiling Checklist

- [ ] Build in release mode with debug symbols
- [ ] Use consistent test positions
- [ ] Run multiple iterations for stable results
- [ ] Profile on same hardware for comparisons
- [ ] Check for background processes affecting results
- [ ] Document baseline before optimizing
