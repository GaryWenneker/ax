//! Capture file_change memories when indexed content changes.

use std::path::Path;

use sqlx::SqlitePool;

use ax_utils::errors::AxError;

use crate::turns::save_file_change_versions;

/// When `previous_hash` differs from `new_hash`, write one file_change memory
/// for that path, turn and `new_hash`.
pub async fn capture_sync_file_change(
    pool: &SqlitePool,
    path: &str,
    previous_hash: Option<&str>,
    new_hash: &str,
    session_id: &str,
    turn_id: &str,
    now_ms: i64,
) -> Result<usize, AxError> {
    let path = path.trim();
    if path.is_empty() {
        return Ok(0);
    }
    let new_hash = new_hash.trim();
    if new_hash.is_empty() {
        return Ok(0);
    }
    if previous_hash.map(str::trim) == Some(new_hash) {
        return Ok(0);
    }
    save_file_change_versions(
        pool,
        &[path.to_string()],
        new_hash,
        session_id,
        turn_id,
        now_ms,
    )
    .await
}

/// Read `.ax/turns/current.json` next to the database when present.
pub fn current_session_turn(project_root: &Path) -> (String, String) {
    let path = project_root.join(".ax").join("turns").join("current.json");
    let Ok(text) = std::fs::read_to_string(path) else {
        return ("sync".into(), "index".into());
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return ("sync".into(), "index".into());
    };
    let session = value
        .get("session_id")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .trim();
    let turn = value
        .get("turn_id")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .trim();
    if session.is_empty() || turn.is_empty() {
        return ("sync".into(), "index".into());
    }
    (session.to_string(), turn.to_string())
}

/// After sync/index, write a file_change memory for every affected path whose
/// stored content is new for this turn. The content hash comes from
/// `file_contents`; a path without stored content (deleted) is recorded once
/// per turn.
pub async fn capture_affected_paths(
    pool: &SqlitePool,
    project_root: &Path,
    paths: &[String],
    now_ms: i64,
) -> Result<usize, AxError> {
    if paths.is_empty() {
        return Ok(0);
    }
    let (session_id, turn_id) = current_session_turn(project_root);
    let mut written = 0usize;
    for path in paths {
        let content_hash: Option<String> =
            sqlx::query_scalar("SELECT content_hash FROM file_contents WHERE path = ?")
                .bind(path.trim())
                .fetch_optional(pool)
                .await
                .map_err(|e| AxError::Other(e.to_string()))?;
        let version = content_hash.unwrap_or_else(|| "missing".into());
        written +=
            capture_sync_file_change(pool, path, None, &version, &session_id, &turn_id, now_ms)
                .await?;
    }
    Ok(written)
}

