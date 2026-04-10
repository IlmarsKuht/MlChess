use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use arena_core::{
    AgentCapabilities, EventPresetSelectionMode, OpeningSourceKind, TimeControl, TournamentKind,
    Variant,
};
use serde::Deserialize;
use serde_json::Value;

use crate::registry_simple_toml::parse_simple_toml;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentRegistration {
    pub(crate) registry_key: String,
    pub(crate) name: String,
    pub(crate) tags: Vec<String>,
    pub(crate) notes: Option<String>,
    pub(crate) documentation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentVersionRegistration {
    pub(crate) registry_key: String,
    pub(crate) agent_key: String,
    pub(crate) version: String,
    pub(crate) executable_path: String,
    pub(crate) working_directory: Option<String>,
    pub(crate) args: Vec<String>,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) capabilities: AgentCapabilities,
    pub(crate) declared_name: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) notes: Option<String>,
    pub(crate) documentation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpeningSuiteRegistration {
    pub(crate) registry_key: String,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) variant: Variant,
    pub(crate) source_kind: OpeningSourceKind,
    pub(crate) source_text: String,
    pub(crate) active: bool,
    pub(crate) starter: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PoolRegistration {
    pub(crate) registry_key: String,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) variant: Variant,
    pub(crate) time_control: TimeControl,
    pub(crate) paired_games: bool,
    pub(crate) swap_colors: bool,
    pub(crate) opening_suite_key: Option<String>,
    pub(crate) opening_seed: Option<u64>,
    pub(crate) active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EventPresetRegistration {
    pub(crate) registry_key: String,
    pub(crate) name: String,
    pub(crate) kind: TournamentKind,
    pub(crate) pool_key: String,
    pub(crate) selection_mode: EventPresetSelectionMode,
    pub(crate) worker_count: u16,
    pub(crate) games_per_pairing: u16,
    pub(crate) active: bool,
}

#[derive(Debug)]
pub(crate) struct RegistrySnapshot {
    pub(crate) agents: Vec<AgentRegistration>,
    pub(crate) versions: Vec<AgentVersionRegistration>,
    pub(crate) openings: Vec<OpeningSuiteRegistration>,
    pub(crate) pools: Vec<PoolRegistration>,
    pub(crate) event_presets: Vec<EventPresetRegistration>,
}

#[derive(Debug)]
struct EngineManifest {
    agent_key: String,
    version_key: String,
    agent_name: String,
    version_label: String,
    declared_name: Option<String>,
    tags: Vec<String>,
    notes: Option<String>,
    documentation: Option<String>,
    supports_chess960: bool,
    executable_path: String,
    working_directory: Option<String>,
    args: Vec<String>,
    env: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    name: String,
    version: String,
    manifest_path: String,
    metadata: Value,
    targets: Vec<CargoTarget>,
}

#[derive(Debug, Deserialize)]
struct CargoTarget {
    kind: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CargoArenaMetadata {
    launcher: Option<String>,
    agent_key: String,
    version_key: String,
    agent_name: String,
    version_label: Option<String>,
    declared_name: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    notes: Option<String>,
    documentation: Option<String>,
    documentation_file: Option<String>,
    supports_chess960: Option<bool>,
}

pub(crate) fn collect_registry_files(workspace_root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let root_manifest = workspace_root.join("Cargo.toml");
    if root_manifest.exists() {
        files.push(root_manifest);
    }

    files.extend(collect_named_files(&workspace_root.join("engines"), "Cargo.toml")?);
    files.extend(collect_named_files(
        &workspace_root.join("engines"),
        "arena-engine.toml",
    )?);
    files.extend(collect_toml_files(&workspace_root.join("setup").join("openings"))?);
    files.extend(collect_toml_files(&workspace_root.join("setup").join("pools"))?);
    files.extend(collect_toml_files(&workspace_root.join("setup").join("events"))?);
    files.sort();
    Ok(files)
}

fn collect_named_files(root: &Path, file_name: &str) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }

    for entry in fs::read_dir(root)
        .with_context(|| format!("failed to read directory {}", root.display()))?
    {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let candidate = entry.path().join(file_name);
            if candidate.exists() {
                files.push(candidate);
            }
        }
    }

    Ok(files)
}

