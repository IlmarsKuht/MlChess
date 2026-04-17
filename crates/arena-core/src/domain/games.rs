use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{MatchStatus, ProtocolLiveSide, Variant};

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