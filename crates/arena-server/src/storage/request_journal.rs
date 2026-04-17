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
pub(crate) async fn insert_request_journal_entry(
    db: &SqlitePool,
    entry: &RequestJournalEntry,
) -> Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO request_journal (
            request_id, client_action_id, client_route, client_ts, method, route, status_code, match_id, tournament_id, game_id, started_at, completed_at, duration_ms, error_text
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entry.request_id.to_string())
    .bind(entry.client_action_id.map(|value| value.to_string()))
    .bind(&entry.client_route)
    .bind(&entry.client_ts)
    .bind(&entry.method)
    .bind(&entry.route)
    .bind(i64::from(entry.status_code))
    .bind(entry.match_id.map(|value| value.to_string()))
    .bind(entry.tournament_id.map(|value| value.to_string()))
    .bind(entry.game_id.map(|value| value.to_string()))
    .bind(ts(entry.started_at))
    .bind(ts(entry.completed_at))
    .bind(entry.duration_ms)
    .bind(&entry.error_text)
    .execute(db)
    .await?;
    Ok(())
}

pub(crate) async fn get_request_journal_entry(
    db: &SqlitePool,
    request_id: Uuid,
) -> Result<Option<RequestJournalEntry>> {
    let row = sqlx::query("SELECT * FROM request_journal WHERE request_id = ?")
        .bind(request_id.to_string())
        .fetch_optional(db)
        .await?;
    row.map(request_journal_from_row).transpose()
}

pub(crate) async fn list_request_journal_for_entities(
    db: &SqlitePool,
    match_id: Option<Uuid>,
    tournament_id: Option<Uuid>,
    game_id: Option<Uuid>,
    limit: usize,
) -> Result<Vec<RequestJournalEntry>> {
    if match_id.is_none() && tournament_id.is_none() && game_id.is_none() {
        return Ok(Vec::new());
    }
    let mut query = QueryBuilder::<Sqlite>::new("SELECT * FROM request_journal WHERE ");
    let mut separated = query.separated(" OR ");
    if let Some(match_id) = match_id {
        separated.push("match_id = ");
        separated.push_bind_unseparated(match_id.to_string());
    }
    if let Some(tournament_id) = tournament_id {
        separated.push("tournament_id = ");
        separated.push_bind_unseparated(tournament_id.to_string());
    }
    if let Some(game_id) = game_id {
        separated.push("game_id = ");
        separated.push_bind_unseparated(game_id.to_string());
    }
    query.push(" ORDER BY completed_at DESC LIMIT ");
    query.push_bind(limit as i64);
    let rows = query.build().fetch_all(db).await?;
    rows.into_iter().map(request_journal_from_row).collect()
}

pub(crate) async fn list_recent_request_errors(
    db: &SqlitePool,
    limit: usize,
) -> Result<Vec<RequestJournalEntry>> {
    let rows = sqlx::query(
        "SELECT * FROM request_journal
         WHERE status_code >= 400 OR error_text IS NOT NULL
         ORDER BY completed_at DESC
         LIMIT ?",
    )
    .bind(limit as i64)
    .fetch_all(db)
    .await?;
    rows.into_iter().map(request_journal_from_row).collect()
}

fn request_journal_from_row(row: sqlx::sqlite::SqliteRow) -> Result<RequestJournalEntry> {
    Ok(RequestJournalEntry {
        request_id: Uuid::parse_str(&row.get::<String, _>("request_id"))?,
        client_action_id: row
            .get::<Option<String>, _>("client_action_id")
            .map(|value| Uuid::parse_str(&value))
            .transpose()?,
        client_route: row.get("client_route"),
        client_ts: row.get("client_ts"),
        method: row.get("method"),
        route: row.get("route"),
        status_code: row.get::<i64, _>("status_code") as u16,
        match_id: row
            .get::<Option<String>, _>("match_id")
            .map(|value| Uuid::parse_str(&value))
            .transpose()?,
        tournament_id: row
            .get::<Option<String>, _>("tournament_id")
            .map(|value| Uuid::parse_str(&value))
            .transpose()?,
        game_id: row
            .get::<Option<String>, _>("game_id")
            .map(|value| Uuid::parse_str(&value))
            .transpose()?,
        started_at: parse_ts(row.get("started_at"))?,
        completed_at: parse_ts(row.get("completed_at"))?,
        duration_ms: row.get("duration_ms"),
        error_text: row.get("error_text"),
    })
}