fn collect_toml_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }

    for entry in fs::read_dir(root)
        .with_context(|| format!("failed to read directory {}", root.display()))?
    {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry.path().extension().and_then(|extension| extension.to_str()) == Some("toml")
        {
            files.push(entry.path());
        }
    }

    Ok(files)
}

pub(crate) fn load_registry_snapshot(workspace_root: &Path) -> Result<RegistrySnapshot> {
    let mut engines = load_rust_engines(workspace_root)?;
    engines.extend(load_command_engines(workspace_root)?);

    let (agents, versions) = split_engine_registrations(engines)?;
    let openings = load_opening_suites(workspace_root)?;
    let opening_keys = openings
        .iter()
        .map(|opening| opening.registry_key.clone())
        .collect::<BTreeSet<_>>();
    let pools = load_pools(workspace_root, &opening_keys)?;
    let pool_keys = pools
        .iter()
        .map(|pool| pool.registry_key.clone())
        .collect::<BTreeSet<_>>();
    let event_presets = load_event_presets(workspace_root, &pool_keys)?;

    Ok(RegistrySnapshot {
        agents,
        versions,
        openings,
        pools,
        event_presets,
    })
}

fn load_rust_engines(workspace_root: &Path) -> Result<Vec<EngineManifest>> {
    let metadata = cargo_metadata(workspace_root)?;
    let engines_root = workspace_root.join("engines");
    let workspace_dir = workspace_root.to_string_lossy().into_owned();
    let mut engines = Vec::new();

    for package in metadata.packages {
        let manifest_path = PathBuf::from(&package.manifest_path);
        let Some(package_dir) = manifest_path.parent() else {
            continue;
        };

        if !package_dir.starts_with(&engines_root) {
            continue;
        }

        if !package
            .targets
            .iter()
            .any(|target| target.kind.iter().any(|kind| kind == "bin"))
        {
            bail!(
                "engine crate {} is missing a binary target at {}",
                package.name,
                manifest_path.display()
            );
        }

        let Some(arena_metadata) = package.metadata.get("arena").cloned() else {
            if package_dir.join("arena-engine.toml").exists() {
                continue;
            }
            bail!(
                "missing [package.metadata.arena] in {}",
                manifest_path.display()
            );
        };
        let arena: CargoArenaMetadata =
            serde_json::from_value(arena_metadata).with_context(|| {
                format!(
                    "failed to parse [package.metadata.arena] in {}",
                    manifest_path.display()
                )
            })?;

        if let Some(launcher) = arena.launcher.as_deref() {
            if launcher != "cargo_package" {
                bail!(
                    "unsupported launcher {launcher} for Rust engine {} in {}",
                    package.name,
                    manifest_path.display()
                );
            }
        }

        engines.push(EngineManifest {
            agent_key: arena.agent_key,
            version_key: arena.version_key,
            agent_name: arena.agent_name,
            version_label: arena.version_label.unwrap_or(package.version),
            declared_name: arena.declared_name,
            tags: normalize_tags(arena.tags),
            notes: normalize_optional_string(arena.notes),
            documentation: resolve_documentation(
                package_dir,
                normalize_optional_string(arena.documentation),
                normalize_optional_string(arena.documentation_file),
            )?,
            supports_chess960: arena.supports_chess960.unwrap_or(true),
            executable_path: rust_engine_binary_path(workspace_root, &package.name),
            working_directory: Some(workspace_dir.clone()),
            args: Vec::new(),
            env: BTreeMap::new(),
        });
    }

    engines.sort_by(|left, right| {
        left.agent_key
            .cmp(&right.agent_key)
            .then(left.version_key.cmp(&right.version_key))
    });
    Ok(engines)
}

fn rust_engine_binary_path(workspace_root: &Path, package_name: &str) -> String {
    let binary_name = if cfg!(windows) {
        format!("{package_name}.exe")
    } else {
        package_name.to_string()
    };

    let candidate_from_current_exe = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join(&binary_name)));

    if let Some(candidate) = candidate_from_current_exe
        .filter(|path| path.exists())
    {
        return candidate.to_string_lossy().into_owned();
    }

    workspace_root
        .join("target")
        .join(default_rust_binary_profile())
        .join(binary_name)
        .to_string_lossy()
        .into_owned()
}

fn default_rust_binary_profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

