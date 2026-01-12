//! Move generation benchmark for profiling with cargo-flamegraph.
//!
//! This benchmark focuses specifically on move generation performance,
//! running many iterations of legal_moves_into on various positions.
//!
//! Usage:
//!   cargo flamegraph --example movegen_bench -p chess_core

use chess_core::{board::Position, movegen::legal_moves_into};
use std::time::Instant;

/// Positions covering different game phases and complexity levels
const TEST_POSITIONS: &[(&str, &str)] = &[
    // Opening positions
    (
        "Start",
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    ),
    (
        "e4",
        "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
    ),
    (
        "Sicilian",
        "rnbqkbnr/pp1ppppp/8/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2",
    ),
    // Complex middlegame
    (
        "Kiwipete",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -",
    ),
    (
        "Complex",
        "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - -",
    ),
    // Promotion positions
    (
        "Promotions",
        "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq -",
    ),
    // Endgame positions
    ("Rook endgame", "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - -"),
    (
        "Queen vs pieces",
        "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ -",
    ),
    // Edge cases
    (
        "Double check",
        "r1bqkb1r/pppp1Npp/2n2n2/4p2Q/2B1P3/8/PPPP1PPP/RNB1K2R b KQkq - 0 1",
    ),
    (
        "Pinned pieces",
        "r1bqkbnr/ppp2ppp/2np4/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 0 4",
    ),
];

const ITERATIONS: usize = 100_000;

fn main() {
    println!("=== Move Generation Benchmark ===");
    println!("Iterations per position: {ITERATIONS}");
    println!();

    let mut move_buf = Vec::with_capacity(256);
    let mut total_moves = 0usize;
    let mut total_time = std::time::Duration::ZERO;

    for (name, fen) in TEST_POSITIONS {
        let mut pos = Position::from_fen(fen);

        print!("{name:.<20}");

        let start = Instant::now();
        let mut moves_generated = 0usize;

        for _ in 0..ITERATIONS {
            legal_moves_into(&mut pos, &mut move_buf);
            moves_generated += move_buf.len();
        }

        let elapsed = start.elapsed();
        total_moves += moves_generated;
        total_time += elapsed;

        let moves_per_pos = moves_generated as f64 / ITERATIONS as f64;
        let mps = if elapsed.as_secs_f64() > 0.0 {
            ITERATIONS as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };

        println!(" {moves_per_pos:>5.1} moves/pos, {mps:>10.0} pos/sec ({elapsed:>8.3?})");
    }

    println!();
    println!("{:=<70}", "");
    let avg_mps = if total_time.as_secs_f64() > 0.0 {
        (ITERATIONS * TEST_POSITIONS.len()) as f64 / total_time.as_secs_f64()
    } else {
        0.0
    };
    println!("TOTAL: {total_moves} moves in {total_time:.3?} ({avg_mps:.0} positions/sec)");
}
