use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::Path,
    sync::Arc,
};

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::registry_loader::{collect_registry_files, load_registry_snapshot};
use crate::registry_sync::sync_snapshot;

#[derive(Clone, Default)]
pub(crate) struct SetupRegistryCache {
    digest: Arc<Mutex<Option<u64>>>,
}

pub(crate) async fn sync_setup_registry_if_changed(
    db: &SqlitePool,
    cache: &SetupRegistryCache,
) -> Result<()> {
    sync_setup_registry_for_root(db, cache, &crate::workspace_root()).await
}

pub(crate) async fn sync_setup_registry_for_root(
    db: &SqlitePool,
    cache: &SetupRegistryCache,
    workspace_root: &Path,
) -> Result<()> {
    let digest = compute_registry_digest(workspace_root)?;
    let mut guard = cache.digest.lock().await;
    if *guard == Some(digest) {
        return Ok(());
    }

    let snapshot = load_registry_snapshot(workspace_root)?;
    sync_snapshot(db, snapshot).await?;
    *guard = Some(digest);
    Ok(())
}

fn compute_registry_digest(workspace_root: &Path) -> Result<u64> {
    let files = collect_registry_files(workspace_root)?;
    let mut hasher = DefaultHasher::new();
    files.hash(&mut hasher);

    for path in files {
        let bytes = fs::read(&path)
            .with_context(|| format!("failed to read registry file {}", path.display()))?;
        path.hash(&mut hasher);
        bytes.hash(&mut hasher);
    }

    Ok(hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{
        list_agent_versions, list_agents, list_event_presets, list_opening_suites, list_pools,
    };
    use arena_core::{FairnessConfig, Variant};
    use chrono::Utc;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::path::PathBuf;
    use uuid::Uuid;

    #[tokio::test]
    async fn syncs_rust_and_command_engines_from_registry() {
        let workspace = temp_workspace("registry-sync");
        write_registry_workspace(&workspace).unwrap();
        let db = new_test_db().await;
        let cache = SetupRegistryCache::default();

        sync_setup_registry_for_root(&db, &cache, &workspace)
            .await
            .unwrap();

        let agents = list_agents(&db).await.unwrap();
        let versions = list_agent_versions(&db, None).await.unwrap();
        let openings = list_opening_suites(&db).await.unwrap();
        let pools = list_pools(&db).await.unwrap();

        assert!(
            agents
                .iter()
                .any(|agent| agent.registry_key.as_deref() == Some("material-plus"))
        );
        assert!(
            versions
                .iter()
                .any(|version| version.registry_key.as_deref() == Some("material-plus/v1"))
        );
        assert!(
            versions
                .iter()
                .any(|version| version.registry_key.as_deref() == Some("material-plus/dev"))
        );
        assert!(
            versions
                .iter()
                .any(|version| version.registry_key.as_deref() == Some("python-ml/dev"))
        );
        assert!(
            openings
                .iter()
                .any(|suite| suite.registry_key.as_deref() == Some("starter-benchmark-suite"))
        );
        assert!(
            pools
                .iter()
                .any(|pool| pool.registry_key.as_deref() == Some("starter-standard-pool"))
        );
        assert!(
            list_event_presets(&db)
                .await
                .unwrap()
                .iter()
                .any(|preset| preset.registry_key.as_deref() == Some("starter-round-robin"))
        );
    }

    #[tokio::test]
    async fn sync_is_idempotent_and_updates_in_place() {
        let workspace = temp_workspace("registry-update");
        write_registry_workspace(&workspace).unwrap();
        let db = new_test_db().await;
        let cache = SetupRegistryCache::default();

        sync_setup_registry_for_root(&db, &cache, &workspace)
            .await
            .unwrap();

        let original_version = list_agent_versions(&db, None)
            .await
            .unwrap()
            .into_iter()
            .find(|version| version.registry_key.as_deref() == Some("python-ml/dev"))
            .unwrap();

        write_file(
            &workspace
                .join("engines")
                .join("python-ml")
                .join("arena-engine.toml"),
            r#"agent_key = "python-ml"
version_key = "dev"
agent_name = "Python ML"
version_label = "dev"
declared_name = "PyTorch Eval"
launcher = "command"
command = "python"
args = ["engines/python-ml/run.py", "--device=cpu"]
supports_chess960 = true
tags = ["ml", "python"]
notes = "Updated notes"

[env]
MODEL_PATH = "models/latest.pt"
"#,
        )
        .unwrap();

        sync_setup_registry_for_root(&db, &cache, &workspace)
            .await
            .unwrap();

        let updated_version = list_agent_versions(&db, None)
            .await
            .unwrap()
            .into_iter()
            .find(|version| version.registry_key.as_deref() == Some("python-ml/dev"))
            .unwrap();

        assert_eq!(updated_version.id, original_version.id);
        assert_eq!(
            updated_version.args,
            vec![
                "engines/python-ml/run.py".to_string(),
                "--device=cpu".to_string()
            ]
        );
        assert_eq!(updated_version.notes.as_deref(), Some("Updated notes"));
    }

    #[tokio::test]
    async fn removing_registry_entries_deletes_missing_versions_and_agents() {
        let workspace = temp_workspace("registry-prune");
        write_registry_workspace(&workspace).unwrap();
        let db = new_test_db().await;
        let cache = SetupRegistryCache::default();

        sync_setup_registry_for_root(&db, &cache, &workspace)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO benchmark_pools (id, registry_key, name, description, variant, initial_ms, increment_ms, fairness, active, created_at)
             VALUES (?, NULL, 'test', NULL, ?, ?, ?, ?, 1, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(serde_json::to_string(&Variant::Standard).unwrap())
        .bind(60_000_i64)
        .bind(1_000_i64)
        .bind(
            serde_json::to_string(&FairnessConfig {
                paired_games: true,
                swap_colors: true,
                opening_suite_id: None,
                opening_seed: None,
            })
            .unwrap(),
        )
        .bind(Utc::now().to_rfc3339())
        .execute(&db)
        .await
        .unwrap();

        fs::remove_dir_all(workspace.join("engines").join("material-plus-v1")).unwrap();
        fs::remove_file(
            workspace
                .join("setup")
                .join("pools")
                .join("starter-standard.toml"),
        )
        .unwrap();
        fs::remove_file(
            workspace
                .join("setup")
                .join("events")
                .join("starter-round-robin.toml"),
        )
        .unwrap();

        let new_cache = SetupRegistryCache::default();
        sync_setup_registry_for_root(&db, &new_cache, &workspace)
            .await
            .unwrap();

        let agents = list_agents(&db).await.unwrap();
        let versions = list_agent_versions(&db, None).await.unwrap();
        assert!(
            agents
                .iter()
                .any(|agent| agent.registry_key.as_deref() == Some("material-plus"))
        );
        assert!(
            !versions
                .iter()
                .any(|version| version.registry_key.as_deref() == Some("material-plus/v1"))
        );
        assert!(versions.iter().any(|version| {
            version.registry_key.as_deref() == Some("material-plus/dev") && version.active
        }));
        assert!(list_pools(&db).await.unwrap().is_empty());
    }

    fn temp_workspace(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("arena-{name}-{}", Uuid::new_v4()));
        if root.exists() {
            let _ = fs::remove_dir_all(&root);
        }
        root
    }

    fn write_registry_workspace(root: &Path) -> Result<()> {
        write_file(
            &root.join("Cargo.toml"),
            r#"[workspace]
members = ["engines/*"]
resolver = "2"
"#,
        )?;
        write_file(
            &root
                .join("engines")
                .join("material-plus-v1")
                .join("Cargo.toml"),
            r#"[package]
name = "material-plus-v1"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "material-plus-v1"
path = "src/main.rs"

[package.metadata.arena]
launcher = "cargo_package"
agent_key = "material-plus"
version_key = "v1"
agent_name = "Material Plus"
version_label = "v1"
declared_name = "Material Plus"
tags = ["starter", "baseline"]
notes = "Bundled material plus starter."
supports_chess960 = true
"#,
        )?;
        write_file(
            &root
                .join("engines")
                .join("material-plus-v1")
                .join("src")
                .join("main.rs"),
            "fn main() {}\n",
        )?;
        write_file(
            &root
                .join("engines")
                .join("material-plus-dev")
                .join("Cargo.toml"),
            r#"[package]
name = "material-plus-dev"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "material-plus-dev"
path = "src/main.rs"

[package.metadata.arena]
launcher = "cargo_package"
agent_key = "material-plus"
version_key = "dev"
agent_name = "Material Plus"
version_label = "dev"
declared_name = "Material Plus"
tags = ["starter", "baseline"]
notes = "Mutable development version."
supports_chess960 = true
"#,
        )?;
        write_file(
            &root
                .join("engines")
                .join("material-plus-dev")
                .join("src")
                .join("main.rs"),
            "fn main() {}\n",
        )?;
        write_file(
            &root
                .join("engines")
                .join("python-ml")
                .join("arena-engine.toml"),
            r#"agent_key = "python-ml"
version_key = "dev"
agent_name = "Python ML"
version_label = "dev"
declared_name = "PyTorch Eval"
launcher = "command"
command = "python"
args = ["engines/python-ml/run.py"]
supports_chess960 = true
tags = ["ml", "python"]
notes = "Repo-local command engine"

[env]
MODEL_PATH = "models/latest.pt"
"#,
        )?;
        write_file(
            &root.join("engines").join("python-ml").join("Cargo.toml"),
            r#"[package]
name = "python-ml"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "python-ml"
path = "src/main.rs"
"#,
        )?;
        write_file(
            &root
                .join("engines")
                .join("python-ml")
                .join("src")
                .join("main.rs"),
            "fn main() {}\n",
        )?;
        write_file(
            &root.join("setup").join("openings").join("starter.toml"),
            r#"registry_key = "starter-benchmark-suite"
name = "Starter Benchmark Suite"
description = "Small opening suite for tests."
variant = "standard"
source_kind = "starter"
source_file = "data/openings/starter.fens"
active = true
starter = true
"#,
        )?;
        write_file(
            &root
                .join("setup")
                .join("pools")
                .join("starter-standard.toml"),
            r#"registry_key = "starter-standard-pool"
name = "Starter Standard Pool"
description = "Starter pool"
variant = "standard"
initial_ms = 60000
increment_ms = 1000
paired_games = true
swap_colors = true
opening_suite_key = "starter-benchmark-suite"
opening_seed = 7
active = true
"#,
        )?;
        write_file(
            &root
                .join("setup")
                .join("events")
                .join("starter-round-robin.toml"),
            r#"registry_key = "starter-round-robin"
name = "Starter Round Robin"
kind = "round_robin"
pool_key = "starter-standard-pool"
selection_mode = "all_active_engines"
worker_count = 2
games_per_pairing = 2
active = true
"#,
        )?;
        write_file(
            &root.join("data").join("openings").join("starter.fens"),
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1\n",
        )?;
        write_file(
            &root.join("engines").join("python-ml").join("run.py"),
            "print('uciok')\n",
        )?;
        Ok(())
    }

    fn write_file(path: &Path, contents: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)?;
        Ok(())
    }

    async fn new_test_db() -> SqlitePool {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::init_db(&db).await.unwrap();
        db
    }
}
