use std::collections::{BTreeSet, HashMap};

use anyhow::{Result, anyhow};
use arena_core::{
    Agent, AgentProtocol, AgentVersion, BenchmarkPool, EventPreset, FairnessConfig,
    OpeningImportRequest, OpeningSuite, import_opening_suite,
};
use chrono::Utc;
use sqlx::SqlitePool;
use tracing::info;
use uuid::Uuid;

use crate::registry_loader::{
    AgentRegistration, AgentVersionRegistration, EventPresetRegistration, OpeningSuiteRegistration,
    PoolRegistration, RegistrySnapshot,
};
use crate::storage::{
    list_agent_versions, list_agents, list_event_presets, list_opening_suites, list_pools,
    insert_agent, insert_agent_version, insert_event_preset, insert_opening_suite, insert_pool,
    update_agent, update_agent_version, update_event_preset, update_opening_suite, update_pool,
};

pub(crate) async fn sync_snapshot(db: &SqlitePool, snapshot: RegistrySnapshot) -> Result<()> {
    let opening_ids = sync_opening_suites(db, &snapshot.openings).await?;
    let pool_ids = sync_pools(db, &snapshot.pools, &opening_ids).await?;
    sync_event_presets(db, &snapshot.event_presets, &pool_ids).await?;
    sync_agents_and_versions(db, &snapshot.agents, &snapshot.versions).await?;
    info!(
        "synced setup registry: {} agents, {} versions, {} openings, {} pools, {} event presets",
        snapshot.agents.len(),
        snapshot.versions.len(),
        snapshot.openings.len(),
        snapshot.pools.len(),
        snapshot.event_presets.len()
    );
    Ok(())
}

async fn sync_agents_and_versions(
    db: &SqlitePool,
    agent_defs: &[AgentRegistration],
    version_defs: &[AgentVersionRegistration],
) -> Result<()> {
    let existing_agents = list_agents(db).await?;
    let existing_versions = list_agent_versions(db, None).await?;
    let mut agent_ids_by_key = HashMap::<String, Uuid>::new();

    for definition in agent_defs {
        let existing = existing_agents
            .iter()
            .find(|agent| agent.registry_key.as_deref() == Some(definition.registry_key.as_str()))
            .cloned();

        let mut agent = match &existing {
            Some(agent) => agent.clone(),
            None => Agent {
                id: Uuid::new_v4(),
                registry_key: Some(definition.registry_key.clone()),
                name: definition.name.clone(),
                protocol: AgentProtocol::Uci,
                tags: definition.tags.clone(),
                notes: definition.notes.clone(),
                created_at: Utc::now(),
            },
        };

        let changed = agent.registry_key.as_deref() != Some(definition.registry_key.as_str())
            || agent.name != definition.name
            || agent.protocol != AgentProtocol::Uci
            || agent.tags != definition.tags
            || agent.notes != definition.notes;

        if changed {
            agent.registry_key = Some(definition.registry_key.clone());
            agent.name = definition.name.clone();
            agent.protocol = AgentProtocol::Uci;
            agent.tags = definition.tags.clone();
            agent.notes = definition.notes.clone();
        }

        match existing {
            Some(_) if changed => update_agent(db, &agent).await?,
            None => insert_agent(db, &agent).await?,
            _ => {}
        }

        agent_ids_by_key.insert(definition.registry_key.clone(), agent.id);
    }

    for definition in version_defs {
        let agent_id = *agent_ids_by_key
            .get(&definition.agent_key)
            .ok_or_else(|| anyhow!("missing agent for version {}", definition.registry_key))?;

        let existing = existing_versions
            .iter()
            .find(|version| version.registry_key.as_deref() == Some(definition.registry_key.as_str()))
            .cloned();

        let mut version = match &existing {
            Some(version) => version.clone(),
            None => AgentVersion {
                id: Uuid::new_v4(),
                registry_key: Some(definition.registry_key.clone()),
                agent_id,
                version: definition.version.clone(),
                executable_path: definition.executable_path.clone(),
                working_directory: definition.working_directory.clone(),
                args: definition.args.clone(),
                env: definition.env.clone(),
                capabilities: definition.capabilities.clone(),
                declared_name: definition.declared_name.clone(),
                tags: definition.tags.clone(),
                notes: definition.notes.clone(),
                created_at: Utc::now(),
            },
        };

        let changed = version.registry_key.as_deref() != Some(definition.registry_key.as_str())
            || version.agent_id != agent_id
            || version.version != definition.version
            || version.executable_path != definition.executable_path
            || version.working_directory != definition.working_directory
            || version.args != definition.args
            || version.env != definition.env
            || version.capabilities != definition.capabilities
            || version.declared_name != definition.declared_name
            || version.tags != definition.tags
            || version.notes != definition.notes;

        if changed {
            version.registry_key = Some(definition.registry_key.clone());
            version.agent_id = agent_id;
            version.version = definition.version.clone();
            version.executable_path = definition.executable_path.clone();
            version.working_directory = definition.working_directory.clone();
            version.args = definition.args.clone();
            version.env = definition.env.clone();
            version.capabilities = definition.capabilities.clone();
            version.declared_name = definition.declared_name.clone();
            version.tags = definition.tags.clone();
            version.notes = definition.notes.clone();
        }

        match existing {
            Some(_) if changed => update_agent_version(db, &version).await?,
            None => insert_agent_version(db, &version).await?,
            _ => {}
        }
    }

    let version_keys = version_defs
        .iter()
        .map(|definition| definition.registry_key.as_str())
        .collect::<BTreeSet<_>>();
    for version in existing_versions {
        if let Some(key) = version.registry_key.as_deref() {
            if !version_keys.contains(key) {
                sqlx::query("DELETE FROM agent_versions WHERE id = ?")
                    .bind(version.id.to_string())
                    .execute(db)
                    .await?;
            }
        }
    }

    let agent_keys = agent_defs
        .iter()
        .map(|definition| definition.registry_key.as_str())
        .collect::<BTreeSet<_>>();
    for agent in existing_agents {
        if let Some(key) = agent.registry_key.as_deref() {
            if !agent_keys.contains(key) {
                sqlx::query("DELETE FROM agents WHERE id = ?")
                    .bind(agent.id.to_string())
                    .execute(db)
                    .await?;
            }
        }
    }

    Ok(())
}

