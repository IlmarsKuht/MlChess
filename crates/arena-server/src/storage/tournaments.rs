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
pub(crate) async fn insert_tournament(db: &SqlitePool, tournament: &Tournament) -> Result<()> {
    insert_tournament_with_executor(db, tournament).await
}

pub(crate) async fn insert_tournament_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tournament: &Tournament,
) -> Result<()> {
    insert_tournament_with_executor(&mut **tx, tournament).await
}

async fn insert_tournament_with_executor<'e, E>(executor: E, tournament: &Tournament) -> Result<()>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query(
        "INSERT INTO tournaments (
            id, name, kind, pool_id, participant_version_ids, worker_count, games_per_pairing, status, created_at, started_at, completed_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(tournament.id.to_string())
    .bind(&tournament.name)
    .bind(encode_json(&tournament.kind)?)
    .bind(tournament.pool_id.to_string())
    .bind(encode_json(&tournament.participant_version_ids)?)
    .bind(i64::from(tournament.worker_count))
    .bind(i64::from(tournament.games_per_pairing))
    .bind(encode_json(&tournament.status)?)
    .bind(ts(tournament.created_at))
    .bind(tournament.started_at.map(ts))
    .bind(tournament.completed_at.map(ts))
    .execute(executor)
    .await?;
    Ok(())
}

pub(crate) async fn list_tournaments(db: &SqlitePool) -> Result<Vec<Tournament>> {
    let rows = sqlx::query("SELECT * FROM tournaments ORDER BY created_at DESC")
        .fetch_all(db)
        .await?;
    rows.into_iter().map(tournament_from_row).collect()
}

pub(crate) async fn get_tournament(db: &SqlitePool, id: Uuid) -> Result<Tournament, ApiError> {
    let row = sqlx::query("SELECT * FROM tournaments WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(db)
        .await?;
    row.map(tournament_from_row)
        .transpose()?
        .ok_or_else(|| ApiError::NotFound(format!("tournament {id} not found")))
}

pub(crate) async fn update_tournament_status(
    db: &SqlitePool,
    tournament_id: Uuid,
    status: TournamentStatus,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<()> {
    update_tournament_status_with_executor(db, tournament_id, status, started_at, completed_at)
        .await
}

pub(crate) async fn update_tournament_status_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tournament_id: Uuid,
    status: TournamentStatus,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<()> {
    update_tournament_status_with_executor(
        &mut **tx,
        tournament_id,
        status,
        started_at,
        completed_at,
    )
    .await
}

fn tournament_from_row(row: sqlx::sqlite::SqliteRow) -> Result<Tournament> {
    Ok(Tournament {
        id: Uuid::parse_str(&row.get::<String, _>("id"))?,
        name: row.get("name"),
        kind: decode_json(&row.get::<String, _>("kind"))?,
        pool_id: Uuid::parse_str(&row.get::<String, _>("pool_id"))?,
        participant_version_ids: decode_json(&row.get::<String, _>("participant_version_ids"))?,
        worker_count: row.get::<i64, _>("worker_count") as u16,
        games_per_pairing: row.get::<i64, _>("games_per_pairing") as u16,
        status: decode_json(&row.get::<String, _>("status"))?,
        created_at: parse_ts(row.get("created_at"))?,
        started_at: row
            .get::<Option<String>, _>("started_at")
            .map(parse_ts)
            .transpose()?,
        completed_at: row
            .get::<Option<String>, _>("completed_at")
            .map(parse_ts)
            .transpose()?,
    })
}

async fn update_tournament_status_with_executor<'e, E>(
    executor: E,
    tournament_id: Uuid,
    status: TournamentStatus,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<()>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query(
        "UPDATE tournaments
         SET status = ?, started_at = COALESCE(?, started_at), completed_at = ?
         WHERE id = ?",
    )
    .bind(encode_json(&status)?)
    .bind(started_at.map(ts))
    .bind(completed_at.map(ts))
    .bind(tournament_id.to_string())
    .execute(executor)
    .await?;
    Ok(())
}
