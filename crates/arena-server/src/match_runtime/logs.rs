use arena_core::GameLogEntry;

use super::types::{CompletedGameTable, MatchRuntime, MatchSession};

pub(crate) fn push_runtime_log(logs: &mut Vec<GameLogEntry>, entry: GameLogEntry) {
    logs.push(entry);
}

pub(crate) fn runtime_log(
    runtime: &MatchRuntime,
    source: &str,
    event: &str,
    message: impl Into<String>,
) -> GameLogEntry {
    GameLogEntry::new(event, "info", source, message.into())
        .with_tournament_id(runtime.tournament_id)
        .with_seq(runtime.seq)
        .with_clocks(runtime.white_time_left_ms, runtime.black_time_left_ms)
}

pub(crate) fn match_runtime_log(
    session: &MatchSession,
    runtime: &MatchRuntime,
    source: &str,
    event: &str,
    message: impl Into<String>,
) -> GameLogEntry {
    runtime_log(runtime, source, event, message).with_match_id(session.match_series.id)
}

pub(crate) fn human_runtime_log(
    session: &MatchSession,
    runtime: &MatchRuntime,
    event: &str,
    message: impl Into<String>,
) -> GameLogEntry {
    match_runtime_log(session, runtime, "server.human_runtime", event, message)
}

pub(crate) fn match_runtime_source(session: &MatchSession) -> &'static str {
    match session.completed_game_table {
        CompletedGameTable::Engine => "server.engine_runtime",
        CompletedGameTable::Human => "server.human_runtime",
    }
}