async fn sync_opening_suites(
    db: &SqlitePool,
    definitions: &[OpeningSuiteRegistration],
) -> Result<HashMap<String, Uuid>> {
    let existing_suites = list_opening_suites(db).await?;
    let mut suite_ids = HashMap::new();

    for definition in definitions {
        let imported = import_opening_suite(OpeningImportRequest {
            registry_key: Some(definition.registry_key.clone()),
            name: definition.name.clone(),
            description: definition.description.clone(),
            variant: definition.variant,
            text: definition.source_text.clone(),
            source_kind: definition.source_kind.clone(),
            starter: definition.starter,
        })?;

        let existing = existing_suites
            .iter()
            .find(|suite| suite.registry_key.as_deref() == Some(definition.registry_key.as_str()))
            .cloned();

        let mut suite = match &existing {
            Some(existing) => opening_suite_with_identity(imported, existing.id, existing.created_at),
            None => imported,
        };
        suite.active = definition.active;

        match existing {
            Some(existing) => {
                if !opening_suites_match(&existing, &suite) {
                    update_opening_suite(db, &suite).await?;
                }
                suite_ids.insert(definition.registry_key.clone(), existing.id);
            }
            None => {
                insert_opening_suite(db, &suite).await?;
                suite_ids.insert(definition.registry_key.clone(), suite.id);
            }
        }
    }

    let suite_keys = definitions
        .iter()
        .map(|definition| definition.registry_key.as_str())
        .collect::<BTreeSet<_>>();
    for suite in existing_suites {
        let should_delete = match suite.registry_key.as_deref() {
            Some(key) => !suite_keys.contains(key),
            None => true,
        };
        if should_delete {
            sqlx::query("DELETE FROM opening_suites WHERE id = ?")
                .bind(suite.id.to_string())
                .execute(db)
                .await?;
        }
    }

    Ok(suite_ids)
}

