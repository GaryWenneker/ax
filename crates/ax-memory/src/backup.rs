//! Append-only memory backups under `.ax/backups/memory/`.
//!
//! Each backup is a zstd-compressed JSONL file (one memory per line) plus a
//! manifest with the row count and the blake3 checksum of the uncompressed payload.
//! Both files are written to a temporary file first and renamed into place.

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use ax_utils::errors::AxError;

use crate::store::{backup_page, remember_with_id};
use crate::types::{MemoryRow, RememberInput};

#[derive(Debug, Clone)]
pub struct BackupResult {
    pub path: PathBuf,
    pub manifest_path: PathBuf,
    pub count: usize,
    pub checksum: String,
}

const JSONL_SUFFIX: &str = ".jsonl.zst";
const MANIFEST_SUFFIX: &str = ".manifest.json";
const PAGE: usize = 1000;

fn other(e: impl std::fmt::Display) -> AxError {
    AxError::Other(e.to_string())
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
    write_backup(pool, project_root, now_ms, None).await
}

/// Daily incremental: memories updated since `since_ms` (exclusive).
pub async fn write_incremental_backup(
    pool: &SqlitePool,
    project_root: &Path,
    since_ms: i64,
    now_ms: i64,
) -> Result<BackupResult, AxError> {
    write_backup(pool, project_root, now_ms, Some(since_ms)).await
}

async fn write_backup(
    pool: &SqlitePool,
    project_root: &Path,
    now_ms: i64,
    since_ms: Option<i64>,
) -> Result<BackupResult, AxError> {
    let dir = backup_dir(project_root);
    std::fs::create_dir_all(&dir).map_err(other)?;
    let stem = match since_ms {
        Some(_) => format!("incr-{now_ms}"),
        None => format!("full-{now_ms}"),
    };
    let path = dir.join(format!("{stem}{JSONL_SUFFIX}"));
    let manifest_path = dir.join(format!("{stem}{MANIFEST_SUFFIX}"));

    let (body, count) = collect_rows(pool, since_ms, PAGE).await?;

    let plain = body.into_bytes();
    let checksum = blake3::hash(&plain).to_hex().to_string();
    let compressed = zstd::encode_all(plain.as_slice(), 3).map_err(other)?;
    ax_utils::atomic_write(&path, &compressed).map_err(other)?;
    let mut manifest = serde_json::json!({
        "kind": if since_ms.is_some() { "incremental" } else { "full" },
        "encoding": "zstd",
        "count": count,
        "checksum": checksum,
        "created_at": now_ms,
        "file": path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
    });
    if let Some(since) = since_ms {
        manifest["since"] = since.into();
    }
    ax_utils::atomic_write(
        &manifest_path,
        &serde_json::to_vec_pretty(&manifest).map_err(other)?,
    )
    .map_err(other)?;

    Ok(BackupResult {
        path,
        manifest_path,
        count,
        checksum,
    })
}

/// Every memory updated after `since` (all of them when `None`) as JSONL,
/// newest first, and the row count. The pages are read inside one read
/// transaction, so the backup is one consistent snapshot of the vault even
/// when memories are written while it runs.
async fn collect_rows(
    pool: &SqlitePool,
    since: Option<i64>,
    page: usize,
) -> Result<(String, usize), AxError> {
    let mut tx = pool.begin().await.map_err(other)?;
    let mut body = String::new();
    let mut count = 0usize;
    let mut after: Option<(i64, String)> = None;
    loop {
        let cursor = after.as_ref().map(|(at, id)| (*at, id.as_str()));
        let rows = backup_page(&mut *tx, cursor, since, page).await?;
        for row in &rows {
            body.push_str(&serde_json::to_string(row).map_err(other)?);
            body.push('\n');
            count += 1;
        }
        if rows.len() < page {
            break;
        }
        after = rows.last().map(|row| (row.updated_at, row.id.clone()));
    }
    tx.commit().await.map_err(other)?;
    Ok((body, count))
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    pub restored: usize,
    pub skipped_existing: usize,
    pub skipped_tombstone: usize,
}

