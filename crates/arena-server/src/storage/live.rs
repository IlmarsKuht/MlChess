#![allow(unused_imports, dead_code)]

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use arena_core::*;
use chrono::Utc;
use sqlx::{Executor, QueryBuilder, Row, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::{
    ApiError,
    db::{as_bool, decode_json, encode_json, parse_ts, ts},
    match_runtime::types::HumanPlayer,
    presentation::HumanPlayerProfile,
    rating::default_entry,
    state::RequestJournalEntry,
};
pub(crate) async fn upsert_live_runtime_checkpoint(
    db: &SqlitePool,
    checkpoint: &LiveRuntimeCheckpoint,
) -> Result<()> {
    upsert_live_runtime_checkpoint_with_executor(db, checkpoint).await
}

pub(crate) async fn upsert_live_runtime_checkpoint_tx(
    tx: &mut Transaction<'_, Sqlite>,
    checkpoint: &LiveRuntimeCheckpoint,
) -> Result<()> {
    upsert_live_runtime_checkpoint_with_executor(&mut **tx, checkpoint).await
}

async fn upsert_live_runtime_checkpoint_with_executor<'e, E>(
    executor: E,
    checkpoint: &LiveRuntimeCheckpoint,
) -> Result<()>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query(
        "INSERT INTO live_runtime_checkpoints (
            match_id, seq, status, result, termination, start_fen, fen, moves, white_remaining_ms, black_remaining_ms, side_to_move, turn_started_server_unix_ms, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(match_id) DO UPDATE SET
            seq = excluded.seq,
            status = excluded.status,
            result = excluded.result,
            termination = excluded.termination,
            start_fen = excluded.start_fen,
            fen = excluded.fen,
            moves = excluded.moves,
            white_remaining_ms = excluded.white_remaining_ms,
            black_remaining_ms = excluded.black_remaining_ms,
            side_to_move = excluded.side_to_move,
            turn_started_server_unix_ms = excluded.turn_started_server_unix_ms,
            updated_at = excluded.updated_at",
    )
    .bind(checkpoint.match_id.to_string())
    .bind(checkpoint.seq as i64)
    .bind(encode_json(&checkpoint.status)?)
    .bind(encode_json(&checkpoint.result)?)
    .bind(encode_json(&checkpoint.termination)?)
    .bind(&checkpoint.start_fen)
    .bind(&checkpoint.fen)
    .bind(encode_json(&checkpoint.moves)?)
    .bind(checkpoint.white_remaining_ms as i64)
    .bind(checkpoint.black_remaining_ms as i64)
    .bind(encode_json(&checkpoint.side_to_move)?)
    .bind(checkpoint.turn_started_server_unix_ms)
    .bind(ts(checkpoint.updated_at))
    .execute(executor)
    .await?;
    Ok(())
}

pub(crate) async fn insert_live_runtime_event(
    db: &SqlitePool,
    event: &LiveEventEnvelope,
) -> Result<()> {
    insert_live_runtime_event_with_executor(db, event).await
}

pub(crate) async fn insert_live_runtime_event_tx(
    tx: &mut Transaction<'_, Sqlite>,
    event: &LiveEventEnvelope,
) -> Result<()> {
    insert_live_runtime_event_with_executor(&mut **tx, event).await
}

async fn insert_live_runtime_event_with_executor<'e, E>(
    executor: E,
    event: &LiveEventEnvelope,
) -> Result<()>
where
    E: Executor<'e, Database = Sqlite>,
{
    let (match_id, seq, event_type) = match event {
        LiveEventEnvelope::Snapshot(value) => (value.match_id, value.seq, LiveEventType::Snapshot),
        LiveEventEnvelope::MoveCommitted(value) => {
            (value.match_id, value.seq, LiveEventType::MoveCommitted)
        }
        LiveEventEnvelope::ClockSync(value) => {
            (value.match_id, value.seq, LiveEventType::ClockSync)
        }
        LiveEventEnvelope::GameFinished(value) => {
            (value.match_id, value.seq, LiveEventType::GameFinished)
        }
    };
    sqlx::query(
        "INSERT OR REPLACE INTO live_runtime_events (match_id, seq, event_type, payload, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(match_id.to_string())
    .bind(seq as i64)
    .bind(encode_json(&event_type)?)
    .bind(encode_json(event)?)
    .bind(ts(Utc::now()))
    .execute(executor)
    .await?;
    Ok(())
}

pub(crate) async fn load_live_runtime_checkpoint(
    db: &SqlitePool,
    match_id: Uuid,
) -> Result<Option<LiveRuntimeCheckpoint>> {
    let row = sqlx::query("SELECT * FROM live_runtime_checkpoints WHERE match_id = ?")
        .bind(match_id.to_string())
        .fetch_optional(db)
        .await?;
    row.map(live_runtime_checkpoint_from_row).transpose()
}

pub(crate) async fn list_live_runtime_checkpoints(
    db: &SqlitePool,
    status: Option<arena_core::LiveStatus>,
) -> Result<Vec<LiveRuntimeCheckpoint>> {
    let rows = if let Some(status) = status {
        sqlx::query(
            "SELECT * FROM live_runtime_checkpoints WHERE status = ? ORDER BY updated_at DESC",
        )
        .bind(encode_json(&status)?)
        .fetch_all(db)
        .await?
    } else {
        sqlx::query("SELECT * FROM live_runtime_checkpoints ORDER BY updated_at DESC")
            .fetch_all(db)
            .await?
    };
    rows.into_iter().map(live_runtime_checkpoint_from_row).collect()
}

pub(crate) async fn load_live_runtime_events_since(
    db: &SqlitePool,
    match_id: Uuid,
    seq: u64,
) -> Result<Vec<LiveEventEnvelope>> {
    let rows = sqlx::query(
        "SELECT payload FROM live_runtime_events WHERE match_id = ? AND seq > ? ORDER BY seq ASC",
    )
    .bind(match_id.to_string())
    .bind(seq as i64)
    .fetch_all(db)
    .await?;
    rows.into_iter()
        .map(|row| decode_json(&row.get::<String, _>("payload")))
        .collect()
}

fn live_runtime_checkpoint_from_row(row: sqlx::sqlite::SqliteRow) -> Result<LiveRuntimeCheckpoint> {
    Ok(LiveRuntimeCheckpoint {
        match_id: Uuid::parse_str(&row.get::<String, _>("match_id"))?,
        seq: row.get::<i64, _>("seq") as u64,
        status: decode_json(&row.get::<String, _>("status"))?,
        result: decode_json(&row.get::<String, _>("result"))?,
        termination: decode_json(&row.get::<String, _>("termination"))?,
        start_fen: row.get("start_fen"),
        fen: row.get("fen"),
        moves: decode_json(&row.get::<String, _>("moves"))?,
        white_remaining_ms: row.get::<i64, _>("white_remaining_ms") as u64,
        black_remaining_ms: row.get::<i64, _>("black_remaining_ms") as u64,
        side_to_move: decode_json(&row.get::<String, _>("side_to_move"))?,
        turn_started_server_unix_ms: row.get("turn_started_server_unix_ms"),
        updated_at: parse_ts(row.get("updated_at"))?,
    })
}
