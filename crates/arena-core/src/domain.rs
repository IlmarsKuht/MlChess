use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    Standard,
    Chess960,
}

impl Variant {
    pub fn is_chess960(self) -> bool {
        matches!(self, Self::Chess960)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentProtocol {
    Uci,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentKind {
    RoundRobin,
    Ladder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventPresetSelectionMode {
    AllActiveEngines,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TournamentStatus {
    Draft,
    Running,
    Completed,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameTermination {
    Checkmate,
    Stalemate,
    FiftyMoveRule,
    Repetition,
    InsufficientMaterial,
    Timeout,
    Resignation,
    IllegalMove,
    MoveLimit,
    EngineFailure,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameResult {
    WhiteWin,
    BlackWin,
    Draw,
}

impl GameResult {
    pub fn white_score(self) -> f64 {
        match self {
            Self::WhiteWin => 1.0,
            Self::BlackWin => 0.0,
            Self::Draw => 0.5,
        }
    }

    pub fn black_score(self) -> f64 {
        1.0 - self.white_score()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeControl {
    pub initial_ms: u64,
    pub increment_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AgentCapabilities {
    pub supports_chess960: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    pub id: Uuid,
    pub registry_key: Option<String>,
    pub name: String,
    pub protocol: AgentProtocol,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    pub documentation: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentVersion {
    pub id: Uuid,
    pub registry_key: Option<String>,
    pub agent_id: Uuid,
    pub version: String,
    pub active: bool,
    pub executable_path: String,
    pub working_directory: Option<String>,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub capabilities: AgentCapabilities,
    pub declared_name: Option<String>,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    pub documentation: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FairnessConfig {
    pub paired_games: bool,
    pub swap_colors: bool,
    pub opening_suite_id: Option<Uuid>,
    pub opening_seed: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkPool {
    pub id: Uuid,
    pub registry_key: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub variant: Variant,
    pub time_control: TimeControl,
    pub fairness: FairnessConfig,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkPoolKey {
    pub variant: Variant,
    pub initial_ms: u64,
    pub increment_ms: u64,
    pub opening_suite_id: Option<Uuid>,
    pub paired_games: bool,
    pub swap_colors: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventPreset {
    pub id: Uuid,
    pub registry_key: Option<String>,
    pub name: String,
    pub kind: TournamentKind,
    pub pool_id: Uuid,
    pub selection_mode: EventPresetSelectionMode,
    pub worker_count: u16,
    pub games_per_pairing: u16,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

impl From<&BenchmarkPool> for BenchmarkPoolKey {
    fn from(pool: &BenchmarkPool) -> Self {
        Self {
            variant: pool.variant,
            initial_ms: pool.time_control.initial_ms,
            increment_ms: pool.time_control.increment_ms,
            opening_suite_id: pool.fairness.opening_suite_id,
            paired_games: pool.fairness.paired_games,
            swap_colors: pool.fairness.swap_colors,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tournament {
    pub id: Uuid,
    pub name: String,
    pub kind: TournamentKind,
    pub pool_id: Uuid,
    pub participant_version_ids: Vec<Uuid>,
    pub worker_count: u16,
    pub games_per_pairing: u16,
    pub status: TournamentStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchSeries {
    pub id: Uuid,
    pub tournament_id: Uuid,
    pub pool_id: Uuid,
    pub round_index: u32,
    pub white_version_id: Uuid,
    pub black_version_id: Uuid,
    pub opening_id: Option<Uuid>,
    pub game_index: u32,
    pub status: MatchStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameLogEntry {
    pub timestamp_ms: u64,
    pub event: String,
    pub level: String,
    pub source: String,
    pub message: String,
    pub match_id: Option<Uuid>,
    pub tournament_id: Option<Uuid>,
    pub game_id: Option<Uuid>,
    pub seq: Option<u64>,
    pub move_uci: Option<String>,
    pub side: Option<ProtocolLiveSide>,
    pub white_remaining_ms: Option<u64>,
    pub black_remaining_ms: Option<u64>,
    pub fields: Option<Value>,
}

impl GameLogEntry {
    pub fn new(
        event: impl Into<String>,
        level: impl Into<String>,
        source: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            timestamp_ms: 0,
            event: event.into(),
            level: level.into(),
            source: source.into(),
            message: message.into(),
            match_id: None,
            tournament_id: None,
            game_id: None,
            seq: None,
            move_uci: None,
            side: None,
            white_remaining_ms: None,
            black_remaining_ms: None,
            fields: None,
        }
    }

    pub fn with_timestamp_ms(mut self, timestamp_ms: u64) -> Self {
        self.timestamp_ms = timestamp_ms;
        self
    }

    pub fn with_match_id(mut self, match_id: Uuid) -> Self {
        self.match_id = Some(match_id);
        self
    }

    pub fn with_tournament_id(mut self, tournament_id: Uuid) -> Self {
        self.tournament_id = Some(tournament_id);
        self
    }

    pub fn with_game_id(mut self, game_id: Uuid) -> Self {
        self.game_id = Some(game_id);
        self
    }

    pub fn with_seq(mut self, seq: u64) -> Self {
        self.seq = Some(seq);
        self
    }

    pub fn with_move_uci(mut self, move_uci: impl Into<String>) -> Self {
        self.move_uci = Some(move_uci.into());
        self
    }

    pub fn with_side(mut self, side: ProtocolLiveSide) -> Self {
        self.side = Some(side);
        self
    }

    pub fn with_clocks(mut self, white_remaining_ms: u64, black_remaining_ms: u64) -> Self {
        self.white_remaining_ms = Some(white_remaining_ms);
        self.black_remaining_ms = Some(black_remaining_ms);
        self
    }

    pub fn with_fields(mut self, fields: Value) -> Self {
        self.fields = Some(fields);
        self
    }

    pub fn with_field(mut self, key: &str, value: Value) -> Self {
        let mut map = self
            .fields
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();
        map.insert(key.to_string(), value);
        self.fields = Some(json!(map));
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameRecord {
    pub id: Uuid,
    pub tournament_id: Uuid,
    pub match_id: Uuid,
    pub pool_id: Uuid,
    pub variant: Variant,
    pub opening_id: Option<Uuid>,
    pub white_version_id: Uuid,
    pub black_version_id: Uuid,
    pub result: GameResult,
    pub termination: GameTermination,
    pub start_fen: String,
    pub pgn: String,
    pub moves_uci: Vec<String>,
    pub white_time_left_ms: u64,
    pub black_time_left_ms: u64,
    pub logs: Vec<GameLogEntry>,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveSide {
    White,
    Black,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveGameFrame {
    pub ply: u32,
    pub fen: String,
    pub move_uci: Option<String>,
    pub white_time_left_ms: u64,
    pub black_time_left_ms: u64,
    pub updated_at: DateTime<Utc>,
    pub side_to_move: LiveSide,
    pub status: MatchStatus,
    pub result: Option<GameResult>,
    pub termination: Option<GameTermination>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveGameState {
    pub match_id: Uuid,
    pub tournament_id: Uuid,
    pub pool_id: Uuid,
    pub variant: Variant,
    pub white_version_id: Uuid,
    pub black_version_id: Uuid,
    pub start_fen: String,
    pub current_fen: String,
    pub moves_uci: Vec<String>,
    pub white_time_left_ms: u64,
    pub black_time_left_ms: u64,
    pub status: MatchStatus,
    pub result: Option<GameResult>,
    pub termination: Option<GameTermination>,
    pub updated_at: DateTime<Utc>,
    pub live_frames: Vec<LiveGameFrame>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveStatus {
    Running,
    Finished,
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveResult {
    WhiteWin,
    BlackWin,
    Draw,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveTermination {
    Checkmate,
    Timeout,
    Resignation,
    Abort,
    Stalemate,
    Repetition,
    InsufficientMaterial,
    FiftyMoveRule,
    IllegalMove,
    MoveLimit,
    EngineFailure,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolLiveSide {
    White,
    Black,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveEventType {
    Snapshot,
    MoveCommitted,
    ClockSync,
    GameFinished,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveMatchSnapshot {
    pub protocol_version: u32,
    pub event_type: LiveEventType,
    pub match_id: Uuid,
    pub seq: u64,
    pub server_now_unix_ms: i64,
    pub status: LiveStatus,
    pub result: LiveResult,
    pub termination: LiveTermination,
    pub fen: String,
    pub moves: Vec<String>,
    pub white_remaining_ms: u64,
    pub black_remaining_ms: u64,
    pub side_to_move: ProtocolLiveSide,
    pub turn_started_server_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveCommittedEvent {
    pub protocol_version: u32,
    pub event_type: LiveEventType,
    pub match_id: Uuid,
    pub seq: u64,
    pub server_now_unix_ms: i64,
    pub status: LiveStatus,
    pub move_uci: String,
    pub fen: String,
    pub moves: Vec<String>,
    pub white_remaining_ms: u64,
    pub black_remaining_ms: u64,
    pub side_to_move: ProtocolLiveSide,
    pub turn_started_server_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockSyncEvent {
    pub protocol_version: u32,
    pub event_type: LiveEventType,
    pub match_id: Uuid,
    pub seq: u64,
    pub server_now_unix_ms: i64,
    pub status: LiveStatus,
    pub white_remaining_ms: u64,
    pub black_remaining_ms: u64,
    pub side_to_move: ProtocolLiveSide,
    pub turn_started_server_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameFinishedEvent {
    pub protocol_version: u32,
    pub event_type: LiveEventType,
    pub match_id: Uuid,
    pub seq: u64,
    pub server_now_unix_ms: i64,
    pub status: LiveStatus,
    pub result: LiveResult,
    pub termination: LiveTermination,
    pub fen: String,
    pub moves: Vec<String>,
    pub white_remaining_ms: u64,
    pub black_remaining_ms: u64,
    pub side_to_move: ProtocolLiveSide,
    pub turn_started_server_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LiveEventEnvelope {
    Snapshot(LiveMatchSnapshot),
    MoveCommitted(MoveCommittedEvent),
    ClockSync(ClockSyncEvent),
    GameFinished(GameFinishedEvent),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveRuntimeCheckpoint {
    pub match_id: Uuid,
    pub seq: u64,
    pub status: LiveStatus,
    pub result: LiveResult,
    pub termination: LiveTermination,
    pub fen: String,
    pub moves: Vec<String>,
    pub white_remaining_ms: u64,
    pub black_remaining_ms: u64,
    pub side_to_move: ProtocolLiveSide,
    pub turn_started_server_unix_ms: i64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RatingSnapshot {
    pub id: Uuid,
    pub pool_id: Option<Uuid>,
    pub agent_version_id: Uuid,
    pub rating: f64,
    pub games_played: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpeningSourceKind {
    Starter,
    FenList,
    PgnImport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpeningPosition {
    pub id: Uuid,
    pub suite_id: Uuid,
    pub label: String,
    pub fen: String,
    pub variant: Variant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpeningSuite {
    pub id: Uuid,
    pub registry_key: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub source_kind: OpeningSourceKind,
    pub source_text: Option<String>,
    pub active: bool,
    pub starter: bool,
    pub positions: Vec<OpeningPosition>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub agent_version_id: Uuid,
    pub rating: f64,
    pub games_played: u32,
    pub wins: u32,
    pub draws: u32,
    pub losses: u32,
}
