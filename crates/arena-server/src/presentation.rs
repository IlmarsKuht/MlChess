use std::collections::HashMap;

use arena_core::{
    AgentVersion, GameRecord, GameResult, LeaderboardEntry, LiveRuntimeCheckpoint, LiveStatus,
    MatchSeries, MatchStatus, TournamentStatus, Variant,
};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::match_runtime::types::HumanPlayer;

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
    pub(crate) watch_state: ApiMatchWatchState,
    pub(crate) game_id: Option<Uuid>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) white_participant: ApiParticipant,
    pub(crate) black_participant: ApiParticipant,
    pub(crate) interactive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ApiMatchWatchState {
    Live,
    Replay,
    Unavailable,
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
    pub(crate) frames: Vec<String>,
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
    let base = version
        .declared_name
        .clone()
        .unwrap_or_else(|| version.agent_id.to_string());
    let archived_suffix = if version.active { "" } else { " (archived)" };
    format!("{base} {}{archived_suffix}", version.version)
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
    status: MatchStatus,
    watch_state: ApiMatchWatchState,
    game_id: Option<Uuid>,
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
        status,
        watch_state,
        game_id,
        created_at: series.created_at,
        white_participant: participant_for_id(
            series.white_version_id,
            version_name_by_id,
            human_player,
        ),
        black_participant: participant_for_id(
            series.black_version_id,
            version_name_by_id,
            human_player,
        ),
        interactive,
    }
}

pub(crate) fn resolve_match_lifecycle(
    series: &MatchSeries,
    tournament_status: TournamentStatus,
    game_id: Option<Uuid>,
    checkpoint: Option<&LiveRuntimeCheckpoint>,
) -> (MatchStatus, ApiMatchWatchState, Option<Uuid>) {
    if let Some(game_id) = game_id {
        return (
            MatchStatus::Completed,
            ApiMatchWatchState::Replay,
            Some(game_id),
        );
    }

    if let Some(checkpoint) = checkpoint {
        return match checkpoint.status {
            LiveStatus::Finished => (
                MatchStatus::Completed,
                if game_id.is_some() {
                    ApiMatchWatchState::Replay
                } else {
                    ApiMatchWatchState::Unavailable
                },
                game_id,
            ),
            LiveStatus::Aborted => (
                MatchStatus::Failed,
                ApiMatchWatchState::Unavailable,
                game_id,
            ),
            LiveStatus::Running => (MatchStatus::Running, ApiMatchWatchState::Live, game_id),
        };
    }

    if matches!(
        tournament_status,
        TournamentStatus::Completed | TournamentStatus::Failed | TournamentStatus::Stopped
    ) {
        let terminal_status = match series.status {
            MatchStatus::Pending => MatchStatus::Skipped,
            MatchStatus::Running => MatchStatus::Failed,
            status => status,
        };
        return (terminal_status, ApiMatchWatchState::Unavailable, game_id);
    }

    let watch_state = match series.status {
        MatchStatus::Completed => ApiMatchWatchState::Replay,
        MatchStatus::Running => ApiMatchWatchState::Unavailable,
        MatchStatus::Failed | MatchStatus::Skipped => ApiMatchWatchState::Unavailable,
        MatchStatus::Pending => ApiMatchWatchState::Unavailable,
    };
    (series.status, watch_state, game_id)
}

pub(crate) fn resolve_tournament_status(
    tournament: &arena_core::Tournament,
    match_statuses: &[MatchStatus],
    now: DateTime<Utc>,
) -> TournamentStatus {
    let tournament_status = tournament.status;
    if tournament_status == TournamentStatus::Draft || match_statuses.is_empty() {
        if tournament_status == TournamentStatus::Running
            && match_statuses.is_empty()
            && tournament.created_at <= now - Duration::seconds(30)
        {
            return TournamentStatus::Failed;
        }
        return tournament_status;
    }

    if match_statuses
        .iter()
        .any(|status| matches!(status, MatchStatus::Running | MatchStatus::Pending))
    {
        return tournament_status;
    }

    if tournament_status == TournamentStatus::Stopped {
        return TournamentStatus::Stopped;
    }

    if tournament_status == TournamentStatus::Failed
        || match_statuses
            .iter()
            .any(|status| *status == MatchStatus::Failed)
    {
        return TournamentStatus::Failed;
    }

    TournamentStatus::Completed
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
        white_participant: participant_for_id(
            game.white_version_id,
            version_name_by_id,
            human_player,
        ),
        black_participant: participant_for_id(
            game.black_version_id,
            version_name_by_id,
            human_player,
        ),
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
