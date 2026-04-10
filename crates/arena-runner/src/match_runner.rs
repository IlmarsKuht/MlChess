use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow};
use arena_core::{
    AgentVersion, GameLogEntry, GameRecord, GameResult, GameTermination, LiveGameFrame,
    LiveGameState, LiveSide, MatchPair, MatchSeries, MatchStatus, OpeningPosition, TimeControl,
    Variant,
};
use async_trait::async_trait;
use chrono::Utc;
use cozy_chess::{Board, Color, GameStatus, util};
use uuid::Uuid;
use tokio::time::timeout;

use crate::{
    calculate_move_budget, classify_position, classify_terminal_board, pgn_from_moves,
    starting_board,
    uci::UciAgentAdapter,
};

#[derive(Clone)]
pub struct MatchRequest {
    pub tournament_id: Uuid,
    pub match_series: MatchSeries,
    pub variant: Variant,
    pub white: AgentVersion,
    pub black: AgentVersion,
    pub opening: Option<OpeningPosition>,
    pub time_control: TimeControl,
    pub max_plies: u16,
    pub opening_seed: Option<u64>,
    pub progress_sink: Option<Arc<dyn Fn(LiveGameState) + Send + Sync>>,
}

#[derive(Clone)]
pub struct MatchBundleRequest {
    pub template: MatchRequest,
    pub paired_games: bool,
    pub swap_colors: bool,
}

#[derive(Clone)]
pub struct MatchPairRequest {
    pub engine_a_id: Uuid,
    pub engine_b_id: Uuid,
    pub games: Vec<MatchRequest>,
}

#[async_trait]
pub trait AgentAdapter: Send {
    async fn prepare(&mut self, variant: Variant, logs: &mut Vec<GameLogEntry>) -> Result<()>;
    async fn begin_game(&mut self, logs: &mut Vec<GameLogEntry>) -> Result<()>;
    async fn choose_move(
        &mut self,
        board: &Board,
        start_fen: &str,
        moves: &[String],
        movetime_ms: u64,
        logs: &mut Vec<GameLogEntry>,
    ) -> Result<String>;
    async fn shutdown(&mut self, logs: &mut Vec<GameLogEntry>) -> Result<()>;
}

pub fn build_adapter(version: AgentVersion) -> Box<dyn AgentAdapter> {
    Box::new(UciAgentAdapter::new(version))
}

pub async fn play_match_bundle(request: MatchBundleRequest) -> Result<Vec<GameRecord>> {
    let mut requests = vec![request.template.clone()];
    if request.paired_games && request.swap_colors {
        let mut reversed = request.template.clone();
        reversed.match_series = MatchSeries {
            id: Uuid::new_v4(),
            white_version_id: request.template.black.id,
            black_version_id: request.template.white.id,
            ..request.template.match_series
        };
        reversed.white = request.template.black;
        reversed.black = request.template.white;
        requests.push(reversed);
    }

    let mut games = Vec::with_capacity(requests.len());
    for game_request in requests {
        games.push(play_single_game(game_request).await?);
    }
    Ok(games)
}

pub async fn play_match_pair(request: MatchPairRequest) -> Result<MatchPair> {
    let mut games = Vec::with_capacity(request.games.len());
    for game_request in request.games {
        games.push(play_single_game(game_request).await?);
    }

    Ok(MatchPair {
        engine_a: request.engine_a_id,
        engine_b: request.engine_b_id,
        games,
    })
}