fn read_backup_bytes(path: &Path) -> Result<Vec<u8>, AxError> {
    let bytes = std::fs::read(path).map_err(other)?;
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == "zst")
    {
        zstd::decode_all(bytes.as_slice()).map_err(other)
    } else {
        Ok(bytes)
    }
}

/// The manifest written next to a backup: `<stem>.manifest.json` for `<stem>.jsonl[.zst]`.
fn manifest_path_for(jsonl_path: &Path) -> Option<PathBuf> {
    let name = jsonl_path.file_name()?.to_str()?;
    let stem = name
        .strip_suffix(JSONL_SUFFIX)
        .or_else(|| name.strip_suffix(".jsonl"))?;
    Some(jsonl_path.with_file_name(format!("{stem}{MANIFEST_SUFFIX}")))
}

/// Read a backup and check it before anything is written: every line must parse.
///
/// A `*.jsonl.zst` backup (written by [`write_full_backup`] or
/// [`write_incremental_backup`]) must also match its manifest: the blake3
/// checksum and the row count. A plain `*.jsonl` file without a manifest, such
/// as the turn-memory copy `prune_turns` appends to, is checked line by line only.
fn read_verified_rows(jsonl_path: &Path) -> Result<Vec<MemoryRow>, AxError> {
    let manifest_path = manifest_path_for(jsonl_path).ok_or_else(|| {
        other(format!(
            "{} is not a memory backup file (*{JSONL_SUFFIX} or *.jsonl)",
            jsonl_path.display()
        ))
    })?;
    let compressed = jsonl_path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(JSONL_SUFFIX));
    let manifest: Option<serde_json::Value> = if compressed || manifest_path.exists() {
        Some(
            std::fs::read(&manifest_path)
                .map_err(|e| other(format!("backup manifest {}: {e}", manifest_path.display())))
                .and_then(|b| serde_json::from_slice(&b).map_err(other))?,
        )
    } else {
        None
    };
    let plain = read_backup_bytes(jsonl_path)?;
    if let Some(manifest) = &manifest {
        let checksum = blake3::hash(&plain).to_hex().to_string();
        if manifest["checksum"].as_str() != Some(checksum.as_str()) {
            return Err(other(format!(
                "backup {} does not match the checksum in its manifest; nothing restored",
                jsonl_path.display()
            )));
        }
    }
    let text = String::from_utf8(plain).map_err(other)?;
    let mut rows = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row: MemoryRow = serde_json::from_str(line).map_err(|e| {
            other(format!(
                "backup {} line {}: {e}; nothing restored",
                jsonl_path.display(),
                n + 1
            ))
        })?;
        rows.push(row);
    }
    if let Some(manifest) = manifest.filter(|m| m["count"].as_u64() != Some(rows.len() as u64)) {
        return Err(other(format!(
            "backup {} holds {} rows but its manifest says {}; nothing restored",
            jsonl_path.display(),
            rows.len(),
            manifest["count"]
        )));
    }
    Ok(rows)
}

