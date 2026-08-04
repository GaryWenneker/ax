//! Shared memory vault export/import for team git sync.
//!
//! Opt-in flow:
//! 1. Tag memories with `shared` (or pass `--tag`)
//! 2. `ax memory export` writes `.ax/memory/shared.jsonl`
//! 3. Commit the file; teammates run `ax memory import` (or post-merge hook)

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

use crate::store::{get, list, update};
use crate::types::{MemoryRow, RememberInput};

fn db_err(e: impl std::fmt::Display) -> AxError {
    AxError::Database(DatabaseError::new(e.to_string()))
}

pub fn default_shared_path(project_root: &Path) -> PathBuf {
    project_root.join(".ax").join("memory").join("shared.jsonl")
}

#[derive(Debug, Default)]
pub struct MemoryExportResult {
    pub written: usize,
    pub path: PathBuf,
}

#[derive(Debug, Default)]
pub struct MemoryImportResult {
    pub inserted: usize,
    pub updated: usize,
    pub skipped: usize,
}

/// Export memories that carry `tag` (default: `shared`) to JSONL.
pub async fn export_shared(
    pool: &SqlitePool,
    project_root: &Path,
    tag: &str,
    out: Option<&Path>,
) -> Result<MemoryExportResult, AxError> {
    let out_path = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_shared_path(project_root));
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AxError::Other(e.to_string()))?;
    }

    let (rows, _) = list(pool, 10_000, 0).await?;
    let tag_l = tag.to_ascii_lowercase();
    let selected: Vec<&MemoryRow> = rows
        .iter()
        .filter(|m| {
            m.tags
                .iter()
                .any(|t| t.eq_ignore_ascii_case(&tag_l))
        })
        .collect();

    let mut body = String::new();
    for m in &selected {
        let line = serde_json::to_string(m).map_err(|e| AxError::Other(e.to_string()))?;
        body.push_str(&line);
        body.push('\n');
    }
    std::fs::write(&out_path, body).map_err(|e| AxError::Other(e.to_string()))?;

    Ok(MemoryExportResult {
        written: selected.len(),
        path: out_path,
    })
}

/// Import JSONL memories. Existing ids are updated; new ids are inserted.
pub async fn import_shared(
    pool: &SqlitePool,
    path: &Path,
) -> Result<MemoryImportResult, AxError> {
    if !path.is_file() {
        return Ok(MemoryImportResult::default());
    }
    let content = std::fs::read_to_string(path).map_err(|e| AxError::Other(e.to_string()))?;
    let mut result = MemoryImportResult::default();

    for (lineno, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row: MemoryRow = serde_json::from_str(line).map_err(|e| {
            AxError::Other(format!("shared.jsonl line {}: {e}", lineno + 1))
        })?;
        match get(pool, &row.id).await? {
            Some(existing) => {
                if existing.updated_at >= row.updated_at
                    && existing.body == row.body
                    && existing.title == row.title
                {
                    result.skipped += 1;
                    continue;
                }
                let ok = update(
                    pool,
                    &row.id,
                    RememberInput {
                        title: row.title,
                        body: row.body,
                        kind: Some(row.kind),
                        tags: row.tags,
                        files: row.files,
                        source: Some("git-sync".into()),
                    },
                )
                .await?;
                if ok {
                    result.updated += 1;
                } else {
                    result.skipped += 1;
                }
            }
            None => {
                // Preserve original id when inserting.
                insert_with_id(pool, &row).await?;
                result.inserted += 1;
            }
        }
    }
    Ok(result)
}

async fn insert_with_id(pool: &SqlitePool, row: &MemoryRow) -> Result<(), AxError> {
    let embedding = crate::embed::embedding_to_blob(&crate::embed::embed_text(&format!(
        "{} {} {}",
        row.title,
        row.body,
        row.tags.join(" ")
    )));
    sqlx::query(
        r#"INSERT INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, embedding, enabled)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&row.id)
    .bind(&row.kind)
    .bind(&row.title)
    .bind(&row.body)
    .bind(serde_json::to_string(&row.tags).unwrap_or_else(|_| "[]".into()))
    .bind(serde_json::to_string(&row.files).unwrap_or_else(|_| "[]".into()))
    .bind(row.confidence)
    .bind("git-sync")
    .bind(row.created_at)
    .bind(row.updated_at)
    .bind(embedding)
    .bind(if row.enabled { 1 } else { 0 })
    .execute(pool)
    .await
    .map_err(db_err)?;
    Ok(())
}

/// True when project `ax.json` has `"memorySync": true`.
pub fn memory_sync_enabled(project_root: &Path) -> bool {
    for name in ["ax.json", ".ax.json"] {
        let path = project_root.join(name);
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };
        if v.get("memorySync").and_then(|x| x.as_bool()) == Some(true) {
            return true;
        }
    }
    false
}

/// Convenience for hooks: export then re-import (idempotent).
pub async fn sync_shared_roundtrip(
    pool: &SqlitePool,
    project_root: &Path,
) -> Result<(MemoryExportResult, MemoryImportResult), AxError> {
    let exported = export_shared(pool, project_root, "shared", None).await?;
    let imported = import_shared(pool, &exported.path).await?;
    Ok((exported, imported))
}