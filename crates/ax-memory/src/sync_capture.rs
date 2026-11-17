//! Capture file_change memories when indexed content changes.

use std::path::Path;

use sqlx::SqlitePool;

use ax_utils::errors::AxError;

use crate::turns::save_file_changes;

/// When `previous_hash` differs from `new_hash`, write one file_change memory.
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
    save_file_changes(pool, &[path.to_string()], session_id, turn_id, now_ms).await
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

/// After sync/index, write file_change memories for every affected path.
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
        // Paths reaching finalize already changed; pass a sentinel new hash.
        written +=
            capture_sync_file_change(pool, path, None, "changed", &session_id, &turn_id, now_ms)
                .await?;
    }
    Ok(written)
}

/// Record graph node versions for nodes whose file_path is in `paths`.
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
        .map_err(|e| AxError::Other(e.to_string()))?;
        for (id, name, kind, signature, start, end) in rows {
            let body = format!("{name}\n{}", signature.clone().unwrap_or_default());
            let latest: Option<String> = sqlx::query_scalar(
                "SELECT body_hash FROM graph_node_versions
                 WHERE node_key = ? AND valid_to IS NULL
                 ORDER BY version DESC LIMIT 1",
            )
            .bind(&id)
            .fetch_optional(pool)
            .await
            .map_err(|e| AxError::Other(e.to_string()))?;
            let body_hash = blake3::hash(body.as_bytes()).to_hex().to_string();
            if latest.as_deref() == Some(body_hash.as_str()) {
                continue;
            }
            let ok = ax_db::graph_history::record_node_version(
                pool,
                &ax_db::graph_history::GraphNodeVersion {
                    node_key: id,
                    body,
                    signature: signature.unwrap_or_default(),
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
