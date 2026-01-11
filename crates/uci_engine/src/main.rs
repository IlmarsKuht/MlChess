use chess_core::{Position, move_to_uci, pick_best_move, set_position_from_uci};
use std::io::{self, BufRead, Write};

fn main() {
    // UCI engines communicate via stdin/stdout.
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    let mut pos = Position::startpos();
    let mut depth: u8 = 3; // simple default

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let parts: Vec<&str> = line.trim().split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "uci" => {
                writeln!(stdout, "id name ChessLabRust 0.1").ok();
                writeln!(stdout, "id author you").ok();
                // Offer one option just to show UCI option plumbing
                writeln!(stdout, "option name Depth type spin default 3 min 1 max 8").ok();
                writeln!(stdout, "uciok").ok();
                stdout.flush().ok();
            }
            "isready" => {
                writeln!(stdout, "readyok").ok();
                stdout.flush().ok();
            }
            "setoption" => {
                // Example: setoption name Depth value 4
                // Minimal parse:
                if let Some(idx_name) = parts.iter().position(|&x| x == "name") {
                    if idx_name + 1 < parts.len() && parts[idx_name + 1] == "Depth" {
                        if let Some(idx_val) = parts.iter().position(|&x| x == "value") {
                            if idx_val + 1 < parts.len() {
                                if let Ok(d) = parts[idx_val + 1].parse::<u8>() {
                                    depth = d.clamp(1, 8);
                                }
                            }
                        }
                    }
                }
            }
            "ucinewgame" => {
                pos = Position::startpos();
            }
            "position" => {
                set_position_from_uci(&mut pos, &parts[1..]);
            }
            "go" => {
                // Parse optional depth override: "go depth X"; default to current depth
                let mut search_depth = depth;
                if let Some(idx) = parts.iter().position(|&x| x.eq_ignore_ascii_case("depth")) {
                    if idx + 1 < parts.len() {
                        if let Ok(d) = parts[idx + 1].parse::<u8>() {
                            search_depth = d.clamp(1, 8);
                        }
                    }
                }

                // Iterative deepening: emit one info line per depth so UIs can plot eval history
                let mut final_mv = None;
                for d in 1..=search_depth {
                    if let Some((mv, score)) = pick_best_move(&pos, d) {
                        final_mv = Some(mv);
                        writeln!(
                            stdout,
                            "info depth {} score cp {} pv {}",
                            d,
                            score,
                            move_to_uci(mv)
                        )
                        .ok();
                        stdout.flush().ok();
                    } else {
                        // No legal moves; stop searching deeper
                        break;
                    }
                }

                if let Some(mv) = final_mv {
                    // Final bestmove uses deepest completed iteration
                    writeln!(stdout, "bestmove {}", move_to_uci(mv)).ok();
                } else {
                    writeln!(stdout, "bestmove 0000").ok(); // no moves
                }
                stdout.flush().ok();
            }
            "quit" => break,
            _ => {
                // ignore unknown commands
            }
        }
    }
}
