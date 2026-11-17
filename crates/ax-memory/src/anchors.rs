//! Link memories to graph node versions.

use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

fn db_err(pool: &SqlitePool, e: impl std::fmt::Display) -> AxError {
    AxError::Database(DatabaseError::new(format!(
        "{} ({})",
        e,
        pool.connect_options().get_filename().display()
    )))
}

pub async fn anchor_memory(
    pool: &SqlitePool,
    memory_id: &str,
    node_key: &str,
    version: i64,
) -> Result<bool, AxError> {
    let memory_id = memory_id.trim();
    let node_key = node_key.trim();
    if memory_id.is_empty() || node_key.is_empty() {
        return Ok(false);
    }
    let result = sqlx::query(
        "INSERT OR IGNORE INTO memory_anchors (memory_id, node_key, version, stale, orphan)
         VALUES (?, ?, ?, 0, 0)",
    )
    .bind(memory_id)
    .bind(node_key)
    .bind(version)
    .execute(pool)
    .await
    .map_err(|e| db_err(pool, e))?;
    Ok(result.rows_affected() == 1)
}

/// Attach a memory to current graph versions for each of its files.
pub async fn auto_attach_memory(pool: &SqlitePool, memory_id: &str) -> Result<usize, AxError> {
    let memory_id = memory_id.trim();
    if memory_id.is_empty() {
        return Ok(0);
    }
    let files_json: Option<String> = sqlx::query_scalar("SELECT files FROM memories WHERE id = ?")
        .bind(memory_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| db_err(pool, e))?;
    let Some(files_json) = files_json else {
        return Ok(0);
    };
    let files: Vec<String> = serde_json::from_str(&files_json).unwrap_or_default();
    if files.is_empty() {
        return Ok(0);
    }
    let mut written = 0usize;
    for path in files {
        let path = path.trim();
        if path.is_empty() {
            continue;
        }
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT n.id, COALESCE(
                (SELECT version FROM graph_node_versions
                 WHERE node_key = n.id AND valid_to IS NULL
                 ORDER BY version DESC LIMIT 1), 0)
             FROM nodes n WHERE n.file_path = ?",
        )
        .bind(path)
        .fetch_all(pool)
        .await
        .map_err(|e| db_err(pool, e))?;
        for (node_key, version) in rows {
            if anchor_memory(pool, memory_id, &node_key, version).await? {
                written += 1;
            }
        }
    }
    Ok(written)
}

/// Flag anchors whose node disappeared or whose body hash drifted.
pub async fn refresh_anchor_flags(pool: &SqlitePool) -> Result<(usize, usize), AxError> {
    let anchors: Vec<(String, String, i64)> =
        sqlx::query_as("SELECT memory_id, node_key, version FROM memory_anchors")
            .fetch_all(pool)
            .await
            .map_err(|e| db_err(pool, e))?;
    let mut stale = 0usize;
    let mut orphan = 0usize;
    for (memory_id, node_key, version) in anchors {
        let node_exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM nodes WHERE id = ? LIMIT 1")
                .bind(&node_key)
                .fetch_optional(pool)
                .await
                .map_err(|e| db_err(pool, e))?;
        if node_exists.is_none() {
            sqlx::query(
                "UPDATE memory_anchors SET orphan = 1 WHERE memory_id = ? AND node_key = ? AND version = ?",
            )
            .bind(&memory_id)
            .bind(&node_key)
            .bind(version)
            .execute(pool)
            .await
            .map_err(|e| db_err(pool, e))?;
            orphan += 1;
            continue;
        }
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT body_hash FROM graph_node_versions WHERE node_key = ? AND version = ?",
        )
        .bind(&node_key)
        .bind(version)
        .fetch_optional(pool)
        .await
        .map_err(|e| db_err(pool, e))?;
        let live: Option<String> = sqlx::query_scalar(
            "SELECT body_hash FROM graph_node_versions
             WHERE node_key = ? AND valid_to IS NULL
             ORDER BY version DESC LIMIT 1",
        )
        .bind(&node_key)
        .fetch_optional(pool)
        .await
        .map_err(|e| db_err(pool, e))?;
        let is_stale = match (stored, live) {
            (Some(a), Some(b)) => a != b,
            (Some(_), None) => true,
            _ => false,
        };
        if is_stale {
            sqlx::query(
                "UPDATE memory_anchors SET stale = 1 WHERE memory_id = ? AND node_key = ? AND version = ?",
            )
            .bind(&memory_id)
            .bind(&node_key)
            .bind(version)
            .execute(pool)
            .await
            .map_err(|e| db_err(pool, e))?;
            stale += 1;
        }
    }
    Ok((stale, orphan))
}

/// Move anchors when a node body hash appears under a new key (rename/move).
pub async fn reanchor_renames(pool: &SqlitePool) -> Result<usize, AxError> {
    let pairs: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT a.node_key, b.node_key, a.body_hash
         FROM graph_node_versions a
         JOIN graph_node_versions b
           ON a.body_hash = b.body_hash AND a.node_key < b.node_key
         WHERE a.valid_to IS NULL AND b.valid_to IS NULL
         LIMIT 500",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| db_err(pool, e))?;
    let mut moved = 0usize;
    for (old_key, new_key, _hash) in pairs {
        let old_exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM nodes WHERE id = ? LIMIT 1")
                .bind(&old_key)
                .fetch_optional(pool)
                .await
                .map_err(|e| db_err(pool, e))?;
        let new_exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM nodes WHERE id = ? LIMIT 1")
                .bind(&new_key)
                .fetch_optional(pool)
                .await
                .map_err(|e| db_err(pool, e))?;
        // Rename: old gone, new present — or both present with equal body.
        if new_exists.is_none() {
            continue;
        }
        if old_exists.is_some() && old_key == new_key {
            continue;
        }
        let anchors: Vec<(String, i64)> =
            sqlx::query_as("SELECT memory_id, version FROM memory_anchors WHERE node_key = ?")
                .bind(&old_key)
                .fetch_all(pool)
                .await
                .map_err(|e| db_err(pool, e))?;
        for (memory_id, version) in anchors {
            let inserted = anchor_memory(pool, &memory_id, &new_key, version).await?;
            if inserted {
                sqlx::query(
                    "DELETE FROM memory_anchors WHERE memory_id = ? AND node_key = ? AND version = ?",
                )
                .bind(&memory_id)
                .bind(&old_key)
                .bind(version)
                .execute(pool)
                .await
                .map_err(|e| db_err(pool, e))?;
                let _ = crate::store::link_memories(
                    pool,
                    &format!("node:{old_key}"),
                    &format!("node:{new_key}"),
                    "renamed",
                )
                .await;
                moved += 1;
            }
        }
    }
    Ok(moved)
}
