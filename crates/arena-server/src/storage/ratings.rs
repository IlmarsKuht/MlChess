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
    storage::list_agent_versions,
    state::RequestJournalEntry,
};
pub(crate) async fn insert_rating_snapshot(
    db: &SqlitePool,
    snapshot: &RatingSnapshot,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO rating_snapshots (
            id, pool_id, agent_version_id, rating, games_played, wins, draws, losses, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(snapshot.id.to_string())
    .bind(snapshot.pool_id.map(|id| id.to_string()))
    .bind(snapshot.agent_version_id.to_string())
    .bind(snapshot.rating)
    .bind(snapshot.games_played as i64)
    .bind(snapshot.wins as i64)
    .bind(snapshot.draws as i64)
    .bind(snapshot.losses as i64)
    .bind(ts(snapshot.created_at))
    .execute(db)
    .await?;
    Ok(())
}

pub(crate) async fn insert_human_rating_snapshot(
    db: &SqlitePool,
    snapshot: &RatingSnapshot,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO human_rating_snapshots (
            id, pool_id, human_player_id, rating, games_played, wins, draws, losses, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(snapshot.id.to_string())
    .bind(snapshot.pool_id.map(|id| id.to_string()))
    .bind(snapshot.agent_version_id.to_string())
    .bind(snapshot.rating)
    .bind(snapshot.games_played as i64)
    .bind(snapshot.wins as i64)
    .bind(snapshot.draws as i64)
    .bind(snapshot.losses as i64)
    .bind(ts(snapshot.created_at))
    .execute(db)
    .await?;
    Ok(())
}

pub(crate) async fn load_rating_history(
    db: &SqlitePool,
    pool_id: Option<Uuid>,
    agent_version_id: Option<Uuid>,
) -> Result<Vec<RatingSnapshot>> {
    let mut query = QueryBuilder::<Sqlite>::new("SELECT * FROM rating_snapshots");
    push_rating_filters(&mut query, pool_id, agent_version_id, "agent_version_id");
    query.push(" ORDER BY created_at ASC");
    let rows = query.build().fetch_all(db).await?;

    let mut human_query = QueryBuilder::<Sqlite>::new("SELECT * FROM human_rating_snapshots");
    push_rating_filters(&mut human_query, pool_id, agent_version_id, "human_player_id");
    human_query.push(" ORDER BY created_at ASC");
    let human_rows = human_query.build().fetch_all(db).await?;

    let mut snapshots = rows
        .into_iter()
        .chain(human_rows)
        .map(rating_snapshot_from_row)
        .collect::<Result<Vec<_>>>()?;
    snapshots.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    Ok(snapshots)
}

fn push_rating_filters(
    query: &mut QueryBuilder<'_, Sqlite>,
    pool_id: Option<Uuid>,
    participant_id: Option<Uuid>,
    participant_column: &'static str,
) {
    let mut has_where = false;
    if let Some(pool_id) = pool_id {
        query.push(if has_where { " AND " } else { " WHERE " });
        has_where = true;
        query.push("pool_id = ");
        query.push_bind(pool_id.to_string());
    }
    if let Some(participant_id) = participant_id {
        query.push(if has_where { " AND " } else { " WHERE " });
        query.push(participant_column);
        query.push(" = ");
        query.push_bind(participant_id.to_string());
    }
}

fn rating_snapshot_from_row(row: sqlx::sqlite::SqliteRow) -> Result<RatingSnapshot> {
    let participant_id = row
        .try_get::<String, _>("agent_version_id")
        .or_else(|_| row.try_get::<String, _>("human_player_id"))?;
    Ok(RatingSnapshot {
        id: Uuid::parse_str(&row.get::<String, _>("id"))?,
        pool_id: row
            .get::<Option<String>, _>("pool_id")
            .map(|value| Uuid::parse_str(&value))
            .transpose()?,
        agent_version_id: Uuid::parse_str(&participant_id)?,
        rating: row.get("rating"),
        games_played: row.get::<i64, _>("games_played") as u32,
        wins: row.get::<i64, _>("wins") as u32,
        draws: row.get::<i64, _>("draws") as u32,
        losses: row.get::<i64, _>("losses") as u32,
        created_at: parse_ts(row.get("created_at"))?,
    })
}

pub(crate) async fn ensure_leaderboard_seed(
    db: &SqlitePool,
    pool_id: Uuid,
    participants: &[Uuid],
) -> Result<HashMap<Uuid, LeaderboardEntry>> {
    let mut leaderboard: HashMap<Uuid, LeaderboardEntry> = load_pool_leaderboard(db, pool_id)
        .await?
        .into_iter()
        .map(|entry| (entry.agent_version_id, entry))
        .collect();
    for participant in participants {
        leaderboard
            .entry(*participant)
            .or_insert_with(|| default_entry(*participant));
    }
    Ok(leaderboard)
}

pub(crate) async fn load_pool_leaderboard(
    db: &SqlitePool,
    pool_id: Uuid,
) -> Result<Vec<LeaderboardEntry>> {
    let human_id = Uuid::from_u128(1);
    let mut entries_by_version: HashMap<Uuid, LeaderboardEntry> = list_agent_versions(db, None)
        .await?
        .into_iter()
        .map(|version| (version.id, default_entry(version.id)))
        .collect();
    entries_by_version.insert(human_id, default_entry(human_id));
    let snapshots = load_rating_history(db, Some(pool_id), None).await?;
    for snapshot in snapshots {
        entries_by_version.insert(
            snapshot.agent_version_id,
            LeaderboardEntry {
                agent_version_id: snapshot.agent_version_id,
                rating: snapshot.rating,
                games_played: snapshot.games_played,
                wins: snapshot.wins,
                draws: snapshot.draws,
                losses: snapshot.losses,
            },
        );
    }
    let mut entries: Vec<_> = entries_by_version.into_values().collect();
    entries.sort_by(|a, b| {
        b.rating
            .total_cmp(&a.rating)
            .then(b.games_played.cmp(&a.games_played))
            .then_with(|| a.agent_version_id.cmp(&b.agent_version_id))
    });
    Ok(entries)
}

pub(crate) async fn load_aggregate_leaderboard(db: &SqlitePool) -> Result<Vec<LeaderboardEntry>> {
    let all_versions = list_agent_versions(db, None).await?;
    let snapshots = load_rating_history(db, None, None).await?;
    let mut latest_by_pool_agent = HashMap::<(Option<Uuid>, Uuid), RatingSnapshot>::new();
    for snapshot in snapshots {
        latest_by_pool_agent.insert((snapshot.pool_id, snapshot.agent_version_id), snapshot);
    }

    let mut aggregate = HashMap::<Uuid, (f64, u32, u32, u32, u32, usize)>::new();
    for snapshot in latest_by_pool_agent.into_values() {
        if snapshot.pool_id.is_none() {
            continue;
        }
        let entry = aggregate
            .entry(snapshot.agent_version_id)
            .or_insert((0.0, 0, 0, 0, 0, 0));
        entry.0 += snapshot.rating;
        entry.1 += snapshot.games_played;
        entry.2 += snapshot.wins;
        entry.3 += snapshot.draws;
        entry.4 += snapshot.losses;
        entry.5 += 1;
    }

    let human_id = Uuid::from_u128(1);
    let mut entries: Vec<_> = all_versions
        .into_iter()
        .map(|version| match aggregate.get(&version.id) {
            Some((rating_sum, games_played, wins, draws, losses, count)) if *count > 0 => {
                LeaderboardEntry {
                    agent_version_id: version.id,
                    rating: rating_sum / *count as f64,
                    games_played: *games_played,
                    wins: *wins,
                    draws: *draws,
                    losses: *losses,
                }
            }
            _ => default_entry(version.id),
        })
        .collect();
    entries.push(match aggregate.get(&human_id) {
        Some((rating_sum, games_played, wins, draws, losses, count)) if *count > 0 => {
            LeaderboardEntry {
                agent_version_id: human_id,
                rating: rating_sum / *count as f64,
                games_played: *games_played,
                wins: *wins,
                draws: *draws,
                losses: *losses,
            }
        }
        _ => default_entry(human_id),
    });
    entries.sort_by(|a, b| {
        b.rating
            .total_cmp(&a.rating)
            .then(b.games_played.cmp(&a.games_played))
            .then_with(|| a.agent_version_id.cmp(&b.agent_version_id))
    });
    Ok(entries)
}

pub(crate) async fn ensure_human_player(db: &SqlitePool) -> Result<HumanPlayer, ApiError> {
    let human_id = Uuid::from_u128(1);
    if let Some(row) = sqlx::query("SELECT * FROM human_players WHERE id = ?")
        .bind(human_id.to_string())
        .fetch_optional(db)
        .await?
    {
        return Ok(HumanPlayer {
            id: Uuid::parse_str(&row.get::<String, _>("id"))
                .map_err(|err| ApiError::Internal(err.into()))?,
            name: row.get("name"),
            created_at: parse_ts(row.get("created_at"))?,
        });
    }

    let player = HumanPlayer {
        id: human_id,
        name: "You".to_string(),
        created_at: Utc::now(),
    };
    sqlx::query("INSERT INTO human_players (id, name, created_at) VALUES (?, ?, ?)")
        .bind(player.id.to_string())
        .bind(&player.name)
        .bind(ts(player.created_at))
        .execute(db)
        .await?;
    Ok(player)
}

pub(crate) async fn load_human_profile(
    db: &SqlitePool,
    player: &HumanPlayer,
) -> Result<HumanPlayerProfile, ApiError> {
    let snapshots = load_rating_history(db, None, Some(player.id)).await?;
    let entry = snapshots.last().cloned().map(|snapshot| LeaderboardEntry {
        agent_version_id: snapshot.agent_version_id,
        rating: snapshot.rating,
        games_played: snapshot.games_played,
        wins: snapshot.wins,
        draws: snapshot.draws,
        losses: snapshot.losses,
    });
    let current = entry.unwrap_or_else(|| default_entry(player.id));
    Ok(HumanPlayerProfile {
        id: player.id,
        name: player.name.clone(),
        rating: current.rating,
        games_played: current.games_played,
        wins: current.wins,
        draws: current.draws,
        losses: current.losses,
    })
}