fn load_command_engines(workspace_root: &Path) -> Result<Vec<EngineManifest>> {
    let mut engines = Vec::new();
    for path in collect_named_files(&workspace_root.join("engines"), "arena-engine.toml")? {
        let document = parse_simple_toml(
            &fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", path.display()))?;

        let launcher = document
            .optional_string("launcher")?
            .unwrap_or_else(|| "command".to_string());
        if launcher != "command" {
            bail!("unsupported launcher {launcher} in {}", path.display());
        }

        let command = document.require_string("command")?;
        let executable_path = normalize_command(workspace_root, &command)?;
        let working_directory = match document.optional_string("working_directory")? {
            Some(value) => Some(
                resolve_workspace_relative(workspace_root, &value)?
                    .to_string_lossy()
                    .into_owned(),
            ),
            None => None,
        };

        let manifest_dir = path
            .parent()
            .context("engine manifest should have a parent directory")?;
        engines.push(EngineManifest {
            agent_key: document.require_string("agent_key")?,
            version_key: document.require_string("version_key")?,
            agent_name: document.require_string("agent_name")?,
            version_label: document.require_string("version_label")?,
            declared_name: normalize_optional_string(document.optional_string("declared_name")?),
            tags: normalize_tags(document.optional_string_array("tags")?),
            notes: normalize_optional_string(document.optional_string("notes")?),
            documentation: resolve_documentation(
                manifest_dir,
                normalize_optional_string(document.optional_string("documentation")?),
                normalize_optional_string(document.optional_string("documentation_file")?),
            )?,
            supports_chess960: document.optional_bool("supports_chess960")?.unwrap_or(true),
            executable_path,
            working_directory,
            args: document.optional_string_array("args")?,
            env: document.string_map("env")?,
        });
    }

    engines.sort_by(|left, right| {
        left.agent_key
            .cmp(&right.agent_key)
            .then(left.version_key.cmp(&right.version_key))
    });
    Ok(engines)
}

fn split_engine_registrations(
    manifests: Vec<EngineManifest>,
) -> Result<(Vec<AgentRegistration>, Vec<AgentVersionRegistration>)> {
    let mut agents_by_key = BTreeMap::<String, AgentRegistration>::new();
    let mut versions = Vec::new();

    for manifest in manifests {
        match agents_by_key.get_mut(&manifest.agent_key) {
            Some(existing) if existing.name != manifest.agent_name => {
                bail!(
                    "conflicting agent metadata for key {}. All versions of one agent must share name",
                    manifest.agent_key
                );
            }
            Some(existing) => {
                existing.tags.extend(manifest.tags.iter().cloned());
                existing.tags = normalize_tags(std::mem::take(&mut existing.tags));
                if existing.notes.is_none() {
                    existing.notes = manifest.notes.clone();
                }
                if existing.documentation.is_none() {
                    existing.documentation = manifest.documentation.clone();
                }
            }
            None => {
                agents_by_key.insert(
                    manifest.agent_key.clone(),
                    AgentRegistration {
                        registry_key: manifest.agent_key.clone(),
                        name: manifest.agent_name.clone(),
                        tags: manifest.tags.clone(),
                        notes: manifest.notes.clone(),
                        documentation: manifest.documentation.clone(),
                    },
                );
            }
        }

        versions.push(AgentVersionRegistration {
            registry_key: format!("{}/{}", manifest.agent_key, manifest.version_key),
            agent_key: manifest.agent_key,
            version: manifest.version_label,
            executable_path: manifest.executable_path,
            working_directory: manifest.working_directory,
            args: manifest.args,
            env: manifest.env,
            capabilities: AgentCapabilities {
                supports_chess960: manifest.supports_chess960,
            },
            declared_name: manifest.declared_name,
            tags: manifest.tags,
            notes: manifest.notes,
            documentation: manifest.documentation,
        });
    }

    ensure_unique_registry_keys(
        "agent version",
        versions.iter().map(|version| version.registry_key.as_str()),
    )?;

    Ok((agents_by_key.into_values().collect(), versions))
}

fn load_opening_suites(workspace_root: &Path) -> Result<Vec<OpeningSuiteRegistration>> {
    let mut suites = Vec::new();
    for path in collect_toml_files(&workspace_root.join("setup").join("openings"))? {
        let document = parse_simple_toml(
            &fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", path.display()))?;

        let source_text = match (
            document.optional_string("source_file")?,
            document.optional_string("text")?,
        ) {
            (Some(file), None) => {
                fs::read_to_string(resolve_workspace_relative(workspace_root, &file)?)
                    .with_context(|| format!("failed to read opening source {file}"))?
            }
            (None, Some(text)) => text,
            (Some(_), Some(_)) => bail!(
                "{} must define either source_file or text, not both",
                path.display()
            ),
            (None, None) => bail!("{} must define source_file or text", path.display()),
        };

        suites.push(OpeningSuiteRegistration {
            registry_key: document.require_string("registry_key")?,
            name: document.require_string("name")?,
            description: normalize_optional_string(document.optional_string("description")?),
            variant: parse_variant(&document.require_string("variant")?)?,
            source_kind: parse_opening_source_kind(&document.require_string("source_kind")?)?,
            source_text,
            active: document.optional_bool("active")?.unwrap_or(true),
            starter: document.optional_bool("starter")?.unwrap_or(false),
        });
    }

    suites.sort_by(|left, right| left.registry_key.cmp(&right.registry_key));
    ensure_unique_registry_keys(
        "opening suite",
        suites.iter().map(|suite| suite.registry_key.as_str()),
    )?;
    Ok(suites)
}

fn load_pools(
    workspace_root: &Path,
    opening_keys: &BTreeSet<String>,
) -> Result<Vec<PoolRegistration>> {
    let mut pools = Vec::new();
    for path in collect_toml_files(&workspace_root.join("setup").join("pools"))? {
        let document = parse_simple_toml(
            &fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", path.display()))?;

        let opening_suite_key =
            normalize_optional_string(document.optional_string("opening_suite_key")?);
        if let Some(opening_suite_key) = &opening_suite_key {
            if !opening_keys.contains(opening_suite_key) {
                bail!(
                    "pool {} references unknown opening suite key {}",
                    path.display(),
                    opening_suite_key
                );
            }
        }

        let opening_seed = document
            .optional_integer("opening_seed")?
            .map(|value| value.try_into())
            .transpose()
            .context("opening_seed must be non-negative")?;

        pools.push(PoolRegistration {
            registry_key: document.require_string("registry_key")?,
            name: document.require_string("name")?,
            description: normalize_optional_string(document.optional_string("description")?),
            variant: parse_variant(&document.require_string("variant")?)?,
            time_control: TimeControl {
                initial_ms: document
                    .require_integer("initial_ms")?
                    .try_into()
                    .context("initial_ms must be non-negative")?,
                increment_ms: document
                    .require_integer("increment_ms")?
                    .try_into()
                    .context("increment_ms must be non-negative")?,
            },
            paired_games: document.require_bool("paired_games")?,
            swap_colors: document.require_bool("swap_colors")?,
            opening_suite_key,
            opening_seed,
            active: document.optional_bool("active")?.unwrap_or(true),
        });
    }

    pools.sort_by(|left, right| left.registry_key.cmp(&right.registry_key));
    ensure_unique_registry_keys("pool", pools.iter().map(|pool| pool.registry_key.as_str()))?;
    Ok(pools)
}

fn load_event_presets(
    workspace_root: &Path,
    pool_keys: &BTreeSet<String>,
) -> Result<Vec<EventPresetRegistration>> {
    let mut presets = Vec::new();
    for path in collect_toml_files(&workspace_root.join("setup").join("events"))? {
        let document = parse_simple_toml(
            &fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", path.display()))?;

        let pool_key = document.require_string("pool_key")?;
        if !pool_keys.contains(&pool_key) {
            bail!(
                "event preset {} references unknown pool key {}",
                path.display(),
                pool_key
            );
        }

        presets.push(EventPresetRegistration {
            registry_key: document.require_string("registry_key")?,
            name: document.require_string("name")?,
            kind: parse_tournament_kind(&document.require_string("kind")?)?,
            pool_key,
            selection_mode: parse_selection_mode(&document.require_string("selection_mode")?)?,
            worker_count: document
                .require_integer("worker_count")?
                .try_into()
                .context("worker_count must be non-negative")?,
            games_per_pairing: document
                .require_integer("games_per_pairing")?
                .try_into()
                .context("games_per_pairing must be non-negative")?,
            active: document.optional_bool("active")?.unwrap_or(true),
        });
    }

    presets.sort_by(|left, right| left.registry_key.cmp(&right.registry_key));
    ensure_unique_registry_keys(
        "event preset",
        presets.iter().map(|preset| preset.registry_key.as_str()),
    )?;
    Ok(presets)
}

fn normalize_tags(mut tags: Vec<String>) -> Vec<String> {
    for tag in &mut tags {
        *tag = tag.trim().to_string();
    }
    tags.retain(|tag| !tag.is_empty());
    tags.sort();
    tags.dedup();
    tags
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn resolve_documentation(
    base_dir: &Path,
    inline: Option<String>,
    file: Option<String>,
) -> Result<Option<String>> {
    match (inline, file) {
        (Some(_), Some(file)) => {
            bail!("documentation and documentation_file cannot both be set (got {file})")
        }
        (Some(inline), None) => Ok(Some(inline)),
        (None, Some(file)) => {
            let path = resolve_relative_to(base_dir, &file)?;
            Ok(Some(
                fs::read_to_string(&path)
                    .with_context(|| format!("failed to read documentation file {}", path.display()))?
                    .trim()
                    .to_string(),
            )
            .filter(|text| !text.is_empty()))
        }
        (None, None) => Ok(None),
    }
}

fn cargo_metadata(workspace_root: &Path) -> Result<CargoMetadata> {
    let cargo_binary = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(cargo_binary)
        .current_dir(workspace_root)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .context("failed to run cargo metadata")?;

    if !output.status.success() {
        bail!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    serde_json::from_slice(&output.stdout).context("failed to decode cargo metadata JSON")
}

fn normalize_command(workspace_root: &Path, value: &str) -> Result<String> {
    let path = Path::new(value);
    if path.is_absolute() {
        bail!("command path {value} must be repo-relative or a bare executable");
    }

    if value.contains('/') || value.contains('\\') || value.starts_with('.') {
        let path = resolve_workspace_relative(workspace_root, value)?;
        if !path.exists() {
            bail!("command path {} does not exist", path.display());
        }
        return Ok(path.to_string_lossy().into_owned());
    }

    Ok(value.to_string())
}

fn resolve_workspace_relative(workspace_root: &Path, value: &str) -> Result<PathBuf> {
    resolve_relative_to(workspace_root, value)
}

fn resolve_relative_to(base: &Path, value: &str) -> Result<PathBuf> {
    let mut path = PathBuf::from(base);
    for component in Path::new(value).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => path.push(part),
            Component::ParentDir => bail!("path {value} must stay within the workspace root"),
            Component::Prefix(_) | Component::RootDir => {
                bail!("path {value} must be relative to the workspace root")
            }
        }
    }
    Ok(path)
}

fn parse_variant(value: &str) -> Result<Variant> {
    match value {
        "standard" => Ok(Variant::Standard),
        "chess960" => Ok(Variant::Chess960),
        _ => bail!("unsupported variant {value}"),
    }
}

fn parse_tournament_kind(value: &str) -> Result<TournamentKind> {
    match value {
        "round_robin" => Ok(TournamentKind::RoundRobin),
        "ladder" => Ok(TournamentKind::Ladder),
        _ => bail!("unsupported tournament kind {value}"),
    }
}

fn parse_selection_mode(value: &str) -> Result<EventPresetSelectionMode> {
    match value {
        "all_active_engines" => Ok(EventPresetSelectionMode::AllActiveEngines),
        _ => bail!("unsupported selection mode {value}"),
    }
}

fn parse_opening_source_kind(value: &str) -> Result<OpeningSourceKind> {
    match value {
        "starter" => Ok(OpeningSourceKind::Starter),
        "fen_list" => Ok(OpeningSourceKind::FenList),
        "pgn_import" => Ok(OpeningSourceKind::PgnImport),
        _ => bail!("unsupported opening source kind {value}"),
    }
}

fn ensure_unique_registry_keys<'a>(
    label: &str,
    keys: impl IntoIterator<Item = &'a str>,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for key in keys {
        if !seen.insert(key.to_string()) {
            bail!("duplicate {label} registry_key {key}");
        }
    }
    Ok(())
}
