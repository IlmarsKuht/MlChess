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
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut query = QueryBuilder::<Sqlite>::new("SELECT * FROM agent_versions WHERE id IN (");
    let mut separated = query.separated(", ");
    for id in ids {
        separated.push_bind(id.to_string());
    }
    query.push(") ORDER BY created_at DESC");
    let rows = query.build().fetch_all(db).await?;
    rows.into_iter().map(agent_version_from_row).collect()
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
