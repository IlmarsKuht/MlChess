use std::collections::HashMap;

use arena_core::{
    AgentVersion, GameRecord, GameResult, LeaderboardEntry, LiveGameFrame, LiveGameState,
    LiveSide, MatchSeries, MatchStatus, Variant,
};
use axum::response::sse::Event;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::state::HumanPlayer;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ApiParticipantKind {
    EngineVersion,
    HumanPlayer,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ApiParticipant {
    pub(crate) kind: ApiParticipantKind,
    pub(crate) id: Uuid,
    pub(crate) display_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ApiMatchSeries {
    pub(crate) id: Uuid,
    pub(crate) tournament_id: Uuid,
    pub(crate) pool_id: Uuid,
    pub(crate) round_index: u32,
    pub(crate) white_version_id: Uuid,
    pub(crate) black_version_id: Uuid,
    pub(crate) opening_id: Option<Uuid>,
    pub(crate) game_index: u32,
    pub(crate) status: MatchStatus,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) white_participant: ApiParticipant,
    pub(crate) black_participant: ApiParticipant,
    pub(crate) interactive: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ApiGameRecord {
    pub(crate) id: Uuid,
    pub(crate) tournament_id: Uuid,
    pub(crate) match_id: Uuid,
    pub(crate) pool_id: Uuid,
    pub(crate) variant: Variant,
    pub(crate) opening_id: Option<Uuid>,
    pub(crate) white_version_id: Uuid,
    pub(crate) black_version_id: Uuid,
    pub(crate) result: GameResult,
    pub(crate) termination: arena_core::GameTermination,
    pub(crate) start_fen: String,
    pub(crate) pgn: String,
    pub(crate) moves_uci: Vec<String>,
    pub(crate) white_time_left_ms: u64,
    pub(crate) black_time_left_ms: u64,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) completed_at: DateTime<Utc>,
    pub(crate) white_participant: ApiParticipant,
    pub(crate) black_participant: ApiParticipant,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ApiLiveGameFrame {
    pub(crate) ply: u32,
    pub(crate) fen: String,
    pub(crate) move_uci: Option<String>,
    pub(crate) white_time_left_ms: u64,
    pub(crate) black_time_left_ms: u64,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) side_to_move: LiveSide,
    pub(crate) status: MatchStatus,
    pub(crate) result: Option<GameResult>,
    pub(crate) termination: Option<arena_core::GameTermination>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ApiLiveGameState {
    pub(crate) match_id: Uuid,
    pub(crate) tournament_id: Uuid,
    pub(crate) pool_id: Uuid,
    pub(crate) variant: Variant,
    pub(crate) white_version_id: Uuid,
    pub(crate) black_version_id: Uuid,
    pub(crate) start_fen: String,
    pub(crate) current_fen: String,
    pub(crate) moves_uci: Vec<String>,
    pub(crate) white_time_left_ms: u64,
    pub(crate) black_time_left_ms: u64,
    pub(crate) status: MatchStatus,
    pub(crate) result: Option<GameResult>,
    pub(crate) termination: Option<arena_core::GameTermination>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) live_frames: Vec<ApiLiveGameFrame>,
    pub(crate) white_participant: ApiParticipant,
    pub(crate) black_participant: ApiParticipant,
    pub(crate) interactive: bool,
    pub(crate) human_turn: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ApiLeaderboardEntry {
    pub(crate) participant: ApiParticipant,
    pub(crate) rating: f64,
    pub(crate) games_played: u32,
    pub(crate) wins: u32,
    pub(crate) draws: u32,
    pub(crate) losses: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HumanPlayerProfile {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) rating: f64,
    pub(crate) games_played: u32,
    pub(crate) wins: u32,
    pub(crate) draws: u32,
    pub(crate) losses: u32,
}

#[derive(Debug, Serialize)]
pub(crate) struct ReplayPayload {
    pub(crate) id: Uuid,
    pub(crate) variant: Variant,
    pub(crate) start_fen: String,
    pub(crate) pgn: String,
    pub(crate) moves_uci: Vec<String>,
    pub(crate) result: GameResult,
    pub(crate) termination: arena_core::GameTermination,
}

pub(crate) fn version_name_by_id(versions: &[AgentVersion]) -> HashMap<Uuid, String> {
    versions
        .iter()
        .map(|version| (version.id, display_version_name(version)))
        .collect()
}

fn display_version_name(version: &AgentVersion) -> String {
    version
        .declared_name
        .clone()
        .unwrap_or_else(|| format!("{} {}", version.agent_id, version.version))
}

pub(crate) fn participant_for_id(
    id: Uuid,
    version_name_by_id: &HashMap<Uuid, String>,
    human_player: &HumanPlayer,
) -> ApiParticipant {
    if id == human_player.id {
        ApiParticipant {
            kind: ApiParticipantKind::HumanPlayer,
            id,
            display_name: human_player.name.clone(),
        }
    } else {
        ApiParticipant {
            kind: ApiParticipantKind::EngineVersion,
            id,
            display_name: version_name_by_id
                .get(&id)
                .cloned()
                .unwrap_or_else(|| "Unknown engine".to_string()),
        }
    }
}

pub(crate) fn api_match_series(
    series: &MatchSeries,
    version_name_by_id: &HashMap<Uuid, String>,
    human_player: &HumanPlayer,
    interactive: bool,
) -> ApiMatchSeries {
    ApiMatchSeries {
        id: series.id,
        tournament_id: series.tournament_id,
        pool_id: series.pool_id,
        round_index: series.round_index,
        white_version_id: series.white_version_id,
        black_version_id: series.black_version_id,
        opening_id: series.opening_id,
        game_index: series.game_index,
        status: series.status,
        created_at: series.created_at,
        white_participant: participant_for_id(series.white_version_id, version_name_by_id, human_player),
        black_participant: participant_for_id(series.black_version_id, version_name_by_id, human_player),
        interactive,
    }
}

pub(crate) fn api_game_record(
    game: &GameRecord,
    version_name_by_id: &HashMap<Uuid, String>,
    human_player: &HumanPlayer,
) -> ApiGameRecord {
    ApiGameRecord {
        id: game.id,
        tournament_id: game.tournament_id,
        match_id: game.match_id,
        pool_id: game.pool_id,
        variant: game.variant,
        opening_id: game.opening_id,
        white_version_id: game.white_version_id,
        black_version_id: game.black_version_id,
        result: game.result,
        termination: game.termination,
        start_fen: game.start_fen.clone(),
        pgn: game.pgn.clone(),
        moves_uci: game.moves_uci.clone(),
        white_time_left_ms: game.white_time_left_ms,
        black_time_left_ms: game.black_time_left_ms,
        started_at: game.started_at,
        completed_at: game.completed_at,
        white_participant: participant_for_id(game.white_version_id, version_name_by_id, human_player),
        black_participant: participant_for_id(game.black_version_id, version_name_by_id, human_player),
    }
}

pub(crate) fn api_live_game_state(
    live_state: &LiveGameState,
    version_name_by_id: &HashMap<Uuid, String>,
    human_player: &HumanPlayer,
    interactive: bool,
) -> ApiLiveGameState {
    let human_turn = live_state.status == MatchStatus::Running
        && ((live_state.white_version_id == human_player.id && side_to_move(&live_state.current_fen) == cozy_chess::Color::White)
            || (live_state.black_version_id == human_player.id
                && side_to_move(&live_state.current_fen) == cozy_chess::Color::Black));
    ApiLiveGameState {
        match_id: live_state.match_id,
        tournament_id: live_state.tournament_id,
        pool_id: live_state.pool_id,
        variant: live_state.variant,
        white_version_id: live_state.white_version_id,
        black_version_id: live_state.black_version_id,
        start_fen: live_state.start_fen.clone(),
        current_fen: live_state.current_fen.clone(),
        moves_uci: live_state.moves_uci.clone(),
        white_time_left_ms: live_state.white_time_left_ms,
        black_time_left_ms: live_state.black_time_left_ms,
        status: live_state.status,
        result: live_state.result,
        termination: live_state.termination,
        updated_at: live_state.updated_at,
        live_frames: live_state.live_frames.iter().map(api_live_game_frame).collect(),
        white_participant: participant_for_id(live_state.white_version_id, version_name_by_id, human_player),
        black_participant: participant_for_id(live_state.black_version_id, version_name_by_id, human_player),
        interactive,
        human_turn,
    }
}

fn api_live_game_frame(frame: &LiveGameFrame) -> ApiLiveGameFrame {
    ApiLiveGameFrame {
        ply: frame.ply,
        fen: frame.fen.clone(),
        move_uci: frame.move_uci.clone(),
        white_time_left_ms: frame.white_time_left_ms,
        black_time_left_ms: frame.black_time_left_ms,
        updated_at: frame.updated_at,
        side_to_move: frame.side_to_move.clone(),
        status: frame.status,
        result: frame.result,
        termination: frame.termination,
    }
}

pub(crate) fn api_leaderboard_entry(
    entry: LeaderboardEntry,
    version_name_by_id: &HashMap<Uuid, String>,
    human_player: &HumanPlayer,
) -> ApiLeaderboardEntry {
    ApiLeaderboardEntry {
        participant: participant_for_id(entry.agent_version_id, version_name_by_id, human_player),
        rating: entry.rating,
        games_played: entry.games_played,
        wins: entry.wins,
        draws: entry.draws,
        losses: entry.losses,
    }
}

pub(crate) fn live_game_event<T: Serialize>(state: &T) -> Event {
    Event::default()
        .event("live_game")
        .json_data(state)
        .expect("live game state should serialize to JSON")
}

pub(crate) fn is_terminal_live_status(status: MatchStatus) -> bool {
    matches!(
        status,
        MatchStatus::Completed | MatchStatus::Failed | MatchStatus::Skipped
    )
}

fn side_to_move(fen: &str) -> cozy_chess::Color {
    fen.split_whitespace()
        .nth(1)
        .map(|token| {
            if token == "b" {
                cozy_chess::Color::Black
            } else {
                cozy_chess::Color::White
            }
        })
        .unwrap_or(cozy_chess::Color::White)
}