pub async fn play_single_game(request: MatchRequest) -> Result<GameRecord> {
    let started_at = Utc::now();
    let start_instant = Instant::now();
    let mut logs = Vec::new();
    let mut board = starting_board(
        request.variant,
        request.opening.as_ref(),
        request.opening_seed,
    );
    let start_fen = board.to_string();
    let mut current_fen = start_fen.clone();
    let mut move_history = Vec::<String>::new();
    let mut white_time_left_ms = request.time_control.initial_ms;
    let mut black_time_left_ms = request.time_control.initial_ms;
    let mut repetitions = HashMap::<u64, u8>::from([(board.hash_without_ep(), 1)]);

    let mut white = build_adapter(request.white.clone());
    let mut black = build_adapter(request.black.clone());
    white.prepare(request.variant, &mut logs).await?;
    black.prepare(request.variant, &mut logs).await?;
    white.begin_game(&mut logs).await?;
    black.begin_game(&mut logs).await?;

    publish_live_state(
        &request,
        &start_fen,
        &current_fen,
        &move_history,
        white_time_left_ms,
        black_time_left_ms,
        MatchStatus::Running,
        None,
        None,
    );

    let mut result = GameResult::Draw;
    let mut termination = GameTermination::MoveLimit;

    for _ in 0..request.max_plies {
        if let Some((game_result, reason)) = classify_position(&board, &repetitions) {
            result = game_result;
            termination = reason;
            break;
        }

        let side = board.side_to_move();
        let remaining = match side {
            Color::White => white_time_left_ms,
            Color::Black => black_time_left_ms,
        };
        let movetime_ms = calculate_move_budget(remaining, request.time_control.increment_ms);
        let actor: &mut Box<dyn AgentAdapter> = match side {
            Color::White => &mut white,
            Color::Black => &mut black,
        };

        let move_started = Instant::now();
        let selected = timeout(
            Duration::from_millis(
                remaining
                    .saturating_add(request.time_control.increment_ms)
                    .saturating_add(250),
            ),
            actor.choose_move(&board, &start_fen, &move_history, movetime_ms, &mut logs),
        )
        .await;
        let elapsed_ms = move_started.elapsed().as_millis() as u64;

        let clock = match side {
            Color::White => &mut white_time_left_ms,
            Color::Black => &mut black_time_left_ms,
        };
        *clock = clock
            .saturating_add(request.time_control.increment_ms)
            .saturating_sub(elapsed_ms);

        if elapsed_ms > remaining.saturating_add(request.time_control.increment_ms) {
            result = if side == Color::White {
                GameResult::BlackWin
            } else {
                GameResult::WhiteWin
            };
            termination = GameTermination::Timeout;
            break;
        }

        let selected = match selected {
            Ok(Ok(selected)) => selected,
            Ok(Err(_)) => {
                result = if side == Color::White {
                    GameResult::BlackWin
                } else {
                    GameResult::WhiteWin
                };
                termination = GameTermination::EngineFailure;
                break;
            }
            Err(_) => {
                result = if side == Color::White {
                    GameResult::BlackWin
                } else {
                    GameResult::WhiteWin
                };
                termination = GameTermination::Timeout;
                break;
            }
        };
        if selected == "0000" {
            result = GameResult::Draw;
            termination = GameTermination::EngineFailure;
            break;
        }

        let mv = util::parse_uci_move(&board, &selected)
            .map_err(|err| anyhow!("engine returned invalid UCI move: {err}"))?;
        if board.try_play(mv).is_err() {
            result = if side == Color::White {
                GameResult::BlackWin
            } else {
                GameResult::WhiteWin
            };
            termination = GameTermination::IllegalMove;
            break;
        }

        move_history.push(selected);
        current_fen = board.to_string();
        *repetitions.entry(board.hash_without_ep()).or_insert(0) += 1;

        publish_live_state(
            &request,
            &start_fen,
            &current_fen,
            &move_history,
            white_time_left_ms,
            black_time_left_ms,
            MatchStatus::Running,
            None,
            None,
        );

        if !matches!(board.status(), GameStatus::Ongoing) {
            let (game_result, reason) = classify_terminal_board(&board);
            result = game_result;
            termination = reason;
            break;
        }
    }

    white.shutdown(&mut logs).await.ok();
    black.shutdown(&mut logs).await.ok();

    let completed_at = Utc::now();
    let pgn = pgn_from_moves(
        &request.match_series.id.to_string(),
        request.variant,
        &start_fen,
        &move_history,
        result,
    );

    publish_live_state(
        &request,
        &start_fen,
        &current_fen,
        &move_history,
        white_time_left_ms,
        black_time_left_ms,
        MatchStatus::Completed,
        Some(result),
        Some(termination),
    );

    Ok(GameRecord {
        id: Uuid::new_v4(),
        tournament_id: request.tournament_id,
        match_id: request.match_series.id,
        pool_id: request.match_series.pool_id,
        variant: request.variant,
        opening_id: request.opening.as_ref().map(|opening| opening.id),
        white_version_id: request.white.id,
        black_version_id: request.black.id,
        result,
        termination,
        start_fen,
        pgn,
        moves_uci: move_history,
        white_time_left_ms,
        black_time_left_ms,
        logs: finalize_logs(logs, start_instant.elapsed().as_millis() as u64),
        started_at,
        completed_at,
    })
}

