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
pub(crate) async fn insert_opening_suite(db: &SqlitePool, suite: &OpeningSuite) -> Result<()> {
    sqlx::query(
        "INSERT INTO opening_suites (id, registry_key, name, description, source_kind, source_text, active, starter, positions, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(suite.id.to_string())
    .bind(&suite.registry_key)
    .bind(&suite.name)
    .bind(&suite.description)
    .bind(encode_json(&suite.source_kind)?)
    .bind(&suite.source_text)
    .bind(if suite.active { 1 } else { 0 })
    .bind(if suite.starter { 1 } else { 0 })
    .bind(encode_json(&suite.positions)?)
    .bind(ts(suite.created_at))
    .execute(db)
    .await?;
    Ok(())
}

pub(crate) async fn update_opening_suite(db: &SqlitePool, suite: &OpeningSuite) -> Result<()> {
    sqlx::query(
        "UPDATE opening_suites SET
            registry_key = ?, name = ?, description = ?, source_kind = ?, source_text = ?, active = ?, starter = ?, positions = ?
        WHERE id = ?",
    )
    .bind(&suite.registry_key)
    .bind(&suite.name)
    .bind(&suite.description)
    .bind(encode_json(&suite.source_kind)?)
    .bind(&suite.source_text)
    .bind(if suite.active { 1 } else { 0 })
    .bind(if suite.starter { 1 } else { 0 })
    .bind(encode_json(&suite.positions)?)
    .bind(suite.id.to_string())
    .execute(db)
    .await?;
    Ok(())
}

pub(crate) async fn list_opening_suites(db: &SqlitePool) -> Result<Vec<OpeningSuite>> {
    let rows = sqlx::query("SELECT * FROM opening_suites ORDER BY created_at DESC")
        .fetch_all(db)
        .await?;
    rows.into_iter().map(opening_suite_from_row).collect()
}

pub(crate) async fn get_opening_suite(db: &SqlitePool, id: Uuid) -> Result<OpeningSuite, ApiError> {
    let row = sqlx::query("SELECT * FROM opening_suites WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(db)
        .await?;
    row.map(opening_suite_from_row)
        .transpose()?
        .ok_or_else(|| ApiError::NotFound(format!("opening suite {id} not found")))
}

fn opening_suite_from_row(row: sqlx::sqlite::SqliteRow) -> Result<OpeningSuite> {
    Ok(OpeningSuite {
        id: Uuid::parse_str(&row.get::<String, _>("id"))?,
        registry_key: row.get("registry_key"),
        name: row.get("name"),
        description: row.get("description"),
        source_kind: decode_json(&row.get::<String, _>("source_kind"))?,
        source_text: row.get("source_text"),
        active: as_bool(row.get("active")),
        starter: as_bool(row.get("starter")),
        positions: decode_json(&row.get::<String, _>("positions"))?,
        created_at: parse_ts(row.get("created_at"))?,
    })
}

pub(crate) async fn load_pool_openings(
    db: &SqlitePool,
    pool: &BenchmarkPool,
) -> Result<Vec<arena_core::OpeningPosition>> {
    let Some(opening_suite_id) = pool.fairness.opening_suite_id else {
        return Ok(Vec::new());
    };
    let suite = get_opening_suite(db, opening_suite_id)
        .await
        .map_err(|err| anyhow!(err.to_string()))?;
    Ok(suite.positions)
}
