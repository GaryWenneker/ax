//! Deterministic episode records from sessions/turns (Phase 8).

use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

fn db_err(pool: &SqlitePool, e: impl std::fmt::Display) -> AxError {
    AxError::Database(DatabaseError::new(format!(
        "{} ({})",
        e,
        pool.connect_options().get_filename().display()
    )))
}

#[derive(Debug, Clone)]
pub struct EpisodeInput {
    pub id: String,
    pub session_id: String,
    pub turn_id: String,
    pub goal: String,
    pub outcome: String,
}

pub async fn record_episode(pool: &SqlitePool, input: &EpisodeInput) -> Result<bool, AxError> {
    let id = input.id.trim();
    if id.is_empty() {
        return Ok(false);
    }
    let result = sqlx::query(
        "INSERT OR IGNORE INTO memory_episodes
         (id, session_id, turn_id, goal, outcome, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(input.session_id.trim())
    .bind(input.turn_id.trim())
    .bind(input.goal.trim())
    .bind(input.outcome.trim())
    .bind(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0),
    )
    .execute(pool)
    .await
    .map_err(|e| db_err(pool, e))?;
    Ok(result.rows_affected() == 1)
}
