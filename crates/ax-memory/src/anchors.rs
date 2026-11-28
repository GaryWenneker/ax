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

/// An anchor is stale when the version it points at has a different body hash than the
/// node's live (open) version, or when the node has no open version any more.
const ANCHOR_DRIFTED: &str = "EXISTS (
        SELECT 1 FROM graph_node_versions s
        WHERE s.node_key = memory_anchors.node_key
          AND s.version = memory_anchors.version
          AND s.body_hash IS NOT (
              SELECT l.body_hash FROM graph_node_versions l
              WHERE l.node_key = memory_anchors.node_key AND l.valid_to IS NULL
              ORDER BY l.version DESC LIMIT 1))";

const NODE_EXISTS: &str = "EXISTS (SELECT 1 FROM nodes WHERE nodes.id = memory_anchors.node_key)";

/// Bring the `orphan` and `stale` flags of every anchor in line with the graph.
///
/// Set-based: a few statements in one transaction, whatever the number of anchors.
/// Flags go both ways: an anchor whose node comes back (file restored, branch switch)
/// loses `orphan`, and one whose live body hash matches again loses `stale`.
/// Returns `(newly_stale, newly_orphaned)`: anchors flagged by this run.
pub async fn refresh_anchor_flags(pool: &SqlitePool) -> Result<(usize, usize), AxError> {
    // Safe (all four statements): only the const fragments NODE_EXISTS and ANCHOR_DRIFTED
    // are interpolated.
    let mut tx = pool.begin().await.map_err(|e| db_err(pool, e))?;
    let orphan = sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE memory_anchors SET orphan = 1 WHERE orphan = 0 AND NOT {NODE_EXISTS}"
    )))
    .execute(&mut *tx)
    .await
    .map_err(|e| db_err(pool, e))?
    .rows_affected();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE memory_anchors SET orphan = 0 WHERE orphan <> 0 AND {NODE_EXISTS}"
    )))
    .execute(&mut *tx)
    .await
    .map_err(|e| db_err(pool, e))?;
    let stale = sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE memory_anchors SET stale = 1
         WHERE stale = 0 AND {NODE_EXISTS} AND {ANCHOR_DRIFTED}"
    )))
    .execute(&mut *tx)
    .await
    .map_err(|e| db_err(pool, e))?
    .rows_affected();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE memory_anchors SET stale = 0
         WHERE stale <> 0 AND {NODE_EXISTS} AND NOT {ANCHOR_DRIFTED}"
    )))
    .execute(&mut *tx)
    .await
    .map_err(|e| db_err(pool, e))?;
    tx.commit().await.map_err(|e| db_err(pool, e))?;
    Ok((stale as usize, orphan as usize))
}

/// Rename pairs read per pass.
const RENAME_PAGE: usize = 500;

