use anyhow::Result;
use arena_core::{AgentVersion, GameLogEntry, Variant};
use async_trait::async_trait;
use cozy_chess::Board;

use crate::uci::UciAgentAdapter;

/// Low-level engine adapter used by the server-owned match runtime.
///
/// Implementations own engine process/protocol concerns only. Match loops,
/// persistence, live publication, and terminal game handling belong in
/// `arena-server`.
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

/// Build the low-level adapter for an engine version.
///
/// This factory starts no authoritative match runtime; callers are responsible
/// for owning lifecycle and game semantics.
pub fn build_adapter(version: AgentVersion) -> Box<dyn AgentAdapter> {
    Box::new(UciAgentAdapter::new(version))
}