/// Lines `start_line..=end_line` (1-based, inclusive) of `content`.
fn source_slice(content: &str, start_line: i64, end_line: i64) -> String {
    let start = start_line.max(1) as usize - 1;
    let end = end_line.max(0) as usize;
    if end <= start {
        return String::new();
    }
    content
        .lines()
        .skip(start)
        .take(end - start)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The hash older versions were recorded with: name and signature only.
fn legacy_body(name: &str, signature: &str) -> String {
    format!("{name}\n{signature}")
}

fn hash(body: &str) -> String {
    blake3::hash(body.as_bytes()).to_hex().to_string()
}

fn other(e: impl std::fmt::Display) -> AxError {
    AxError::Other(e.to_string())
}

/// Record graph node versions for nodes whose file_path is in `paths`.
///
/// A version's body is the node's source text (its lines in the stored file
/// content), so a rewritten body with the same signature is a new version and
/// anchors on the old one go stale. Without stored content the body falls back
/// to name and signature. An open version recorded with that fallback hash is
/// upgraded in place when the signature still matches, so switching to source
/// hashes does not flag every anchor as stale.
pub async fn record_versions_for_paths(
    pool: &SqlitePool,
    project_root: &Path,
    paths: &[String],
    now_ms: i64,
) -> Result<usize, AxError> {
    if paths.is_empty() {
        return Ok(0);
    }
    let (session_id, turn_id) = current_session_turn(project_root);
    let mut written = 0usize;
    for path in paths {
        let path = path.trim();
        if path.is_empty() {
            continue;
        }
        let rows: Vec<(String, String, String, Option<String>, i64, i64)> = sqlx::query_as(
            "SELECT id, name, kind, signature, start_line, end_line FROM nodes WHERE file_path = ?",
        )
        .bind(path)
        .fetch_all(pool)
        .await
        .map_err(other)?;
        if rows.is_empty() {
            continue;
        }
        let content: Option<String> =
            sqlx::query_scalar("SELECT content FROM file_contents WHERE path = ?")
                .bind(path)
                .fetch_optional(pool)
                .await
                .map_err(other)?;
        // Latest open version of every node of this file, in one query.
        let open: Vec<(String, i64, String)> = sqlx::query_as(
            "SELECT v.node_key, v.version, v.body_hash FROM graph_node_versions v
             WHERE v.file_path = ? AND v.valid_to IS NULL
               AND v.version = (SELECT MAX(l.version) FROM graph_node_versions l
                                WHERE l.node_key = v.node_key AND l.valid_to IS NULL)",
        )
        .bind(path)
        .fetch_all(pool)
        .await
        .map_err(other)?;
        let open: std::collections::HashMap<String, (i64, String)> = open
            .into_iter()
            .map(|(key, version, body_hash)| (key, (version, body_hash)))
            .collect();
        for (id, name, kind, signature, start, end) in rows {
            let signature = signature.unwrap_or_default();
            let legacy = legacy_body(&name, &signature);
            let body = content
                .as_deref()
                .map(|text| source_slice(text, start, end))
                .filter(|slice| !slice.trim().is_empty())
                .unwrap_or_else(|| legacy.clone());
            let body_hash = hash(&body);
            let latest = match open.get(&id) {
                Some(found) => Some(found.clone()),
                // A node can move files; its open version is then under the old path.
                None => sqlx::query_as::<_, (i64, String)>(
                    "SELECT version, body_hash FROM graph_node_versions
                     WHERE node_key = ? AND valid_to IS NULL
                     ORDER BY version DESC LIMIT 1",
                )
                .bind(&id)
                .fetch_optional(pool)
                .await
                .map_err(other)?,
            };
            if let Some((version, latest_hash)) = &latest {
                if *latest_hash == body_hash {
                    continue;
                }
                if body != legacy && *latest_hash == hash(&legacy) {
                    upgrade_legacy_hash(pool, &id, *version, &body, &body_hash).await?;
                    continue;
                }
            }
            let ok = ax_db::graph_history::record_node_version(
                pool,
                &ax_db::graph_history::GraphNodeVersion {
                    node_key: id,
                    body,
                    signature,
                    kind,
                    file_path: path.to_string(),
                    start_line: start,
                    end_line: end,
                    valid_from: now_ms,
                    commit_id: String::new(),
                    session_id: session_id.clone(),
                    turn_id: turn_id.clone(),
                },
            )
            .await?;
            if ok {
                written += 1;
            }
        }
    }
    Ok(written)
}

/// Replace the name-and-signature hash of an open version with the hash of
/// its source text, keeping the version number anchors point at.
async fn upgrade_legacy_hash(
    pool: &SqlitePool,
    node_key: &str,
    version: i64,
    body: &str,
    body_hash: &str,
) -> Result<(), AxError> {
    let mut tx = pool.begin().await.map_err(other)?;
    sqlx::query("INSERT OR IGNORE INTO graph_body_blobs (body_hash, body) VALUES (?, ?)")
        .bind(body_hash)
        .bind(body.as_bytes())
        .execute(&mut *tx)
        .await
        .map_err(other)?;
    sqlx::query("UPDATE graph_node_versions SET body_hash = ? WHERE node_key = ? AND version = ?")
        .bind(body_hash)
        .bind(node_key)
        .bind(version)
        .execute(&mut *tx)
        .await
        .map_err(other)?;
    tx.commit().await.map_err(other)
}