/// Move anchors from a removed symbol to the symbol that replaced it (rename or move).
///
/// A pair counts as a rename only when all of these hold:
/// - the old key has anchors, an open version, and no row in `nodes` any more;
/// - the new key is in `nodes`, has the same kind and live body hash, and its first
///   version is not older than the old key's live version (it appeared afterwards);
/// - the match is unique both ways: one candidate for the old key, one old key for
///   the new key across the whole table, not just the page read in this pass.
///   Same-signature symbols elsewhere make the pair ambiguous; those anchors are
///   left alone and later flagged as orphans.
///
/// The old key's open version is closed whether or not the move went ahead, so a
/// removed symbol leaves the join after one pass.
pub async fn reanchor_renames(pool: &SqlitePool) -> Result<usize, AxError> {
    // `olds` counts every removed key that matches the new key, over the whole table
    // and not only this page. A removed key closed by an earlier pass still counts when
    // it was open after the new key appeared and kept its anchors (it was ambiguous), so
    // a later pass cannot treat the new key as unique once its rival has been closed.
    let pairs: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT a.node_key, b.node_key,
                (SELECT COUNT(DISTINCT a2.node_key) FROM graph_node_versions a2
                 WHERE a2.body_hash = b.body_hash AND a2.kind = b.kind
                   AND a2.node_key <> b.node_key
                   AND NOT EXISTS (SELECT 1 FROM nodes WHERE id = a2.node_key)
                   AND EXISTS (SELECT 1 FROM memory_anchors m WHERE m.node_key = a2.node_key)
                   AND NOT EXISTS (SELECT 1 FROM graph_node_versions g
                                   WHERE g.node_key = a2.node_key AND g.valid_from > a2.valid_from)
                   AND (SELECT MIN(f.valid_from) FROM graph_node_versions f
                        WHERE f.node_key = b.node_key) >= a2.valid_from
                   AND (a2.valid_to IS NULL
                        OR a2.valid_to >= (SELECT MIN(f.valid_from) FROM graph_node_versions f
                                           WHERE f.node_key = b.node_key))) AS olds
         FROM graph_node_versions a
         JOIN graph_node_versions b
           ON a.body_hash = b.body_hash AND a.kind = b.kind AND a.node_key <> b.node_key
         WHERE a.valid_to IS NULL AND b.valid_to IS NULL
           AND NOT EXISTS (SELECT 1 FROM nodes WHERE id = a.node_key)
           AND EXISTS (SELECT 1 FROM nodes WHERE id = b.node_key)
           AND EXISTS (SELECT 1 FROM memory_anchors m WHERE m.node_key = a.node_key)
           AND (SELECT MIN(f.valid_from) FROM graph_node_versions f
                WHERE f.node_key = b.node_key) >= a.valid_from
         ORDER BY a.node_key, b.node_key
         LIMIT ?",
    )
    .bind(RENAME_PAGE as i64)
    .fetch_all(pool)
    .await
    .map_err(|e| db_err(pool, e))?;

    // A full page may cut the last old key's candidates short. With one candidate in
    // the page it may still have more, so it waits for the next pass. With two or more
    // it is ambiguous either way and is closed now; otherwise a key with a page full of
    // candidates would fill every pass and block every key after it.
    let truncated_last = (pairs.len() == RENAME_PAGE)
        .then(|| pairs.last().map(|(old, _, _)| old.clone()))
        .flatten()
        .filter(|last| pairs.iter().filter(|(old, _, _)| old == last).count() == 1);
    let mut per_old: std::collections::BTreeMap<&str, Vec<(&str, i64)>> = Default::default();
    for (old_key, new_key, olds) in &pairs {
        if truncated_last.as_deref() == Some(old_key.as_str()) {
            continue;
        }
        per_old.entry(old_key).or_default().push((new_key, *olds));
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let mut moved = 0usize;
    for (old_key, candidates) in per_old {
        let unique = match candidates.as_slice() {
            [(new_key, 1)] => Some(*new_key),
            _ => None,
        };
        if let Some(new_key) = unique {
            moved += move_anchors(pool, old_key, new_key).await?;
        }
        sqlx::query(
            "UPDATE graph_node_versions SET valid_to = ? WHERE node_key = ? AND valid_to IS NULL",
        )
        .bind(now_ms)
        .bind(old_key)
        .execute(pool)
        .await
        .map_err(|e| db_err(pool, e))?;
    }
    Ok(moved)
}

async fn move_anchors(pool: &SqlitePool, old_key: &str, new_key: &str) -> Result<usize, AxError> {
    let anchors: Vec<(String, i64)> =
        sqlx::query_as("SELECT memory_id, version FROM memory_anchors WHERE node_key = ?")
            .bind(old_key)
            .fetch_all(pool)
            .await
            .map_err(|e| db_err(pool, e))?;
    let mut moved = 0usize;
    for (memory_id, version) in anchors {
        let inserted = anchor_memory(pool, &memory_id, new_key, version).await?;
        sqlx::query(
            "DELETE FROM memory_anchors WHERE memory_id = ? AND node_key = ? AND version = ?",
        )
        .bind(&memory_id)
        .bind(old_key)
        .bind(version)
        .execute(pool)
        .await
        .map_err(|e| db_err(pool, e))?;
        if inserted {
            moved += 1;
        }
    }
    if moved > 0 {
        crate::store::link_memories(
            pool,
            &format!("node:{old_key}"),
            &format!("node:{new_key}"),
            "renamed",
        )
        .await?;
    }
    Ok(moved)
}
