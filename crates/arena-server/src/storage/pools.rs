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
pub(crate) async fn insert_pool(db: &SqlitePool, pool: &BenchmarkPool) -> Result<()> {
    sqlx::query(
        "INSERT INTO benchmark_pools (id, registry_key, name, description, variant, initial_ms, increment_ms, fairness, active, created_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(pool.id.to_string())
    .bind(&pool.registry_key)
    .bind(&pool.name)
    .bind(&pool.description)
    .bind(encode_json(&pool.variant)?)
    .bind(pool.time_control.initial_ms as i64)
    .bind(pool.time_control.increment_ms as i64)
    .bind(encode_json(&pool.fairness)?)
    .bind(if pool.active { 1 } else { 0 })
    .bind(ts(pool.created_at))
    .execute(db)
    .await?;
    Ok(())
}

pub(crate) async fn update_pool(db: &SqlitePool, pool: &BenchmarkPool) -> Result<()> {
    sqlx::query(
        "UPDATE benchmark_pools SET
            registry_key = ?, name = ?, description = ?, variant = ?, initial_ms = ?, increment_ms = ?, fairness = ?, active = ?
        WHERE id = ?",
    )
    .bind(&pool.registry_key)
    .bind(&pool.name)
    .bind(&pool.description)
    .bind(encode_json(&pool.variant)?)
    .bind(pool.time_control.initial_ms as i64)
    .bind(pool.time_control.increment_ms as i64)
    .bind(encode_json(&pool.fairness)?)
    .bind(if pool.active { 1 } else { 0 })
    .bind(pool.id.to_string())
    .execute(db)
    .await?;
    Ok(())
}

pub(crate) async fn list_pools(db: &SqlitePool) -> Result<Vec<BenchmarkPool>> {
    let rows = sqlx::query("SELECT * FROM benchmark_pools ORDER BY created_at DESC")
        .fetch_all(db)
        .await?;
    rows.into_iter().map(pool_from_row).collect()
}

pub(crate) async fn get_pool(db: &SqlitePool, id: Uuid) -> Result<BenchmarkPool, ApiError> {
    let row = sqlx::query("SELECT * FROM benchmark_pools WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(db)
        .await?;
    row.map(pool_from_row)
        .transpose()?
        .ok_or_else(|| ApiError::NotFound(format!("pool {id} not found")))
}

pub(crate) async fn ensure_pool_exists(db: &SqlitePool, id: Uuid) -> Result<(), ApiError> {
    get_pool(db, id).await.map(|_| ())
}

fn pool_from_row(row: sqlx::sqlite::SqliteRow) -> Result<BenchmarkPool> {
    Ok(BenchmarkPool {
        id: Uuid::parse_str(&row.get::<String, _>("id"))?,
        registry_key: row.get("registry_key"),
        name: row.get("name"),
        description: row.get("description"),
        variant: decode_json(&row.get::<String, _>("variant"))?,
        time_control: TimeControl {
            initial_ms: row.get::<i64, _>("initial_ms") as u64,
            increment_ms: row.get::<i64, _>("increment_ms") as u64,
        },
        fairness: decode_json(&row.get::<String, _>("fairness"))?,
        active: as_bool(row.get("active")),
        created_at: parse_ts(row.get("created_at"))?,
    })
}
