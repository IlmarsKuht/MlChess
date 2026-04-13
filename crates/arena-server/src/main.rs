use std::{env, path::PathBuf};

use anyhow::Result;
use arena_server::{cleanup_stale_match_statuses, run_server};
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() -> Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .compact()
        .init();

    let bind_addr = env::var("ARENA_BIND").unwrap_or_else(|_| "127.0.0.1:4000".to_string());
    let db_url = env::var("ARENA_DATABASE_URL")
        .ok()
        .or_else(default_database_url)
        .unwrap_or_else(|| "sqlite://arena.db".to_string());
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("cleanup-stale-match-statuses") {
        let updated = cleanup_stale_match_statuses(&db_url).await?;
        println!("updated {updated} stale match rows");
        return Ok(());
    }
    let frontend_dist = env::var("ARENA_FRONTEND_DIST")
        .ok()
        .map(PathBuf::from)
        .or_else(find_default_frontend_dist);
    run_server(&db_url, &bind_addr, frontend_dist).await
}

fn default_database_url() -> Option<String> {
    let path = env::current_dir().ok()?.join("arena.db");
    let normalized = path.to_string_lossy().replace('\\', "/");
    let prefix = if normalized.starts_with('/') {
        "sqlite://"
    } else {
        "sqlite:///"
    };
    Some(format!("{prefix}{normalized}"))
}

fn find_default_frontend_dist() -> Option<PathBuf> {
    let cwd = env::current_dir().ok()?;
    let candidate = cwd.join("frontend").join("dist");
    candidate.exists().then_some(candidate)
}