async fn sync_pools(
    db: &SqlitePool,
    definitions: &[PoolRegistration],
    opening_ids: &HashMap<String, Uuid>,
) -> Result<HashMap<String, Uuid>> {
    let existing_pools = list_pools(db).await?;
    let mut pool_ids = HashMap::new();

    for definition in definitions {
        let fairness = FairnessConfig {
            paired_games: definition.paired_games,
            swap_colors: definition.swap_colors,
            opening_suite_id: definition
                .opening_suite_key
                .as_ref()
                .map(|key| {
                    opening_ids
                        .get(key)
                        .copied()
                        .ok_or_else(|| anyhow!("missing opening suite id for key {key}"))
                })
                .transpose()?,
            opening_seed: definition.opening_seed,
        };

        let existing = existing_pools
            .iter()
            .find(|pool| pool.registry_key.as_deref() == Some(definition.registry_key.as_str()))
            .cloned();

        let mut pool = match &existing {
            Some(pool) => pool.clone(),
            None => BenchmarkPool {
                id: Uuid::new_v4(),
                registry_key: Some(definition.registry_key.clone()),
                name: definition.name.clone(),
                description: definition.description.clone(),
                variant: definition.variant,
                time_control: definition.time_control.clone(),
                fairness: fairness.clone(),
                active: definition.active,
                created_at: Utc::now(),
            },
        };

        let changed = pool.registry_key.as_deref() != Some(definition.registry_key.as_str())
            || pool.name != definition.name
            || pool.description != definition.description
            || pool.variant != definition.variant
            || pool.time_control != definition.time_control
            || pool.fairness != fairness
            || pool.active != definition.active;

        if changed {
            pool.registry_key = Some(definition.registry_key.clone());
            pool.name = definition.name.clone();
            pool.description = definition.description.clone();
            pool.variant = definition.variant;
            pool.time_control = definition.time_control.clone();
            pool.fairness = fairness;
            pool.active = definition.active;
        }

        match existing {
            Some(_) if changed => update_pool(db, &pool).await?,
            None => insert_pool(db, &pool).await?,
            _ => {}
        }
        pool_ids.insert(definition.registry_key.clone(), pool.id);
    }

    let pool_keys = definitions
        .iter()
        .map(|definition| definition.registry_key.as_str())
        .collect::<BTreeSet<_>>();
    for pool in existing_pools {
        let should_delete = match pool.registry_key.as_deref() {
            Some(key) => !pool_keys.contains(key),
            None => true,
        };
        if should_delete {
            sqlx::query("DELETE FROM benchmark_pools WHERE id = ?")
                .bind(pool.id.to_string())
                .execute(db)
                .await?;
        }
    }

    Ok(pool_ids)
}

async fn sync_event_presets(
    db: &SqlitePool,
    definitions: &[EventPresetRegistration],
    pool_ids: &HashMap<String, Uuid>,
) -> Result<()> {
    let existing_presets = list_event_presets(db).await?;

    for definition in definitions {
        let pool_id = *pool_ids
            .get(&definition.pool_key)
            .ok_or_else(|| anyhow!("missing pool for event preset {}", definition.registry_key))?;
        let existing = existing_presets
            .iter()
            .find(|preset| preset.registry_key.as_deref() == Some(definition.registry_key.as_str()))
            .cloned();

        let mut preset = match &existing {
            Some(preset) => preset.clone(),
            None => EventPreset {
                id: Uuid::new_v4(),
                registry_key: Some(definition.registry_key.clone()),
                name: definition.name.clone(),
                kind: definition.kind,
                pool_id,
                selection_mode: definition.selection_mode,
                worker_count: definition.worker_count.max(1),
                games_per_pairing: definition.games_per_pairing.max(1),
                active: definition.active,
                created_at: Utc::now(),
            },
        };

        let changed = preset.registry_key.as_deref() != Some(definition.registry_key.as_str())
            || preset.name != definition.name
            || preset.kind != definition.kind
            || preset.pool_id != pool_id
            || preset.selection_mode != definition.selection_mode
            || preset.worker_count != definition.worker_count.max(1)
            || preset.games_per_pairing != definition.games_per_pairing.max(1)
            || preset.active != definition.active;

        if changed {
            preset.registry_key = Some(definition.registry_key.clone());
            preset.name = definition.name.clone();
            preset.kind = definition.kind;
            preset.pool_id = pool_id;
            preset.selection_mode = definition.selection_mode;
            preset.worker_count = definition.worker_count.max(1);
            preset.games_per_pairing = definition.games_per_pairing.max(1);
            preset.active = definition.active;
        }

        match existing {
            Some(_) if changed => update_event_preset(db, &preset).await?,
            None => insert_event_preset(db, &preset).await?,
            _ => {}
        }
    }

    let preset_keys = definitions
        .iter()
        .map(|definition| definition.registry_key.as_str())
        .collect::<BTreeSet<_>>();
    for preset in existing_presets {
        let should_delete = match preset.registry_key.as_deref() {
            Some(key) => !preset_keys.contains(key),
            None => true,
        };
        if should_delete {
            sqlx::query("DELETE FROM event_presets WHERE id = ?")
                .bind(preset.id.to_string())
                .execute(db)
                .await?;
        }
    }

    Ok(())
}

fn opening_suite_with_identity(
    mut suite: OpeningSuite,
    id: Uuid,
    created_at: chrono::DateTime<Utc>,
) -> OpeningSuite {
    suite.id = id;
    suite.created_at = created_at;
    for position in &mut suite.positions {
        position.suite_id = id;
    }
    suite
}

fn opening_suites_match(left: &OpeningSuite, right: &OpeningSuite) -> bool {
    left.registry_key == right.registry_key
        && left.name == right.name
        && left.description == right.description
        && left.source_kind == right.source_kind
        && left.source_text == right.source_text
        && left.active == right.active
        && left.starter == right.starter
        && left.positions.len() == right.positions.len()
        && left
            .positions
            .iter()
            .zip(&right.positions)
            .all(|(left, right)| {
                left.label == right.label && left.fen == right.fen && left.variant == right.variant
            })
}
