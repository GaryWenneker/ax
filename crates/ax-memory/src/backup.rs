//! Append-only full memory backups under `.ax/backups/memory/`.

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use ax_utils::errors::AxError;

use crate::store::{list, remember_with_id};
use crate::types::{MemoryRow, RememberInput};

#[derive(Debug, Clone)]
pub struct BackupResult {
    pub path: PathBuf,
    pub manifest_path: PathBuf,
    pub count: usize,
    pub checksum: String,
}

fn backup_dir(project_root: &Path) -> PathBuf {
    project_root.join(".ax").join("backups").join("memory")
}

/// Write every memory as one JSON object per line, plus a manifest with count and checksum.
pub async fn write_full_backup(
    pool: &SqlitePool,
    project_root: &Path,
    now_ms: i64,
) -> Result<BackupResult, AxError> {
    let dir = backup_dir(project_root);
    std::fs::create_dir_all(&dir).map_err(|e| AxError::Other(e.to_string()))?;
    let stem = format!("full-{now_ms}");
    let path = dir.join(format!("{stem}.jsonl.zst"));
    let manifest_path = dir.join(format!("{stem}.manifest.json"));

    let (rows, _) = list(pool, 1_000_000, 0).await?;
    let mut body = String::new();
    for row in &rows {
        let line = serde_json::to_string(row).map_err(|e| AxError::Other(e.to_string()))?;
        body.push_str(&line);
        body.push('\n');
    }
    let plain = body.into_bytes();
    let checksum = blake3::hash(&plain).to_hex().to_string();
    let compressed =
        zstd::encode_all(plain.as_slice(), 3).map_err(|e| AxError::Other(e.to_string()))?;
    std::fs::write(&path, &compressed).map_err(|e| AxError::Other(e.to_string()))?;
    let manifest = serde_json::json!({
        "kind": "full",
        "encoding": "zstd",
        "count": rows.len(),
        "checksum": checksum,
        "created_at": now_ms,
        "file": path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
    });
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).map_err(|e| AxError::Other(e.to_string()))?,
    )
    .map_err(|e| AxError::Other(e.to_string()))?;

    Ok(BackupResult {
        path,
        manifest_path,
        count: rows.len(),
        checksum,
    })
}

#[allow(dead_code)]
fn _row_shape(row: &MemoryRow) -> &str {
    &row.id
}

/// Daily incremental: memories updated since `since_ms` (exclusive).
pub async fn write_incremental_backup(
    pool: &SqlitePool,
    project_root: &Path,
    since_ms: i64,
    now_ms: i64,
) -> Result<BackupResult, AxError> {
    let dir = backup_dir(project_root);
    std::fs::create_dir_all(&dir).map_err(|e| AxError::Other(e.to_string()))?;
    let stem = format!("incr-{now_ms}");
    let path = dir.join(format!("{stem}.jsonl.zst"));
    let manifest_path = dir.join(format!("{stem}.manifest.json"));
    let (rows, _) = list(pool, 1_000_000, 0).await?;
    let changed: Vec<_> = rows
        .into_iter()
        .filter(|r| r.updated_at > since_ms)
        .collect();
    let mut body = String::new();
    for row in &changed {
        let line = serde_json::to_string(row).map_err(|e| AxError::Other(e.to_string()))?;
        body.push_str(&line);
        body.push('\n');
    }
    let plain = body.into_bytes();
    let checksum = blake3::hash(&plain).to_hex().to_string();
    let compressed =
        zstd::encode_all(plain.as_slice(), 3).map_err(|e| AxError::Other(e.to_string()))?;
    std::fs::write(&path, &compressed).map_err(|e| AxError::Other(e.to_string()))?;
    let manifest = serde_json::json!({
        "kind": "incremental",
        "encoding": "zstd",
        "since": since_ms,
        "count": changed.len(),
        "checksum": checksum,
        "created_at": now_ms,
        "file": path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
    });
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).map_err(|e| AxError::Other(e.to_string()))?,
    )
    .map_err(|e| AxError::Other(e.to_string()))?;
    Ok(BackupResult {
        path,
        manifest_path,
        count: changed.len(),
        checksum,
    })
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    pub restored: usize,
    pub skipped_existing: usize,
    pub skipped_tombstone: usize,
}

fn read_backup_text(path: &Path) -> Result<String, AxError> {
    let bytes = std::fs::read(path).map_err(|e| AxError::Other(e.to_string()))?;
    let plain = if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == "zst")
    {
        zstd::decode_all(bytes.as_slice()).map_err(|e| AxError::Other(e.to_string()))?
    } else {
        bytes
    };
    String::from_utf8(plain).map_err(|e| AxError::Other(e.to_string()))
}

pub async fn restore_from_backup(
    pool: &SqlitePool,
    jsonl_path: &Path,
) -> Result<RestoreReport, AxError> {
    let text = read_backup_text(jsonl_path)?;
    let mut report = RestoreReport::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row: MemoryRow =
            serde_json::from_str(line).map_err(|e| AxError::Other(e.to_string()))?;
        let tomb: Option<String> =
            sqlx::query_scalar("SELECT memory_id FROM memory_tombstones WHERE memory_id = ?")
                .bind(&row.id)
                .fetch_optional(pool)
                .await
                .map_err(|e| AxError::Other(e.to_string()))?;
        if tomb.is_some() {
            report.skipped_tombstone += 1;
            continue;
        }
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM memories WHERE id = ?")
            .bind(&row.id)
            .fetch_optional(pool)
            .await
            .map_err(|e| AxError::Other(e.to_string()))?;
        if exists.is_some() {
            report.skipped_existing += 1;
            continue;
        }
        remember_with_id(
            pool,
            row.id.clone(),
            RememberInput {
                title: row.title,
                body: row.body,
                kind: Some(row.kind),
                tags: row.tags,
                files: row.files,
                source: Some(row.source),
            },
        )
        .await?;
        report.restored += 1;
    }
    Ok(report)
}
