use std::collections::HashMap;

use arena_core::{GameResult, MatchSeries, MatchStatus, TimeControl, Variant};
use arena_runner::AgentAdapter;
use chrono::{DateTime, Utc};
use cozy_chess::{Board, Color};
use uuid::Uuid;

#[derive(Clone)]
pub(crate) struct HumanGameHandle {
    pub(crate) command_tx: tokio::sync::mpsc::Sender<HumanGameCommand>,
}

pub(crate) enum HumanGameCommand {
    SubmitMove {
        intent_id: Uuid,
        client_action_id: Option<Uuid>,
        ws_connection_id: Option<Uuid>,
        move_uci: String,
        respond_to: tokio::sync::oneshot::Sender<HumanMoveAck>,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum HumanMoveAck {
    Accepted,
    RejectedIllegal,
    RejectedNotYourTurn,
    RejectedGameFinished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletedGameTable {
    Engine,
    Human,
}

#[derive(Clone)]
pub(crate) struct MatchSession {
    pub(crate) name: String,
    pub(crate) match_series: MatchSeries,
    pub(crate) completed_game_table: CompletedGameTable,
}

pub(crate) struct MatchRuntime {
    pub(crate) tournament_id: Uuid,
    pub(crate) variant: Variant,
    pub(crate) time_control: TimeControl,
    pub(crate) start_fen: String,
    pub(crate) current_fen: String,
    pub(crate) board: Board,
    pub(crate) repetitions: HashMap<u64, u8>,
    pub(crate) move_history: Vec<String>,
    pub(crate) white_time_left_ms: u64,
    pub(crate) black_time_left_ms: u64,
    pub(crate) max_plies: u16,
    pub(crate) white_seat: MatchSeatController,
    pub(crate) black_seat: MatchSeatController,
    pub(crate) logs: Vec<arena_core::GameLogEntry>,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) turn_started_server_unix_ms: i64,
    pub(crate) seq: u64,
    pub(crate) result: Option<GameResult>,
    pub(crate) termination: Option<arena_core::GameTermination>,
    pub(crate) status: MatchStatus,
}

pub(crate) enum MatchSeatController {
    Engine(EngineSeatController),
    Human(HumanSeatController),
}

pub(crate) struct EngineSeatController {
    pub(crate) adapter: Option<Box<dyn AgentAdapter>>,
}

pub(crate) struct HumanSeatController {
    pub(crate) player: HumanPlayer,
    pub(crate) command_rx: tokio::sync::mpsc::Receiver<HumanGameCommand>,
    pub(crate) seen_intents: HashMap<Uuid, HumanMoveAck>,
}

impl MatchRuntime {
    pub(crate) fn active_side(&self) -> Color {
        self.board.side_to_move()
    }

    pub(crate) fn has_human_seat(&self) -> bool {
        matches!(self.white_seat, MatchSeatController::Human(_))
            || matches!(self.black_seat, MatchSeatController::Human(_))
    }

    pub(crate) fn active_seat(&self) -> &MatchSeatController {
        if self.active_side() == Color::White {
            &self.white_seat
        } else {
            &self.black_seat
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HumanPlayer {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) created_at: DateTime<Utc>,
}
