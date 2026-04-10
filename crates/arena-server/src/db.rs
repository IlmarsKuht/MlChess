use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Serialize, de::DeserializeOwned};
use sqlx::{Row, SqlitePool};

pub(crate) async fn init_db(db: &SqlitePool) -> Result<()> {
    for statement in [
        "CREATE TABLE IF NOT EXISTS agents (
            id TEXT PRIMARY KEY,
            registry_key TEXT,
            name TEXT NOT NULL,
            protocol TEXT NOT NULL,
            tags TEXT NOT NULL,
            notes TEXT,
            documentation TEXT,
            created_at TEXT NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS agent_versions (
            id TEXT PRIMARY KEY,
            registry_key TEXT,
            agent_id TEXT NOT NULL,
            version TEXT NOT NULL,
            active INTEGER NOT NULL,
            executable_path TEXT NOT NULL,
            working_directory TEXT,
            args TEXT NOT NULL,
            env TEXT NOT NULL,
            capabilities TEXT NOT NULL,
            declared_name TEXT,
            tags TEXT NOT NULL,
            notes TEXT,
            documentation TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY(agent_id) REFERENCES agents(id) ON DELETE CASCADE
        )",
        "CREATE TABLE IF NOT EXISTS benchmark_pools (
            id TEXT PRIMARY KEY,
            registry_key TEXT,
            name TEXT NOT NULL,
            description TEXT,
            variant TEXT NOT NULL,
            initial_ms INTEGER NOT NULL,
            increment_ms INTEGER NOT NULL,
            fairness TEXT NOT NULL,
            active INTEGER NOT NULL,
            created_at TEXT NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS opening_suites (
            id TEXT PRIMARY KEY,
            registry_key TEXT,
            name TEXT NOT NULL,
            description TEXT,
            source_kind TEXT NOT NULL,
            source_text TEXT,
            active INTEGER NOT NULL,
            starter INTEGER NOT NULL,
            positions TEXT NOT NULL,
            created_at TEXT NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS tournaments (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            pool_id TEXT NOT NULL,
            participant_version_ids TEXT NOT NULL,
            worker_count INTEGER NOT NULL,
            games_per_pairing INTEGER NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL,
            started_at TEXT,
            completed_at TEXT,
            FOREIGN KEY(pool_id) REFERENCES benchmark_pools(id) ON DELETE CASCADE
        )",
        "CREATE TABLE IF NOT EXISTS match_series (
            id TEXT PRIMARY KEY,
            tournament_id TEXT NOT NULL,
            pool_id TEXT NOT NULL,
            round_index INTEGER NOT NULL,
            white_version_id TEXT NOT NULL,
            black_version_id TEXT NOT NULL,
            opening_id TEXT,
            game_index INTEGER NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(tournament_id) REFERENCES tournaments(id) ON DELETE CASCADE,
            FOREIGN KEY(pool_id) REFERENCES benchmark_pools(id) ON DELETE CASCADE,
            FOREIGN KEY(white_version_id) REFERENCES agent_versions(id) ON DELETE CASCADE,
            FOREIGN KEY(black_version_id) REFERENCES agent_versions(id) ON DELETE CASCADE
        )",
        "CREATE TABLE IF NOT EXISTS games (
            id TEXT PRIMARY KEY,
            tournament_id TEXT NOT NULL,
            match_id TEXT NOT NULL,
            pool_id TEXT NOT NULL,
            variant TEXT NOT NULL,
            opening_id TEXT,
            white_version_id TEXT NOT NULL,
            black_version_id TEXT NOT NULL,
            result TEXT NOT NULL,
            termination TEXT NOT NULL,
            start_fen TEXT NOT NULL,
            pgn TEXT NOT NULL,
            moves_uci TEXT NOT NULL,
            white_time_left_ms INTEGER NOT NULL,
            black_time_left_ms INTEGER NOT NULL,
            logs TEXT NOT NULL,
            started_at TEXT NOT NULL,
            completed_at TEXT NOT NULL,
            FOREIGN KEY(tournament_id) REFERENCES tournaments(id) ON DELETE CASCADE,
            FOREIGN KEY(match_id) REFERENCES match_series(id) ON DELETE CASCADE,
            FOREIGN KEY(pool_id) REFERENCES benchmark_pools(id) ON DELETE CASCADE,
            FOREIGN KEY(white_version_id) REFERENCES agent_versions(id) ON DELETE CASCADE,
            FOREIGN KEY(black_version_id) REFERENCES agent_versions(id) ON DELETE CASCADE
        )",
        "CREATE TABLE IF NOT EXISTS rating_snapshots (
            id TEXT PRIMARY KEY,
            pool_id TEXT,
            agent_version_id TEXT NOT NULL,
            rating REAL NOT NULL,
            games_played INTEGER NOT NULL,
            wins INTEGER NOT NULL,
            draws INTEGER NOT NULL,
            losses INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(pool_id) REFERENCES benchmark_pools(id) ON DELETE CASCADE,
            FOREIGN KEY(agent_version_id) REFERENCES agent_versions(id) ON DELETE CASCADE
        )",
        "CREATE TABLE IF NOT EXISTS human_players (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            created_at TEXT NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS human_games (
            id TEXT PRIMARY KEY,
            tournament_id TEXT NOT NULL,
            match_id TEXT NOT NULL,
            pool_id TEXT NOT NULL,
            variant TEXT NOT NULL,
            opening_id TEXT,
            white_version_id TEXT NOT NULL,
            black_version_id TEXT NOT NULL,
            result TEXT NOT NULL,
            termination TEXT NOT NULL,
            start_fen TEXT NOT NULL,
            pgn TEXT NOT NULL,
            moves_uci TEXT NOT NULL,
            white_time_left_ms INTEGER NOT NULL,
            black_time_left_ms INTEGER NOT NULL,
            logs TEXT NOT NULL,
            started_at TEXT NOT NULL,
            completed_at TEXT NOT NULL,
            FOREIGN KEY(pool_id) REFERENCES benchmark_pools(id) ON DELETE CASCADE
        )",
        "CREATE TABLE IF NOT EXISTS human_rating_snapshots (
            id TEXT PRIMARY KEY,
            pool_id TEXT,
            human_player_id TEXT NOT NULL,
            rating REAL NOT NULL,
            games_played INTEGER NOT NULL,
            wins INTEGER NOT NULL,
            draws INTEGER NOT NULL,
            losses INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(pool_id) REFERENCES benchmark_pools(id) ON DELETE CASCADE
        )",
        "CREATE TABLE IF NOT EXISTS event_presets (
            id TEXT PRIMARY KEY,
            registry_key TEXT,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            pool_id TEXT NOT NULL,
            selection_mode TEXT NOT NULL,
            worker_count INTEGER NOT NULL,
            games_per_pairing INTEGER NOT NULL,
            active INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(pool_id) REFERENCES benchmark_pools(id) ON DELETE CASCADE
        )",
        "CREATE TABLE IF NOT EXISTS live_runtime_checkpoints (
            match_id TEXT PRIMARY KEY,
            seq INTEGER NOT NULL,
            status TEXT NOT NULL,
            result TEXT NOT NULL,
            termination TEXT NOT NULL,
            fen TEXT NOT NULL,
            moves TEXT NOT NULL,
            white_remaining_ms INTEGER NOT NULL,
            black_remaining_ms INTEGER NOT NULL,
            side_to_move TEXT NOT NULL,
            turn_started_server_unix_ms INTEGER NOT NULL,
            updated_at TEXT NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS live_runtime_events (
            match_id TEXT NOT NULL,
            seq INTEGER NOT NULL,
            event_type TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY(match_id, seq)
        )",
    ] {
        sqlx::query(statement).execute(db).await?;
    }
    ensure_column(db, "agents", "registry_key", "TEXT").await?;
    ensure_column(db, "agents", "documentation", "TEXT").await?;
    ensure_column(db, "agent_versions", "registry_key", "TEXT").await?;
    ensure_column(db, "agent_versions", "active", "INTEGER NOT NULL DEFAULT 1").await?;
    ensure_column(db, "agent_versions", "documentation", "TEXT").await?;
    ensure_column(db, "benchmark_pools", "registry_key", "TEXT").await?;
    ensure_column(db, "opening_suites", "registry_key", "TEXT").await?;
    ensure_column(db, "event_presets", "registry_key", "TEXT").await?;
    ensure_foreign_key_schema(db).await?;
    for statement in [
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_agents_registry_key ON agents(registry_key) WHERE registry_key IS NOT NULL",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_versions_registry_key ON agent_versions(registry_key) WHERE registry_key IS NOT NULL",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_benchmark_pools_registry_key ON benchmark_pools(registry_key) WHERE registry_key IS NOT NULL",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_opening_suites_registry_key ON opening_suites(registry_key) WHERE registry_key IS NOT NULL",
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_event_presets_registry_key ON event_presets(registry_key) WHERE registry_key IS NOT NULL",
    ] {
        sqlx::query(statement).execute(db).await?;
    }
    Ok(())
}

