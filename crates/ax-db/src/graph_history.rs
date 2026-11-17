use ax_utils::errors::{AxError, DatabaseError};
use sqlx::SqlitePool;

pub struct GraphNodeVersion {
    pub node_key: String,
    pub body: String,
    pub signature: String,
    pub kind: String,
    pub file_path: String,
    pub start_line: i64,
    pub end_line: i64,
    pub valid_from: i64,
    pub commit_id: String,
    pub session_id: String,
    pub turn_id: String,
}

pub struct GraphEdgeVersion {
    pub source_key: String,
    pub target_key: String,
    pub kind: String,
    pub valid_from: i64,
    pub commit_id: String,
    pub session_id: String,
    pub turn_id: String,
}

fn db_err(e: sqlx::Error) -> AxError {
    AxError::Database(DatabaseError::new(e.to_string()))
}

pub async fn record_node_version(
    pool: &SqlitePool,
    version: &GraphNodeVersion,
) -> Result<bool, AxError> {
    let node_key = version.node_key.trim();
    if node_key.is_empty() {
        return Ok(false);
    }
    let body_hash = blake3::hash(version.body.as_bytes()).to_hex().to_string();
    sqlx::query("INSERT OR IGNORE INTO graph_body_blobs (body_hash, body) VALUES (?, ?)")
        .bind(&body_hash)
        .bind(version.body.as_bytes())
        .execute(pool)
        .await
        .map_err(db_err)?;
    let next: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version), 0) + 1 FROM graph_node_versions WHERE node_key = ?",
    )
    .bind(node_key)
    .fetch_one(pool)
    .await
    .map_err(db_err)?;
    if next > 1 {
        sqlx::query(
            "UPDATE graph_node_versions SET valid_to = ? WHERE node_key = ? AND version = ? AND valid_to IS NULL",
        )
        .bind(version.valid_from)
        .bind(node_key)
        .bind(next - 1)
        .execute(pool)
        .await
        .map_err(db_err)?;
    }
    let result = sqlx::query(
        "INSERT INTO graph_node_versions
         (node_key, version, body_hash, signature, kind, file_path, start_line, end_line,
          valid_from, commit_id, session_id, turn_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(node_key)
    .bind(next)
    .bind(&body_hash)
    .bind(&version.signature)
    .bind(&version.kind)
    .bind(&version.file_path)
    .bind(version.start_line)
    .bind(version.end_line)
    .bind(version.valid_from)
    .bind(&version.commit_id)
    .bind(&version.session_id)
    .bind(&version.turn_id)
    .execute(pool)
    .await
    .map_err(db_err)?;
    Ok(result.rows_affected() == 1)
}

pub async fn record_edge_version(
    pool: &SqlitePool,
    version: &GraphEdgeVersion,
) -> Result<bool, AxError> {
    let source_key = version.source_key.trim();
    let target_key = version.target_key.trim();
    let kind = version.kind.trim();
    if source_key.is_empty() || target_key.is_empty() || kind.is_empty() {
        return Ok(false);
    }
    let next: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version), 0) + 1 FROM graph_edge_versions
         WHERE source_key = ? AND target_key = ? AND kind = ?",
    )
    .bind(source_key)
    .bind(target_key)
    .bind(kind)
    .fetch_one(pool)
    .await
    .map_err(db_err)?;
    if next > 1 {
        sqlx::query(
            "UPDATE graph_edge_versions SET valid_to = ?
             WHERE source_key = ? AND target_key = ? AND kind = ? AND version = ? AND valid_to IS NULL",
        )
        .bind(version.valid_from)
        .bind(source_key)
        .bind(target_key)
        .bind(kind)
        .bind(next - 1)
        .execute(pool)
        .await
        .map_err(db_err)?;
    }
    let result = sqlx::query(
        "INSERT INTO graph_edge_versions
         (source_key, target_key, kind, version, valid_from, commit_id, session_id, turn_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(source_key)
    .bind(target_key)
    .bind(kind)
    .bind(next)
    .bind(version.valid_from)
    .bind(&version.commit_id)
    .bind(&version.session_id)
    .bind(&version.turn_id)
    .execute(pool)
    .await
    .map_err(db_err)?;
    Ok(result.rows_affected() == 1)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNodeAsOf {
    pub node_key: String,
    pub version: i64,
    pub body_hash: String,
    pub valid_from: i64,
    pub valid_to: Option<i64>,
}

pub async fn node_as_of(
    pool: &SqlitePool,
    node_key: &str,
    at_ms: i64,
) -> Result<Option<GraphNodeAsOf>, AxError> {
    let node_key = node_key.trim();
    if node_key.is_empty() {
        return Ok(None);
    }
    let row: Option<(i64, String, i64, Option<i64>)> = sqlx::query_as(
        "SELECT version, body_hash, valid_from, valid_to FROM graph_node_versions
         WHERE node_key = ? AND valid_from <= ?
           AND (valid_to IS NULL OR valid_to > ?)
         ORDER BY version DESC LIMIT 1",
    )
    .bind(node_key)
    .bind(at_ms)
    .bind(at_ms)
    .fetch_optional(pool)
    .await
    .map_err(db_err)?;
    Ok(
        row.map(|(version, body_hash, valid_from, valid_to)| GraphNodeAsOf {
            node_key: node_key.to_string(),
            version,
            body_hash,
            valid_from,
            valid_to,
        }),
    )
}