/// Restore memories from a backup written by [`write_full_backup`] or
/// [`write_incremental_backup`], or from a turn-memory copy written by `prune_turns`.
///
/// The file is checked first (against its manifest when it has one); on any
/// mismatch or parse error nothing is written. Ids that already exist or were purged (tombstoned) are
/// skipped, so running a restore again after an interruption completes it.
/// Restored rows keep their original timestamps, confidence, enabled flag and tier;
/// they are not stamped with the current session or turn. A row restored into the
/// archive tier gets its archive vector back.
pub async fn restore_from_backup(
    pool: &SqlitePool,
    jsonl_path: &Path,
) -> Result<RestoreReport, AxError> {
    let rows = read_verified_rows(jsonl_path)?;
    let mut report = RestoreReport::default();
    for row in rows {
        let tomb: Option<String> =
            sqlx::query_scalar("SELECT memory_id FROM memory_tombstones WHERE memory_id = ?")
                .bind(&row.id)
                .fetch_optional(pool)
                .await
                .map_err(other)?;
        if tomb.is_some() {
            report.skipped_tombstone += 1;
            continue;
        }
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM memories WHERE id = ?")
            .bind(&row.id)
            .fetch_optional(pool)
            .await
            .map_err(other)?;
        if exists.is_some() {
            report.skipped_existing += 1;
            continue;
        }
        remember_with_id(
            pool,
            row.id.clone(),
            RememberInput {
                title: row.title.clone(),
                body: row.body.clone(),
                kind: Some(row.kind.clone()),
                tags: row.tags.clone(),
                files: row.files.clone(),
                source: Some(row.source.clone()),
            },
        )
        .await?;
        sqlx::query(
            "UPDATE memories SET created_at = ?, updated_at = ?, confidence = ?, enabled = ?,
                    tier = ?, session_id = '', turn_id = ''
             WHERE id = ?",
        )
        .bind(row.created_at)
        .bind(row.updated_at)
        .bind(row.confidence)
        .bind(if row.enabled { 1 } else { 0 })
        .bind(&row.tier)
        .bind(&row.id)
        .execute(pool)
        .await
        .map_err(other)?;
        if row.tier == crate::brain::TIER_ARCHIVE {
            // Archive recall searches by vector as well as by text.
            crate::brain::index_archive_vector(pool, &row.id).await?;
        }
        report.restored += 1;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn vault_with_tied_rows(n: usize) -> (tempfile::TempDir, ax_db::Database) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ax")).unwrap();
        let db = ax_db::Database::open(&dir.path().join(".ax/ax.db"))
            .await
            .unwrap();
        for i in 0..n {
            crate::store::remember(
                db.pool(),
                RememberInput {
                    title: format!("tied {i}"),
                    body: "body".into(),
                    kind: Some("note".into()),
                    ..RememberInput::default()
                },
            )
            .await
            .unwrap();
        }
        // Bulk captures and imports often share one timestamp.
        sqlx::query("UPDATE memories SET updated_at = 100")
            .execute(db.pool())
            .await
            .unwrap();
        (dir, db)
    }

    fn ids(body: &str) -> Vec<String> {
        body.lines()
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).unwrap()["id"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    }

    #[tokio::test]
    async fn paging_through_tied_timestamps_reads_every_row_once() {
        let (_dir, db) = vault_with_tied_rows(7).await;
        let (body, count) = collect_rows(db.pool(), None, 2).await.unwrap();
        let mut seen = ids(&body);
        db.close().await;
        assert_eq!(count, 7);
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 7, "a row was read twice or skipped");
    }

    #[tokio::test]
    async fn a_row_updated_between_pages_does_not_shift_later_pages() {
        let (_dir, db) = vault_with_tied_rows(5).await;
        let mut conn = db.pool().acquire().await.unwrap();
        let first = backup_page(&mut *conn, None, None, 2).await.unwrap();
        // A hook updates a row the backup already read: it moves to the top.
        sqlx::query("UPDATE memories SET updated_at = 500 WHERE id = ?")
            .bind(&first[0].id)
            .execute(db.pool())
            .await
            .unwrap();
        let mut seen: Vec<String> = first.iter().map(|r| r.id.clone()).collect();
        let mut after = first.last().map(|r| (r.updated_at, r.id.clone()));
        loop {
            let cursor = after.as_ref().map(|(at, id)| (*at, id.as_str()));
            let rows = backup_page(&mut *conn, cursor, None, 2).await.unwrap();
            seen.extend(rows.iter().map(|r| r.id.clone()));
            if rows.len() < 2 {
                break;
            }
            after = rows.last().map(|r| (r.updated_at, r.id.clone()));
        }
        drop(conn);
        db.close().await;
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 5, "a row was skipped after another moved up");
    }
}