fn publish_live_state(
    request: &MatchRequest,
    start_fen: &str,
    current_fen: &str,
    moves_uci: &[String],
    white_time_left_ms: u64,
    black_time_left_ms: u64,
    status: MatchStatus,
    result: Option<GameResult>,
    termination: Option<GameTermination>,
) {
    let Some(sink) = &request.progress_sink else {
        return;
    };

    sink(LiveGameState {
        match_id: request.match_series.id,
        tournament_id: request.tournament_id,
        pool_id: request.match_series.pool_id,
        variant: request.variant,
        white_version_id: request.white.id,
        black_version_id: request.black.id,
        start_fen: start_fen.to_string(),
        current_fen: current_fen.to_string(),
        moves_uci: moves_uci.to_vec(),
        white_time_left_ms,
        black_time_left_ms,
        status,
        result,
        termination,
        updated_at: Utc::now(),
        live_frames: build_live_frames(
            start_fen,
            current_fen,
            moves_uci,
            white_time_left_ms,
            black_time_left_ms,
            status,
            result,
            termination,
        ),
    });
}

fn build_live_frames(
    _start_fen: &str,
    current_fen: &str,
    moves_uci: &[String],
    white_time_left_ms: u64,
    black_time_left_ms: u64,
    status: MatchStatus,
    result: Option<GameResult>,
    termination: Option<GameTermination>,
) -> Vec<LiveGameFrame> {
    let ply = moves_uci.len() as u32;
    vec![LiveGameFrame {
        ply,
        fen: current_fen.to_string(),
        move_uci: moves_uci.last().cloned(),
        white_time_left_ms,
        black_time_left_ms,
        updated_at: Utc::now(),
        side_to_move: side_from_fen(current_fen),
        status,
        result,
        termination,
    }]
}

fn side_from_fen(fen: &str) -> LiveSide {
    if fen.split_whitespace().nth(1) == Some("b") {
        LiveSide::Black
    } else {
        LiveSide::White
    }
}

fn finalize_logs(mut logs: Vec<GameLogEntry>, total_elapsed_ms: u64) -> Vec<GameLogEntry> {
    logs.push(GameLogEntry {
        timestamp_ms: total_elapsed_ms,
        level: "info".to_string(),
        source: "runner".to_string(),
        message: "game finished".to_string(),
    });
    logs
}

#[cfg(test)]
mod tests {
    use super::*;
    use arena_core::{AgentCapabilities, MatchStatus};
    use std::collections::BTreeMap;

    fn sample_version(executable_path: &str) -> AgentVersion {
        AgentVersion {
            id: Uuid::new_v4(),
            registry_key: None,
            agent_id: Uuid::new_v4(),
            version: "0.1.0".to_string(),
            active: true,
            executable_path: executable_path.to_string(),
            working_directory: None,
            args: Vec::new(),
            env: BTreeMap::new(),
            capabilities: AgentCapabilities {
                supports_chess960: true,
            },
            declared_name: Some("sample".to_string()),
            tags: vec!["test".to_string()],
            notes: None,
            documentation: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn pgn_contains_moves() {
        let request = MatchRequest {
            tournament_id: Uuid::new_v4(),
            match_series: MatchSeries {
                id: Uuid::new_v4(),
                tournament_id: Uuid::new_v4(),
                pool_id: Uuid::new_v4(),
                round_index: 0,
                white_version_id: Uuid::new_v4(),
                black_version_id: Uuid::new_v4(),
                opening_id: None,
                game_index: 0,
                status: MatchStatus::Pending,
                created_at: Utc::now(),
            },
            variant: Variant::Standard,
            white: sample_version("white"),
            black: sample_version("black"),
            opening: None,
            time_control: TimeControl {
                initial_ms: 1_000,
                increment_ms: 0,
            },
            max_plies: 200,
            opening_seed: Some(1),
            progress_sink: None,
        };
        let pgn = pgn_from_moves(
            &request.match_series.id.to_string(),
            request.variant,
            "start",
            &["e2e4".to_string(), "e7e5".to_string()],
            GameResult::Draw,
        );
        assert!(pgn.contains("1. e2e4 e7e5"));
    }
}