pub(crate) async fn clear_session_event_history(db: &SqlitePool) -> Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM games").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM match_series")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM tournaments")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

async fn ensure_column(db: &SqlitePool, table: &str, column: &str, definition: &str) -> Result<()> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(db)
        .await?;
    let exists = rows
        .into_iter()
        .any(|row| row.get::<String, _>("name") == column);
    if !exists {
        sqlx::query(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))
        .execute(db)
        .await?;
    }
    Ok(())
}

async fn ensure_foreign_key_schema(db: &SqlitePool) -> Result<()> {
    let expected_counts = [
        ("agent_versions", 1_i64),
        ("tournaments", 1_i64),
        ("match_series", 4_i64),
        ("games", 5_i64),
        ("rating_snapshots", 2_i64),
        ("event_presets", 1_i64),
    ];
    let mut needs_rebuild = false;
    for (table, expected) in expected_counts {
        if !table_exists(db, table).await? {
            continue;
        }
        if foreign_key_count(db, table).await? != expected {
            needs_rebuild = true;
            break;
        }
    }

    if !needs_rebuild {
        return Ok(());
    }

    let mut conn = db.acquire().await?;
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await?;
    sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;

    for statement in [
        "DROP TABLE IF EXISTS agent_versions_new",
        "CREATE TABLE agent_versions_new (
            id TEXT PRIMARY KEY,
            registry_key TEXT,
            agent_id TEXT NOT NULL,
            version TEXT NOT NULL,
            active INTEGER NOT NULL,
            executable_path TEXT NOT NULL,
            working_directory TEXT,
            args TEXT NOT NULL,
            env TEXT NOT NULL,
            capabilities TEXT NOT NULL,
            declared_name TEXT,
            tags TEXT NOT NULL,
            notes TEXT,
            documentation TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY(agent_id) REFERENCES agents(id) ON DELETE CASCADE
        )",
        "INSERT INTO agent_versions_new
            (id, registry_key, agent_id, version, active, executable_path, working_directory, args, env, capabilities, declared_name, tags, notes, documentation, created_at)
         SELECT id, registry_key, agent_id, version, COALESCE(active, 1), executable_path, working_directory, args, env, capabilities, declared_name, tags, notes, documentation, created_at
         FROM agent_versions",
        "DROP TABLE agent_versions",
        "ALTER TABLE agent_versions_new RENAME TO agent_versions",
        "DROP TABLE IF EXISTS tournaments_new",
        "CREATE TABLE tournaments_new (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            pool_id TEXT NOT NULL,
            participant_version_ids TEXT NOT NULL,
            worker_count INTEGER NOT NULL,
            games_per_pairing INTEGER NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL,
            started_at TEXT,
            completed_at TEXT,
            FOREIGN KEY(pool_id) REFERENCES benchmark_pools(id) ON DELETE CASCADE
        )",
        "INSERT INTO tournaments_new
            (id, name, kind, pool_id, participant_version_ids, worker_count, games_per_pairing, status, created_at, started_at, completed_at)
         SELECT id, name, kind, pool_id, participant_version_ids, worker_count, games_per_pairing, status, created_at, started_at, completed_at
         FROM tournaments",
        "DROP TABLE tournaments",
        "ALTER TABLE tournaments_new RENAME TO tournaments",
        "DROP TABLE IF EXISTS match_series_new",
        "CREATE TABLE match_series_new (
            id TEXT PRIMARY KEY,
            tournament_id TEXT NOT NULL,
            pool_id TEXT NOT NULL,
            round_index INTEGER NOT NULL,
            white_version_id TEXT NOT NULL,
            black_version_id TEXT NOT NULL,
            opening_id TEXT,
            game_index INTEGER NOT NULL,
            status TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(tournament_id) REFERENCES tournaments(id) ON DELETE CASCADE,
            FOREIGN KEY(pool_id) REFERENCES benchmark_pools(id) ON DELETE CASCADE,
            FOREIGN KEY(white_version_id) REFERENCES agent_versions(id) ON DELETE CASCADE,
            FOREIGN KEY(black_version_id) REFERENCES agent_versions(id) ON DELETE CASCADE
        )",
        "INSERT INTO match_series_new
            (id, tournament_id, pool_id, round_index, white_version_id, black_version_id, opening_id, game_index, status, created_at)
         SELECT id, tournament_id, pool_id, round_index, white_version_id, black_version_id, opening_id, game_index, status, created_at
         FROM match_series",
        "DROP TABLE match_series",
        "ALTER TABLE match_series_new RENAME TO match_series",
        "DROP TABLE IF EXISTS games_new",
        "CREATE TABLE games_new (
            id TEXT PRIMARY KEY,
            tournament_id TEXT NOT NULL,
            match_id TEXT NOT NULL,
            pool_id TEXT NOT NULL,
            variant TEXT NOT NULL,
            opening_id TEXT,
            white_version_id TEXT NOT NULL,
            black_version_id TEXT NOT NULL,
            result TEXT NOT NULL,
            termination TEXT NOT NULL,
            start_fen TEXT NOT NULL,
            pgn TEXT NOT NULL,
            moves_uci TEXT NOT NULL,
            white_time_left_ms INTEGER NOT NULL,
            black_time_left_ms INTEGER NOT NULL,
            logs TEXT NOT NULL,
            started_at TEXT NOT NULL,
            completed_at TEXT NOT NULL,
            FOREIGN KEY(tournament_id) REFERENCES tournaments(id) ON DELETE CASCADE,
            FOREIGN KEY(match_id) REFERENCES match_series(id) ON DELETE CASCADE,
            FOREIGN KEY(pool_id) REFERENCES benchmark_pools(id) ON DELETE CASCADE,
            FOREIGN KEY(white_version_id) REFERENCES agent_versions(id) ON DELETE CASCADE,
            FOREIGN KEY(black_version_id) REFERENCES agent_versions(id) ON DELETE CASCADE
        )",
        "INSERT INTO games_new
            (id, tournament_id, match_id, pool_id, variant, opening_id, white_version_id, black_version_id, result, termination, start_fen, pgn, moves_uci, white_time_left_ms, black_time_left_ms, logs, started_at, completed_at)
         SELECT id, tournament_id, match_id, pool_id, variant, opening_id, white_version_id, black_version_id, result, termination, start_fen, pgn, moves_uci, white_time_left_ms, black_time_left_ms, logs, started_at, completed_at
         FROM games",
        "DROP TABLE games",
        "ALTER TABLE games_new RENAME TO games",
        "DROP TABLE IF EXISTS rating_snapshots_new",
        "CREATE TABLE rating_snapshots_new (
            id TEXT PRIMARY KEY,
            pool_id TEXT,
            agent_version_id TEXT NOT NULL,
            rating REAL NOT NULL,
            games_played INTEGER NOT NULL,
            wins INTEGER NOT NULL,
            draws INTEGER NOT NULL,
            losses INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(pool_id) REFERENCES benchmark_pools(id) ON DELETE CASCADE,
            FOREIGN KEY(agent_version_id) REFERENCES agent_versions(id) ON DELETE CASCADE
        )",
        "INSERT INTO rating_snapshots_new
            (id, pool_id, agent_version_id, rating, games_played, wins, draws, losses, created_at)
         SELECT id, pool_id, agent_version_id, rating, games_played, wins, draws, losses, created_at
         FROM rating_snapshots",
        "DROP TABLE rating_snapshots",
        "ALTER TABLE rating_snapshots_new RENAME TO rating_snapshots",
        "DROP TABLE IF EXISTS event_presets_new",
        "CREATE TABLE event_presets_new (
            id TEXT PRIMARY KEY,
            registry_key TEXT,
            name TEXT NOT NULL,
            kind TEXT NOT NULL,
            pool_id TEXT NOT NULL,
            selection_mode TEXT NOT NULL,
            worker_count INTEGER NOT NULL,
            games_per_pairing INTEGER NOT NULL,
            active INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(pool_id) REFERENCES benchmark_pools(id) ON DELETE CASCADE
        )",
        "INSERT INTO event_presets_new
            (id, registry_key, name, kind, pool_id, selection_mode, worker_count, games_per_pairing, active, created_at)
         SELECT id, registry_key, name, kind, pool_id, selection_mode, worker_count, games_per_pairing, active, created_at
         FROM event_presets",
        "DROP TABLE event_presets",
        "ALTER TABLE event_presets_new RENAME TO event_presets",
        "COMMIT",
    ] {
        sqlx::query(statement).execute(&mut *conn).await?;
    }

    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

async fn table_exists(db: &SqlitePool, table: &str) -> Result<bool> {
    let row = sqlx::query("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?")
        .bind(table)
        .fetch_optional(db)
        .await?;
    Ok(row.is_some())
}

async fn foreign_key_count(db: &SqlitePool, table: &str) -> Result<i64> {
    let rows = sqlx::query(&format!("PRAGMA foreign_key_list({table})"))
        .fetch_all(db)
        .await?;
    Ok(rows.len() as i64)
}

pub(crate) fn encode_json<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}

pub(crate) fn decode_json<T: DeserializeOwned>(value: &str) -> Result<T> {
    Ok(serde_json::from_str(value)?)
}

pub(crate) fn as_bool(value: i64) -> bool {
    value != 0
}

pub(crate) fn ts(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

pub(crate) fn parse_ts(value: String) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(&value)?.with_timezone(&Utc))
}
