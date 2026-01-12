//! Perft benchmark for profiling with cargo-flamegraph.
//!
//! Usage:
//!   cargo flamegraph --example perft_bench -p chess_core -- [depth] [fen]
//!
//! Examples:
//!   # Default: depth 5 from starting position
//!   cargo flamegraph --example perft_bench -p chess_core
//!
//!   # Custom depth
//!   cargo flamegraph --example perft_bench -p chess_core -- 6
//!
//!   # Custom depth and position (Kiwipete - complex middlegame)
//!   cargo flamegraph --example perft_bench -p chess_core -- 5 "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -"

use chess_core::{board::Position, perft::perft};
use std::env;
use std::time::Instant;

/// Standard test positions for comprehensive profiling
const TEST_POSITIONS: &[(&str, &str)] = &[
    (
        "Starting position",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    ),
    (
        "Kiwipete",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -",
    ),
    ("Position 3", "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - -"),
    (
        "Position 4",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq -",
    ),
    (
        "Position 5",
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ -",
    ),
    (
        "Position 6",
        "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - -",
    ),
];

fn main() {
    let args: Vec<String> = env::args().collect();

    let depth: u8 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(5);

    // If FEN provided, use single position mode
    if let Some(fen) = args.get(2) {
        run_single_position(fen, depth);
    } else {
        run_all_positions(depth);
    }
}

fn run_single_position(fen: &str, depth: u8) {
    let mut pos = Position::from_fen(fen);

    println!("Position: {fen}");
    println!("Depth: {depth}");
    println!();

    // Warm-up run at lower depth
    if depth > 2 {
        let _ = perft(&mut pos, depth.saturating_sub(2));
    }

    let start = Instant::now();
    let nodes = perft(&mut pos, depth);
    let elapsed = start.elapsed();

    let nps = if elapsed.as_secs_f64() > 0.0 {
        nodes as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };

    println!("Nodes: {nodes}");
    println!("Time: {elapsed:.3?}");
    println!("NPS: {nps:.0}");
}

fn run_all_positions(depth: u8) {
    println!("=== Perft Benchmark Suite ===");
    println!("Depth: {depth}");
    println!();

    let mut total_nodes = 0u64;
    let mut total_time = std::time::Duration::ZERO;

    for (name, fen) in TEST_POSITIONS {
        let mut pos = Position::from_fen(fen);

        print!("{name:.<30}");

        let start = Instant::now();
        let nodes = perft(&mut pos, depth);
        let elapsed = start.elapsed();

        total_nodes += nodes;
        total_time += elapsed;

        let nps = if elapsed.as_secs_f64() > 0.0 {
            nodes as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };

        println!(" {nodes:>12} nodes in {elapsed:>8.3?} ({nps:>10.0} nps)");
    }

    println!();
    println!("{:=<70}", "");
    let total_nps = if total_time.as_secs_f64() > 0.0 {
        total_nodes as f64 / total_time.as_secs_f64()
    } else {
        0.0
    };
    println!("TOTAL: {total_nodes} nodes in {total_time:.3?} ({total_nps:.0} nps)");
}
