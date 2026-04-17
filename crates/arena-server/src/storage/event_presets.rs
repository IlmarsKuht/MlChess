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
pub(crate) async fn insert_event_preset(db: &SqlitePool, preset: &EventPreset) -> Result<()> {
    sqlx::query(
        "INSERT INTO event_presets (
            id, registry_key, name, kind, pool_id, selection_mode, worker_count, games_per_pairing, active, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(preset.id.to_string())
    .bind(&preset.registry_key)
    .bind(&preset.name)
    .bind(encode_json(&preset.kind)?)
    .bind(preset.pool_id.to_string())
    .bind(encode_json(&preset.selection_mode)?)
    .bind(i64::from(preset.worker_count))
    .bind(i64::from(preset.games_per_pairing))
    .bind(if preset.active { 1 } else { 0 })
    .bind(ts(preset.created_at))
    .execute(db)
    .await?;
    Ok(())
}

pub(crate) async fn update_event_preset(db: &SqlitePool, preset: &EventPreset) -> Result<()> {
    sqlx::query(
        "UPDATE event_presets SET
            registry_key = ?, name = ?, kind = ?, pool_id = ?, selection_mode = ?, worker_count = ?, games_per_pairing = ?, active = ?
        WHERE id = ?",
    )
    .bind(&preset.registry_key)
    .bind(&preset.name)
    .bind(encode_json(&preset.kind)?)
    .bind(preset.pool_id.to_string())
    .bind(encode_json(&preset.selection_mode)?)
    .bind(i64::from(preset.worker_count))
    .bind(i64::from(preset.games_per_pairing))
    .bind(if preset.active { 1 } else { 0 })
    .bind(preset.id.to_string())
    .execute(db)
    .await?;
    Ok(())
}

pub(crate) async fn list_event_presets(db: &SqlitePool) -> Result<Vec<EventPreset>> {
    let rows = sqlx::query("SELECT * FROM event_presets ORDER BY created_at DESC")
        .fetch_all(db)
        .await?;
    rows.into_iter().map(event_preset_from_row).collect()
}

pub(crate) async fn get_event_preset(db: &SqlitePool, id: Uuid) -> Result<EventPreset, ApiError> {
    let row = sqlx::query("SELECT * FROM event_presets WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(db)
        .await?;
    row.map(event_preset_from_row)
        .transpose()?
        .ok_or_else(|| ApiError::NotFound(format!("event preset {id} not found")))
}

fn event_preset_from_row(row: sqlx::sqlite::SqliteRow) -> Result<EventPreset> {
    Ok(EventPreset {
        id: Uuid::parse_str(&row.get::<String, _>("id"))?,
        registry_key: row.get("registry_key"),
        name: row.get("name"),
        kind: decode_json(&row.get::<String, _>("kind"))?,
        pool_id: Uuid::parse_str(&row.get::<String, _>("pool_id"))?,
        selection_mode: decode_json(&row.get::<String, _>("selection_mode"))?,
        worker_count: row.get::<i64, _>("worker_count") as u16,
        games_per_pairing: row.get::<i64, _>("games_per_pairing") as u16,
        active: as_bool(row.get("active")),
        created_at: parse_ts(row.get("created_at"))?,
    })
}
