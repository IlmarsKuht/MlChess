use std::collections::HashMap;

use anyhow::{Result, anyhow};
use arena_core::{
    Agent, AgentVersion, BenchmarkPool, EventPreset, GameRecord, LeaderboardEntry,
    LiveEventEnvelope, LiveEventType, LiveRuntimeCheckpoint, MatchSeries, MatchStatus,
    OpeningSuite, RatingSnapshot, TimeControl, Tournament, TournamentStatus,
};
use chrono::Utc;
use sqlx::{Executor, Row, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::{
    ApiError,
    db::{as_bool, decode_json, encode_json, parse_ts, ts},
    match_runtime::types::HumanPlayer,
    presentation::HumanPlayerProfile,
    rating::default_entry,
    state::RequestJournalEntry,
};

pub(crate) async fn insert_agent(db: &SqlitePool, agent: &Agent) -> Result<()> {
    sqlx::query(
        "INSERT INTO agents (id, registry_key, name, protocol, tags, notes, documentation, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
        .bind(agent.id.to_string())
        .bind(&agent.registry_key)
        .bind(&agent.name)
        .bind(encode_json(&agent.protocol)?)
        .bind(encode_json(&agent.tags)?)
        .bind(&agent.notes)
        .bind(&agent.documentation)
        .bind(ts(agent.created_at))
        .execute(db)
        .await?;
    Ok(())
}

pub(crate) async fn update_agent(db: &SqlitePool, agent: &Agent) -> Result<()> {
    sqlx::query(
        "UPDATE agents SET registry_key = ?, name = ?, protocol = ?, tags = ?, notes = ?, documentation = ? WHERE id = ?",
    )
    .bind(&agent.registry_key)
    .bind(&agent.name)
    .bind(encode_json(&agent.protocol)?)
    .bind(encode_json(&agent.tags)?)
    .bind(&agent.notes)
    .bind(&agent.documentation)
    .bind(agent.id.to_string())
    .execute(db)
    .await?;
    Ok(())
}

pub(crate) async fn list_agents(db: &SqlitePool) -> Result<Vec<Agent>> {
    let rows = sqlx::query("SELECT * FROM agents ORDER BY created_at DESC")
        .fetch_all(db)
        .await?;
    rows.into_iter().map(agent_from_row).collect()
}

pub(crate) async fn get_agent(db: &SqlitePool, id: Uuid) -> Result<Agent, ApiError> {
    let row = sqlx::query("SELECT * FROM agents WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(db)
        .await?;
    row.map(agent_from_row)
        .transpose()?
        .ok_or_else(|| ApiError::NotFound(format!("agent {id} not found")))
}

fn agent_from_row(row: sqlx::sqlite::SqliteRow) -> Result<Agent> {
    Ok(Agent {
        id: Uuid::parse_str(&row.get::<String, _>("id"))?,
        registry_key: row.get("registry_key"),
        name: row.get("name"),
        protocol: decode_json(&row.get::<String, _>("protocol"))?,
        tags: decode_json(&row.get::<String, _>("tags"))?,
        notes: row.get("notes"),
        documentation: row.get("documentation"),
        created_at: parse_ts(row.get("created_at"))?,
    })
}

pub(crate) async fn insert_agent_version(db: &SqlitePool, version: &AgentVersion) -> Result<()> {
    sqlx::query(
        "INSERT INTO agent_versions (
            id, registry_key, agent_id, version, active, executable_path, working_directory, args, env, capabilities, declared_name, tags, notes, documentation, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(version.id.to_string())
    .bind(&version.registry_key)
    .bind(version.agent_id.to_string())
    .bind(&version.version)
    .bind(if version.active { 1 } else { 0 })
    .bind(&version.executable_path)
    .bind(&version.working_directory)
    .bind(encode_json(&version.args)?)
    .bind(encode_json(&version.env)?)
    .bind(encode_json(&version.capabilities)?)
    .bind(&version.declared_name)
    .bind(encode_json(&version.tags)?)
    .bind(&version.notes)
    .bind(&version.documentation)
    .bind(ts(version.created_at))
    .execute(db)
    .await?;
    Ok(())
}

pub(crate) async fn update_agent_version(db: &SqlitePool, version: &AgentVersion) -> Result<()> {
    sqlx::query(
        "UPDATE agent_versions SET
            registry_key = ?, agent_id = ?, version = ?, active = ?, executable_path = ?, working_directory = ?, args = ?, env = ?, capabilities = ?, declared_name = ?, tags = ?, notes = ?, documentation = ?
        WHERE id = ?",
    )
    .bind(&version.registry_key)
    .bind(version.agent_id.to_string())
    .bind(&version.version)
    .bind(if version.active { 1 } else { 0 })
    .bind(&version.executable_path)
    .bind(&version.working_directory)
    .bind(encode_json(&version.args)?)
    .bind(encode_json(&version.env)?)
    .bind(encode_json(&version.capabilities)?)
    .bind(&version.declared_name)
    .bind(encode_json(&version.tags)?)
    .bind(&version.notes)
    .bind(&version.documentation)
    .bind(version.id.to_string())
    .execute(db)
    .await?;
    Ok(())
}

pub(crate) async fn list_agent_versions(
    db: &SqlitePool,
    agent_id: Option<Uuid>,
) -> Result<Vec<AgentVersion>> {
    let rows = if let Some(agent_id) = agent_id {
        sqlx::query("SELECT * FROM agent_versions WHERE agent_id = ? ORDER BY created_at DESC")
            .bind(agent_id.to_string())
            .fetch_all(db)
            .await?
    } else {
        sqlx::query("SELECT * FROM agent_versions ORDER BY created_at DESC")
            .fetch_all(db)
            .await?
    };
    rows.into_iter().map(agent_version_from_row).collect()
}

pub(crate) async fn list_agent_versions_by_ids(
    db: &SqlitePool,
    ids: &[Uuid],
) -> Result<Vec<AgentVersion>> {
    let versions = list_agent_versions(db, None).await?;
    Ok(versions
        .into_iter()
        .filter(|version| ids.contains(&version.id))
        .collect())
}

pub(crate) async fn get_agent_version(db: &SqlitePool, id: Uuid) -> Result<AgentVersion, ApiError> {
    let row = sqlx::query("SELECT * FROM agent_versions WHERE id = ?")
        .bind(id.to_string())
        .fetch_optional(db)
        .await?;
    row.map(agent_version_from_row)
        .transpose()?
        .ok_or_else(|| ApiError::NotFound(format!("agent version {id} not found")))
}

pub(crate) async fn ensure_agent_version_exists(db: &SqlitePool, id: Uuid) -> Result<(), ApiError> {
    get_agent_version(db, id).await.map(|_| ())
}

fn agent_version_from_row(row: sqlx::sqlite::SqliteRow) -> Result<AgentVersion> {
    Ok(AgentVersion {
        id: Uuid::parse_str(&row.get::<String, _>("id"))?,
        registry_key: row.get("registry_key"),
        agent_id: Uuid::parse_str(&row.get::<String, _>("agent_id"))?,
        version: row.get("version"),
        active: as_bool(row.get("active")),
        executable_path: row.get("executable_path"),
        working_directory: row.get("working_directory"),
        args: decode_json(&row.get::<String, _>("args"))?,
        env: decode_json(&row.get::<String, _>("env"))?,
        capabilities: decode_json(&row.get::<String, _>("capabilities"))?,
        declared_name: row.get("declared_name"),
        tags: decode_json(&row.get::<String, _>("tags"))?,
        notes: row.get("notes"),
        documentation: row.get("documentation"),
        created_at: parse_ts(row.get("created_at"))?,
    })
}

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

#[allow(dead_code)]
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
    let rows = sqlx::query("SELECT * FROM games ORDER BY completed_at DESC")
        .fetch_all(db)
        .await?;
    let human_rows = sqlx::query("SELECT * FROM human_games ORDER BY completed_at DESC")
        .fetch_all(db)
        .await?;
    rows.into_iter()
        .chain(human_rows)
        .map(game_from_row)
        .filter(|result| match result {
            Ok(game) => {
                tournament_id
                    .map(|id| game.tournament_id == id)
                    .unwrap_or(true)
                    && agent_version_id
                        .map(|id| game.white_version_id == id || game.black_version_id == id)
                        .unwrap_or(true)
            }
            Err(_) => true,
        })
        .collect()
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
    let rows = sqlx::query("SELECT * FROM rating_snapshots ORDER BY created_at ASC")
        .fetch_all(db)
        .await?;
    let human_rows = sqlx::query("SELECT * FROM human_rating_snapshots ORDER BY created_at ASC")
        .fetch_all(db)
        .await?;
    rows.into_iter()
        .chain(human_rows)
        .map(rating_snapshot_from_row)
        .filter(|result| match result {
            Ok(snapshot) => {
                pool_id
                    .map(|value| snapshot.pool_id == Some(value))
                    .unwrap_or(true)
                    && agent_version_id
                        .map(|value| snapshot.agent_version_id == value)
                        .unwrap_or(true)
            }
            Err(_) => true,
        })
        .collect()
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
    let rows = sqlx::query("SELECT * FROM live_runtime_checkpoints ORDER BY updated_at DESC")
        .fetch_all(db)
        .await?;
    rows.into_iter()
        .map(live_runtime_checkpoint_from_row)
        .filter(|result| match result {
            Ok(checkpoint) => status
                .map(|value| checkpoint.status == value)
                .unwrap_or(true),
            Err(_) => true,
        })
        .collect()
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
    let rows = sqlx::query("SELECT * FROM request_journal ORDER BY completed_at DESC")
        .fetch_all(db)
        .await?;
    Ok(rows
        .into_iter()
        .map(request_journal_from_row)
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .filter(|entry| {
            match_id
                .map(|value| entry.match_id == Some(value))
                .unwrap_or(false)
                || tournament_id
                    .map(|value| entry.tournament_id == Some(value))
                    .unwrap_or(false)
                || game_id
                    .map(|value| entry.game_id == Some(value))
                    .unwrap_or(false)
        })
        .take(limit)
        .collect())
}

pub(crate) async fn list_recent_request_errors(
    db: &SqlitePool,
    limit: usize,
) -> Result<Vec<RequestJournalEntry>> {
    let rows = sqlx::query("SELECT * FROM request_journal ORDER BY completed_at DESC")
        .fetch_all(db)
        .await?;
    Ok(rows
        .into_iter()
        .map(request_journal_from_row)
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .filter(|entry| entry.status_code >= 400 || entry.error_text.is_some())
        .take(limit)
        .collect())
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
