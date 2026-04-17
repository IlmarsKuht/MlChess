use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use uuid::Uuid;

use crate::{
    db::init_db,
    presentation::{resolve_match_lifecycle, resolve_tournament_status},
};

pub async fn cleanup_stale_match_statuses(db_url: &str) -> Result<u64> {
    let db_options = db_url
        .parse::<SqliteConnectOptions>()
        .with_context(|| format!("failed to parse sqlite connection string {db_url}"))?
        .create_if_missing(true)
        .foreign_keys(true);
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(db_options)
        .await
        .with_context(|| format!("failed to connect to {db_url}"))?;
    sqlx::query("PRAGMA foreign_keys = ON").execute(&db).await?;
    init_db(&db).await?;
    reconcile_history_statuses(&db).await
}

pub(crate) async fn reconcile_history_statuses(db: &sqlx::SqlitePool) -> Result<u64> {
    let tournaments = crate::storage::list_tournaments(db).await?;
    let match_series = crate::storage::list_match_series(db, None).await?;
    let games = crate::storage::list_games(db, None, None).await?;
    let checkpoints = crate::storage::list_live_runtime_checkpoints(db, None).await?;
    let game_by_match_id = games
        .into_iter()
        .map(|game| (game.match_id, game))
        .collect::<std::collections::HashMap<_, _>>();
    let checkpoint_by_match_id = checkpoints
        .into_iter()
        .map(|checkpoint| (checkpoint.match_id, checkpoint))
        .collect::<std::collections::HashMap<_, _>>();
    let tournament_by_id = tournaments
        .iter()
        .map(|tournament| (tournament.id, tournament))
        .collect::<std::collections::HashMap<_, _>>();
    let now = Utc::now();
    let stale_cutoff = now - chrono::Duration::seconds(30);
    let mut updated = 0_u64;
    let mut match_statuses_by_tournament_id: std::collections::HashMap<
        Uuid,
        Vec<arena_core::MatchStatus>,
    > = std::collections::HashMap::new();
    let mut tournament_completed_at: std::collections::HashMap<Uuid, chrono::DateTime<Utc>> =
        std::collections::HashMap::new();

    for series in match_series {
        let tournament = match tournament_by_id.get(&series.tournament_id) {
            Some(value) => *value,
            None => continue,
        };
        let game = game_by_match_id.get(&series.id);
        let checkpoint = checkpoint_by_match_id.get(&series.id);
        let (mut resolved_status, _, _) = resolve_match_lifecycle(
            &series,
            tournament.status,
            game.map(|value| value.id),
            checkpoint,
        );
        if game.is_none()
            && checkpoint.is_none()
            && series.created_at <= stale_cutoff
            && tournament.status != arena_core::TournamentStatus::Draft
        {
            resolved_status = match series.status {
                arena_core::MatchStatus::Pending => arena_core::MatchStatus::Skipped,
                arena_core::MatchStatus::Running => arena_core::MatchStatus::Failed,
                status => status,
            };
        }

        if resolved_status != series.status {
            crate::storage::update_match_series_status(db, series.id, resolved_status).await?;
            updated += 1;
        }
        match_statuses_by_tournament_id
            .entry(series.tournament_id)
            .or_default()
            .push(resolved_status);
        if let Some(completed_at) = game.map(|value| value.completed_at).or_else(|| {
            checkpoint.and_then(|value| {
                (value.status != arena_core::LiveStatus::Running).then_some(value.updated_at)
            })
        }) {
            tournament_completed_at
                .entry(series.tournament_id)
                .and_modify(|existing| {
                    if completed_at > *existing {
                        *existing = completed_at;
                    }
                })
                .or_insert(completed_at);
        }
    }

    for tournament in tournaments {
        let empty = Vec::new();
        let match_statuses = match_statuses_by_tournament_id
            .get(&tournament.id)
            .unwrap_or(&empty);
        let resolved_status = resolve_tournament_status(&tournament, match_statuses, now);
        if resolved_status == tournament.status {
            continue;
        }
        let completed_at = if matches!(
            resolved_status,
            arena_core::TournamentStatus::Completed
                | arena_core::TournamentStatus::Failed
                | arena_core::TournamentStatus::Stopped
        ) {
            Some(
                tournament
                    .completed_at
                    .or_else(|| tournament_completed_at.get(&tournament.id).copied())
                    .unwrap_or(now),
            )
        } else {
            tournament.completed_at
        };
        crate::storage::update_tournament_status(
            db,
            tournament.id,
            resolved_status,
            tournament.started_at,
            completed_at,
        )
        .await?;
        updated += 1;
    }

    Ok(updated)
}
