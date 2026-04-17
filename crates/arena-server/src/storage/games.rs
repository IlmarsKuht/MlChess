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
pub(crate) async fn insert_game(db: &SqlitePool, game: &GameRecord) -> Result<()> {
    insert_game_with_executor(db, "games", game).await
}

pub(crate) async fn insert_game_tx(
    tx: &mut Transaction<'_, Sqlite>,
    game: &GameRecord,
) -> Result<()> {
    insert_game_with_executor(&mut **tx, "games", game).await
}

#[allow(dead_code)]
pub(crate) async fn insert_human_game(db: &SqlitePool, game: &GameRecord) -> Result<()> {
    insert_game_with_executor(db, "human_games", game).await
}

pub(crate) async fn insert_human_game_tx(
    tx: &mut Transaction<'_, Sqlite>,
    game: &GameRecord,
) -> Result<()> {
    insert_game_with_executor(&mut **tx, "human_games", game).await
}

async fn insert_game_with_executor<'e, E>(executor: E, table: &str, game: &GameRecord) -> Result<()>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query(
        &format!(
            "INSERT INTO {table} (
            id, tournament_id, match_id, pool_id, variant, opening_id, white_version_id, black_version_id, result, termination, start_fen, pgn, moves_uci, white_time_left_ms, black_time_left_ms, logs, started_at, completed_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ),
    )
    .bind(game.id.to_string())
    .bind(game.tournament_id.to_string())
    .bind(game.match_id.to_string())
    .bind(game.pool_id.to_string())
    .bind(encode_json(&game.variant)?)
    .bind(game.opening_id.map(|id| id.to_string()))
    .bind(game.white_version_id.to_string())
    .bind(game.black_version_id.to_string())
    .bind(encode_json(&game.result)?)
    .bind(encode_json(&game.termination)?)
    .bind(&game.start_fen)
    .bind(&game.pgn)
    .bind(encode_json(&game.moves_uci)?)
    .bind(game.white_time_left_ms as i64)
    .bind(game.black_time_left_ms as i64)
    .bind(encode_json(&game.logs)?)
    .bind(ts(game.started_at))
    .bind(ts(game.completed_at))
    .execute(executor)
    .await?;
    Ok(())
}

pub(crate) async fn list_games(
    db: &SqlitePool,
    tournament_id: Option<Uuid>,
    agent_version_id: Option<Uuid>,
) -> Result<Vec<GameRecord>> {
    let mut query = QueryBuilder::<Sqlite>::new("SELECT * FROM games");
    push_game_filters(&mut query, tournament_id, agent_version_id);
    query.push(" ORDER BY completed_at DESC");
    let rows = query.build().fetch_all(db).await?;

    let mut human_query = QueryBuilder::<Sqlite>::new("SELECT * FROM human_games");
    push_game_filters(&mut human_query, tournament_id, agent_version_id);
    human_query.push(" ORDER BY completed_at DESC");
    let human_rows = human_query.build().fetch_all(db).await?;

    let mut games = rows
        .into_iter()
        .chain(human_rows)
        .map(game_from_row)
        .collect::<Result<Vec<_>>>()?;
    games.sort_by(|left, right| right.completed_at.cmp(&left.completed_at));
    Ok(games)
}

fn push_game_filters(
    query: &mut QueryBuilder<'_, Sqlite>,
    tournament_id: Option<Uuid>,
    agent_version_id: Option<Uuid>,
) {
    let mut has_where = false;
    if let Some(tournament_id) = tournament_id {
        query.push(if has_where { " AND " } else { " WHERE " });
        has_where = true;
        query.push("tournament_id = ");
        query.push_bind(tournament_id.to_string());
    }
    if let Some(agent_version_id) = agent_version_id {
        query.push(if has_where { " AND " } else { " WHERE " });
        query.push("(white_version_id = ");
        query.push_bind(agent_version_id.to_string());
        query.push(" OR black_version_id = ");
        query.push_bind(agent_version_id.to_string());
        query.push(")");
    }
}

pub(crate) async fn get_game(db: &SqlitePool, id: Uuid) -> Result<GameRecord, ApiError> {
    let row = sqlx::query("SELECT * FROM games WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(db)
        .await?;
    if let Some(row) = row {
        return game_from_row(row).map_err(ApiError::Internal);
    }
    let row = sqlx::query("SELECT * FROM human_games WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(db)
        .await?;
    row.map(game_from_row)
        .transpose()?
        .ok_or_else(|| ApiError::NotFound(format!("game {id} not found")))
}

fn game_from_row(row: sqlx::sqlite::SqliteRow) -> Result<GameRecord> {
    Ok(GameRecord {
        id: Uuid::parse_str(&row.get::<String, _>("id"))?,
        tournament_id: Uuid::parse_str(&row.get::<String, _>("tournament_id"))?,
        match_id: Uuid::parse_str(&row.get::<String, _>("match_id"))?,
        pool_id: Uuid::parse_str(&row.get::<String, _>("pool_id"))?,
        variant: decode_json(&row.get::<String, _>("variant"))?,
        opening_id: row
            .get::<Option<String>, _>("opening_id")
            .map(|value| Uuid::parse_str(&value))
            .transpose()?,
        white_version_id: Uuid::parse_str(&row.get::<String, _>("white_version_id"))?,
        black_version_id: Uuid::parse_str(&row.get::<String, _>("black_version_id"))?,
        result: decode_json(&row.get::<String, _>("result"))?,
        termination: decode_json(&row.get::<String, _>("termination"))?,
        start_fen: row.get("start_fen"),
        pgn: row.get("pgn"),
        moves_uci: decode_json(&row.get::<String, _>("moves_uci"))?,
        white_time_left_ms: row.get::<i64, _>("white_time_left_ms") as u64,
        black_time_left_ms: row.get::<i64, _>("black_time_left_ms") as u64,
        logs: decode_json(&row.get::<String, _>("logs"))?,
        started_at: parse_ts(row.get("started_at"))?,
        completed_at: parse_ts(row.get("completed_at"))?,
    })
}
