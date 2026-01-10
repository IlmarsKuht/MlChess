use std::time::Instant;

use rayon::prelude::*;

use chess_core::{Position, perft};

const FULL_PERFT_ENV: &str = "FULL_PERFT";
const NODE_LIMIT: u64 = 10_000_000;

fn parse_epd_line(line: &str) -> Option<(String, Vec<(u8, u64)>)> {
    let mut parts = line.split(';');
    let fen = parts.next()?.trim();
    if fen.is_empty() {
        return None;
    }

    let mut depths = Vec::new();
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut items = part.split_whitespace();
        let key = items.next().unwrap_or("");
        let val = items.next().unwrap_or("");
        if !key.starts_with('D') {
            continue;
        }
        let depth: u8 = key[1..]
            .parse()
            .unwrap_or_else(|_| panic!("Invalid depth token in EPD: {}", key));
        let expected: u64 = val
            .parse()
            .unwrap_or_else(|_| panic!("Invalid node count in EPD: {}", val));
        depths.push((depth, expected));
    }
    if depths.is_empty() {
        return None;
    }
    depths.sort_by_key(|(d, _)| *d);
    Some((fen.to_string(), depths))
}

#[test]
fn perft_from_standard_epd() {
    let full = std::env::var(FULL_PERFT_ENV).is_ok();
    let data = include_str!("standard.epd");
    let cases: Vec<(usize, String, Vec<(u8, u64)>)> = data
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            parse_epd_line(line).map(|(fen, depths)| (idx, fen, depths))
        })
        .collect();

    cases.par_iter().for_each(|(idx, fen, depths)| {
        let mut ran_depths = Vec::new();
        let mut total_nodes: u64 = 0;
        let case_start = Instant::now();

        for (depth, expected) in depths {
            if !full && *expected > NODE_LIMIT {
                eprintln!(
                    "Skipping depth {} for case {} (expected {} nodes) — set {}=1 to run all.",
                    depth,
                    idx + 1,
                    expected,
                    FULL_PERFT_ENV
                );
                continue;
            }
            let mut pos = Position::from_fen(fen);
            let got = perft(&mut pos, *depth);
            assert!(
                got == *expected,
                "Perft mismatch for FEN '{}' at depth {}: expected {}, got {}",
                fen,
                depth,
                expected,
                got
            );

            ran_depths.push(*depth);
            total_nodes += got;
        }

        let case_elapsed = case_start.elapsed();
        if !ran_depths.is_empty() {
            println!(
                "Case {:03} done: depths {:?}, total nodes {}, elapsed {:.3?} ({:.1} Mn/s)",
                idx + 1,
                ran_depths,
                total_nodes,
                case_elapsed,
                (total_nodes as f64 / 1_000_000.0) / case_elapsed.as_secs_f64()
            );
        }
    });
}
