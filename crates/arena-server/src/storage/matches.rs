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
pub(crate) async fn insert_match_series(db: &SqlitePool, series: &MatchSeries) -> Result<()> {
    insert_match_series_with_executor(db, series).await
}

pub(crate) async fn insert_match_series_tx(
    tx: &mut Transaction<'_, Sqlite>,
    series: &MatchSeries,
) -> Result<()> {
    insert_match_series_with_executor(&mut **tx, series).await
}

async fn insert_match_series_with_executor<'e, E>(executor: E, series: &MatchSeries) -> Result<()>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query(
        "INSERT INTO match_series (
            id, tournament_id, pool_id, round_index, white_version_id, black_version_id, opening_id, game_index, status, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(series.id.to_string())
    .bind(series.tournament_id.to_string())
    .bind(series.pool_id.to_string())
    .bind(series.round_index as i64)
    .bind(series.white_version_id.to_string())
    .bind(series.black_version_id.to_string())
    .bind(series.opening_id.map(|id| id.to_string()))
    .bind(series.game_index as i64)
    .bind(encode_json(&series.status)?)
    .bind(ts(series.created_at))
    .execute(executor)
    .await?;
    Ok(())
}

pub(crate) async fn list_match_series(
    db: &SqlitePool,
    tournament_id: Option<Uuid>,
) -> Result<Vec<MatchSeries>> {
    let rows = if let Some(tournament_id) = tournament_id {
        sqlx::query("SELECT * FROM match_series WHERE tournament_id = ? ORDER BY created_at DESC")
            .bind(tournament_id.to_string())
            .fetch_all(db)
            .await?
    } else {
        sqlx::query("SELECT * FROM match_series ORDER BY created_at DESC")
            .fetch_all(db)
            .await?
    };
    rows.into_iter().map(match_series_from_row).collect()
}

pub(crate) async fn get_match_series(db: &SqlitePool, id: Uuid) -> Result<MatchSeries, ApiError> {
    let row = sqlx::query("SELECT * FROM match_series WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(db)
        .await?;
    row.map(match_series_from_row)
        .transpose()?
        .ok_or_else(|| ApiError::NotFound(format!("match series {id} not found")))
}

pub(crate) async fn update_match_series_status(
    db: &SqlitePool,
    id: Uuid,
    status: MatchStatus,
) -> Result<()> {
    update_match_series_status_with_executor(db, id, status).await
}

pub(crate) async fn update_match_series_status_tx(
    tx: &mut Transaction<'_, Sqlite>,
    id: Uuid,
    status: MatchStatus,
) -> Result<()> {
    update_match_series_status_with_executor(&mut **tx, id, status).await
}

fn match_series_from_row(row: sqlx::sqlite::SqliteRow) -> Result<MatchSeries> {
    Ok(MatchSeries {
        id: Uuid::parse_str(&row.get::<String, _>("id"))?,
        tournament_id: Uuid::parse_str(&row.get::<String, _>("tournament_id"))?,
        pool_id: Uuid::parse_str(&row.get::<String, _>("pool_id"))?,
        round_index: row.get::<i64, _>("round_index") as u32,
        white_version_id: Uuid::parse_str(&row.get::<String, _>("white_version_id"))?,
        black_version_id: Uuid::parse_str(&row.get::<String, _>("black_version_id"))?,
        opening_id: row
            .get::<Option<String>, _>("opening_id")
            .map(|value| Uuid::parse_str(&value))
            .transpose()?,
        game_index: row.get::<i64, _>("game_index") as u32,
        status: decode_json(&row.get::<String, _>("status"))?,
        created_at: parse_ts(row.get("created_at"))?,
    })
}

async fn update_match_series_status_with_executor<'e, E>(
    executor: E,
    id: Uuid,
    status: MatchStatus,
) -> Result<()>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query("UPDATE match_series SET status = ? WHERE id = ?")
        .bind(encode_json(&status)?)
        .bind(id.to_string())
        .execute(executor)
        .await?;
    Ok(())
}