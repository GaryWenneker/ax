//! Memory access log for ACT-R and usage analytics.

use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

fn db_err(pool: &SqlitePool, e: impl std::fmt::Display) -> AxError {
    AxError::Database(DatabaseError::new(format!(
        "{} ({})",
        e,
        pool.connect_options().get_filename().display()
    )))
}

pub async fn record_access(
    pool: &SqlitePool,
    memory_id: &str,
    kind: &str,
    accessed_at: i64,
) -> Result<bool, AxError> {
    let memory_id = memory_id.trim();
    let kind = kind.trim();
    if memory_id.is_empty() || kind.is_empty() {
        return Ok(false);
    }
    let result =
        sqlx::query("INSERT INTO memory_access (memory_id, kind, accessed_at) VALUES (?, ?, ?)")
            .bind(memory_id)
            .bind(kind)
            .bind(accessed_at)
            .execute(pool)
            .await
            .map_err(|e| db_err(pool, e))?;
    Ok(result.rows_affected() == 1)
}

pub async fn access_times(pool: &SqlitePool, memory_id: &str) -> Result<Vec<i64>, AxError> {
    let rows: Vec<i64> = sqlx::query_scalar(
        "SELECT accessed_at FROM memory_access WHERE memory_id = ? ORDER BY accessed_at",
    )
    .bind(memory_id.trim())
    .fetch_all(pool)
    .await
    .map_err(|e| db_err(pool, e))?;
    Ok(rows)
}

pub async fn record_feedback(
    pool: &SqlitePool,
    memory_id: &str,
    useful: bool,
    accessed_at: i64,
) -> Result<bool, AxError> {
    let kind = if useful { "useful" } else { "wrong" };
    record_access(pool, memory_id, kind, accessed_at).await
}
