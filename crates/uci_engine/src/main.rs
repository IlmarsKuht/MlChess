//! UCI Chess Engine Binary
//!
//! This binary implements the Universal Chess Interface (UCI) protocol,
//! allowing the engine to be used with chess GUIs like Arena, Cute Chess, etc.
//!
//! Supports multiple engine backends:
//! - Classical: Alpha-beta search with material evaluation
//! - Neural: Neural network-based evaluation (requires trained model)

use chess_core::{move_to_uci, set_position_from_uci, Engine, Position};
use classical_engine::ClassicalEngine;
use ml_engine::NeuralEngine;
use std::io::{self, BufRead, Write};

/// Available engine types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EngineType {
    Classical,
    Neural,
}

impl EngineType {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "classical" | "classic" | "alpha-beta" => Some(EngineType::Classical),
            "neural" | "nn" | "ml" => Some(EngineType::Neural),
            _ => None,
        }
    }
}

/// Creates an engine instance based on the type
fn create_engine(engine_type: EngineType) -> Box<dyn Engine> {
    match engine_type {
        EngineType::Classical => Box::new(ClassicalEngine::new()),
        EngineType::Neural => Box::new(NeuralEngine::new()),
    }
}

fn main() {
    // UCI engines communicate via stdin/stdout.
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    let mut pos = Position::startpos();
    let mut depth: u8 = 3;
    let mut engine_type = EngineType::Classical;
    let mut engine: Box<dyn Engine> = create_engine(engine_type);

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "uci" => {
                writeln!(stdout, "id name ML-chess {}", engine.name()).ok();
                writeln!(stdout, "id author {}", engine.author()).ok();
                // Engine options
                writeln!(stdout, "option name Depth type spin default 3 min 1 max 20").ok();
                writeln!(
                    stdout,
                    "option name Engine type combo default Classical var Classical var Neural"
                )
                .ok();
                writeln!(stdout, "option name ModelVersion type string default v001").ok();
                writeln!(stdout, "uciok").ok();
                stdout.flush().ok();
            }
            "isready" => {
                writeln!(stdout, "readyok").ok();
                stdout.flush().ok();
            }
            "setoption" => {
                // Parse: setoption name <name> value <value>
                if let Some(idx_name) = parts.iter().position(|&x| x == "name") {
                    if idx_name + 1 < parts.len() {
                        let option_name = parts[idx_name + 1];
                        let value = parts
                            .iter()
                            .position(|&x| x == "value")
                            .and_then(|idx| parts.get(idx + 1).copied());

                        match option_name.to_lowercase().as_str() {
                            "depth" => {
                                if let Some(v) = value {
                                    if let Ok(d) = v.parse::<u8>() {
                                        depth = d.clamp(1, 20);
                                    }
                                }
                            }
                            "engine" => {
                                if let Some(v) = value {
                                    if let Some(new_type) = EngineType::from_str(v) {
                                        if new_type != engine_type {
                                            engine_type = new_type;
                                            engine = create_engine(engine_type);
                                        }
                                    }
                                }
                            }
                            "modelversion" => {
                                if let Some(v) = value {
                                    engine.set_option("ModelVersion", v);
                                }
                            }
                            _ => {
                                // Try passing to engine
                                if let Some(v) = value {
                                    engine.set_option(option_name, v);
                                }
                            }
                        }
                    }
                }
            }
            "ucinewgame" => {
                pos = Position::startpos();
                engine.new_game();
            }
            "position" => {
                set_position_from_uci(&mut pos, &parts[1..]);
            }
            "go" => {
                // Parse optional depth override: "go depth X"
                let mut search_depth = depth;
                if let Some(idx) = parts.iter().position(|&x| x.eq_ignore_ascii_case("depth")) {
                    if idx + 1 < parts.len() {
                        if let Ok(d) = parts[idx + 1].parse::<u8>() {
                            search_depth = d.clamp(1, 20);
                        }
                    }
                }

                // Iterative deepening with info output
                let mut final_mv = None;
                for d in 1..=search_depth {
                    let result = engine.search(&pos, d);

                    if let Some(mv) = result.best_move {
                        final_mv = Some(mv);
                        writeln!(
                            stdout,
                            "info depth {} score cp {} nodes {} pv {}",
                            result.depth,
                            result.score,
                            result.nodes,
                            move_to_uci(mv)
                        )
                        .ok();
                        stdout.flush().ok();
                    } else {
                        break;
                    }
                }

                if let Some(mv) = final_mv {
                    writeln!(stdout, "bestmove {}", move_to_uci(mv)).ok();
                } else {
                    writeln!(stdout, "bestmove 0000").ok();
                }
                stdout.flush().ok();
            }
            "quit" => break,
            _ => {
                // Ignore unknown commands
            }
        }
    }
}
