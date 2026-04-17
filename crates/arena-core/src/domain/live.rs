use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    pub start_fen: String,
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
    pub start_fen: String,
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
    pub start_fen: String,
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
    pub start_fen: String,
    pub fen: String,
    pub moves: Vec<String>,
    pub white_remaining_ms: u64,
    pub black_remaining_ms: u64,
    pub side_to_move: ProtocolLiveSide,
    pub turn_started_server_unix_ms: i64,
    pub updated_at: DateTime<Utc>,
}