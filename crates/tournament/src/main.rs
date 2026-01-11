//! Tournament CLI
//!
//! Run matches between engines and track Elo ratings.

use classical_engine::ClassicalEngine;
use ml_engine::NeuralEngine;
use tournament::{quick_match, EloTracker, MatchConfig, MatchRunner, TournamentConfig, TournamentResults};
use chess_core::Engine;
use std::env;

fn print_usage() {
    println!("ML-chess Tournament Runner");
    println!();
    println!("Usage:");
    println!("  tournament match <engine1> <engine2> [--games N] [--depth D]");
    println!("  tournament gauntlet <challenger> [--games N] [--depth D]");
    println!("  tournament leaderboard");
    println!();
    println!("Engines:");
    println!("  classical     - Alpha-beta with material eval");
    println!("  neural        - Neural network (random fallback)");
    println!("  neural:vNNN   - Neural network with specific model version");
    println!();
    println!("Examples:");
    println!("  tournament match classical neural --games 20 --depth 4");
    println!("  tournament gauntlet neural:v002 --games 10");
}

fn create_engine(spec: &str) -> Box<dyn Engine> {
    let parts: Vec<&str> = spec.split(':').collect();
    match parts[0].to_lowercase().as_str() {
        "classical" | "classic" => Box::new(ClassicalEngine::new()),
        "neural" | "nn" => {
            if parts.len() > 1 {
                match NeuralEngine::with_model("models/", parts[1]) {
                    Ok(engine) => Box::new(engine),
                    Err(e) => {
                        eprintln!("Warning: Failed to load model {}: {}", parts[1], e);
                        eprintln!("Using random fallback");
                        Box::new(NeuralEngine::new())
                    }
                }
            } else {
                Box::new(NeuralEngine::new())
            }
        }
        _ => {
            eprintln!("Unknown engine: {}", spec);
            Box::new(ClassicalEngine::new())
        }
    }
}

fn run_match(args: &[String]) {
    if args.len() < 2 {
        eprintln!("Error: match requires two engine specifications");
        print_usage();
        return;
    }

    let engine1_spec = &args[0];
    let engine2_spec = &args[1];

    // Parse optional arguments
    let mut num_games: u32 = 10;
    let mut depth: u8 = 4;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--games" | "-g" => {
                if i + 1 < args.len() {
                    num_games = args[i + 1].parse().unwrap_or(10);
                    i += 1;
                }
            }
            "--depth" | "-d" => {
                if i + 1 < args.len() {
                    depth = args[i + 1].parse().unwrap_or(4);
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    println!("=== Match: {} vs {} ===", engine1_spec, engine2_spec);
    println!("Games: {}, Depth: {}", num_games, depth);
    println!();

    let mut engine1 = create_engine(engine1_spec);
    let mut engine2 = create_engine(engine2_spec);

    let config = MatchConfig {
        num_games,
        depth,
        verbose: true,
        ..Default::default()
    };

    let runner = MatchRunner::new(config);
    let result = runner.run_match(engine1.as_mut(), engine2.as_mut());

    println!();
    println!("=== Final Result ===");
    println!(
        "{}: {} wins, {} losses, {} draws",
        engine1_spec, result.wins, result.losses, result.draws
    );
    println!("Score: {:.1}%", result.score() * 100.0);

    // Update Elo tracker
    let mut tracker = EloTracker::load("tournament_elo.json").unwrap_or_default();
    tracker.update_ratings(engine1_spec, engine2_spec, &result);
    tracker.print_leaderboard();

    if let Err(e) = tracker.save("tournament_elo.json") {
        eprintln!("Warning: Failed to save Elo tracker: {}", e);
    }
}

fn run_gauntlet(args: &[String]) {
    if args.is_empty() {
        eprintln!("Error: gauntlet requires a challenger engine");
        print_usage();
        return;
    }

    let challenger_spec = &args[0];

    // Parse optional arguments
    let mut num_games: u32 = 10;
    let mut depth: u8 = 4;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--games" | "-g" => {
                if i + 1 < args.len() {
                    num_games = args[i + 1].parse().unwrap_or(10);
                    i += 1;
                }
            }
            "--depth" | "-d" => {
                if i + 1 < args.len() {
                    depth = args[i + 1].parse().unwrap_or(4);
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    let opponents = vec!["classical"];

    println!("=== Gauntlet: {} vs all ===", challenger_spec);
    println!("Opponents: {:?}", opponents);
    println!("Games per match: {}, Depth: {}", num_games, depth);
    println!();

    let mut tracker = EloTracker::load("tournament_elo.json").unwrap_or_default();
    let mut results = TournamentResults::new(
        &format!("Gauntlet: {}", challenger_spec),
        std::iter::once(challenger_spec.to_string())
            .chain(opponents.iter().map(|s| s.to_string()))
            .collect(),
        TournamentConfig {
            games_per_match: num_games,
            search_depth: depth,
            ..Default::default()
        },
    );

    for opponent in opponents {
        println!("\n--- {} vs {} ---", challenger_spec, opponent);

        let mut challenger = create_engine(challenger_spec);
        let mut opp_engine = create_engine(opponent);

        let result = quick_match(challenger.as_mut(), opp_engine.as_mut(), num_games, depth);

        println!(
            "Result: {}-{}-{} (Score: {:.1}%)",
            result.wins,
            result.losses,
            result.draws,
            result.score() * 100.0
        );

        tracker.update_ratings(challenger_spec, opponent, &result);
        results.add_match(challenger_spec, opponent, result);
    }

    println!();
    tracker.print_leaderboard();
    results.print_report();

    if let Err(e) = tracker.save("tournament_elo.json") {
        eprintln!("Warning: Failed to save Elo tracker: {}", e);
    }
}

fn show_leaderboard() {
    match EloTracker::load("tournament_elo.json") {
        Ok(tracker) => tracker.print_leaderboard(),
        Err(_) => {
            println!("No tournament data found. Run some matches first!");
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        return;
    }

    match args[1].as_str() {
        "match" => run_match(&args[2..]),
        "gauntlet" => run_gauntlet(&args[2..]),
        "leaderboard" | "elo" => show_leaderboard(),
        "help" | "--help" | "-h" => print_usage(),
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            print_usage();
        }
    }
}
