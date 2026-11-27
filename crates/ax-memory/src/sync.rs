//! Shared memory vault export/import for team git sync.
//!
//! Opt-in flow:
//! 1. Tag memories with `shared` (or pass `--tag`)
//! 2. `ax memory export` writes `.ax/memory/shared.jsonl`
//! 3. Commit the file; teammates run `ax memory import` (or post-merge hook)

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

use crate::store::{enabled_with_tag, get, update};
use crate::types::{MemoryRow, RememberInput};

fn db_err(e: impl std::fmt::Display) -> AxError {
    AxError::Database(DatabaseError::new(e.to_string()))
}

pub fn default_shared_path(project_root: &Path) -> PathBuf {
    project_root.join(".ax").join("memory").join("shared.jsonl")
}

/// Export requires explicit consent via a matching tag and rejects all local,
/// private, disabled and recall-only records even if they also carry that tag.
pub fn is_shared_export_candidate(memory: &MemoryRow, tag: &str) -> bool {
    memory.enabled
        && !crate::folder_sync::is_recall_only(&memory.kind)
        && memory.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
        && !memory.tags.iter().any(|t| {
            t.eq_ignore_ascii_case("local")
                || t.eq_ignore_ascii_case("noshare")
                || t.eq_ignore_ascii_case("private")
        })
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

/// Export every memory that carries `tag` (default: `shared`) to JSONL,
/// ordered by id so the committed file diffs cleanly. The file is replaced
/// atomically: readers see the old or the new export, never a partial one.
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

    let rows = enabled_with_tag(pool, tag).await?;
    let selected: Vec<&MemoryRow> = rows
        .iter()
        .filter(|memory| is_shared_export_candidate(memory, tag))
        .collect();

    let mut body = String::new();
    for m in &selected {
        let line = serde_json::to_string(m).map_err(|e| AxError::Other(e.to_string()))?;
        body.push_str(&line);
        body.push('\n');
    }
    ax_utils::atomic_write(&out_path, body.as_bytes())
        .map_err(|e| AxError::Other(e.to_string()))?;

    Ok(MemoryExportResult {
        written: selected.len(),
        path: out_path,
    })
}

/// Source stamped on every row written by an import. It marks the text as
/// untrusted data (see [`crate::provenance::trust_level_for`]).
pub const IMPORT_SOURCE: &str = "import";

/// Sources that mark a local row as a copy of shared data, so a newer shared
/// line may replace it. `git-sync` is the source older imports wrote.
fn came_from_import(source: &str) -> bool {
    source == IMPORT_SOURCE || source == "git-sync"
}

/// A kind the vault knows and that is not recall-only (turns and folder docs
/// are never shared).
fn importable_kind(kind: &str) -> bool {
    crate::types::MEMORY_KINDS.contains(&kind) && !crate::folder_sync::is_recall_only(kind)
}

/// Import JSONL memories as untrusted data.
///
/// - A line with an unknown or recall-only kind, an empty id, or the id of a
///   purged (tombstoned) memory is skipped.
/// - New ids are inserted with source `import`, so they are untrusted and get
///   the same tier, search text, cues and flags as any remembered memory.
/// - An existing id is replaced only when the local row is itself shared
///   (it came from an import or carries the `shared` tag) and the shared line
///   is strictly newer. The row then keeps the shared `updated_at` and becomes
///   untrusted. A local memory that was never shared is never replaced.
/// - A row whose text reads like an instruction to the agent is stored as a
///   `candidate`; it is not recalled until someone confirms it.
///
/// Every line is parsed before anything is written, so a malformed file
/// changes nothing.
pub async fn import_shared(pool: &SqlitePool, path: &Path) -> Result<MemoryImportResult, AxError> {
    if !path.is_file() {
        return Ok(MemoryImportResult::default());
    }
    let content = std::fs::read_to_string(path).map_err(|e| AxError::Other(e.to_string()))?;
    let mut rows: Vec<MemoryRow> = Vec::new();
    for (lineno, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row: MemoryRow = serde_json::from_str(line)
            .map_err(|e| AxError::Other(format!("shared.jsonl line {}: {e}", lineno + 1)))?;
        rows.push(row);
    }

    let mut result = MemoryImportResult::default();
    for row in rows {
        if row.id.trim().is_empty()
            || !importable_kind(&row.kind)
            || is_tombstoned(pool, &row.id).await?
        {
            result.skipped += 1;
            continue;
        }
        match get(pool, &row.id).await? {
            Some(existing) => {
                let shared_locally = came_from_import(&existing.source)
                    || existing
                        .tags
                        .iter()
                        .any(|t| t.eq_ignore_ascii_case("shared"));
                if !shared_locally || existing.updated_at >= row.updated_at {
                    result.skipped += 1;
                    continue;
                }
                let ok = update(
                    pool,
                    &row.id,
                    RememberInput {
                        title: row.title.clone(),
                        body: row.body.clone(),
                        kind: Some(row.kind.clone()),
                        tags: row.tags.clone(),
                        files: row.files.clone(),
                        source: Some(IMPORT_SOURCE.into()),
                    },
                )
                .await?;
                if ok {
                    sqlx::query("UPDATE memories SET updated_at = ? WHERE id = ?")
                        .bind(row.updated_at)
                        .bind(&row.id)
                        .execute(pool)
                        .await
                        .map_err(db_err)?;
                    mark_imported(pool, &row).await?;
                    result.updated += 1;
                } else {
                    result.skipped += 1;
                }
            }
            None => {
                insert_with_id(pool, &row).await?;
                result.inserted += 1;
            }
        }
    }
    Ok(result)
}

async fn is_tombstoned(pool: &SqlitePool, id: &str) -> Result<bool, AxError> {
    let tomb: Option<String> =
        sqlx::query_scalar("SELECT memory_id FROM memory_tombstones WHERE memory_id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(db_err)?;
    Ok(tomb.is_some())
}

/// Insert a shared row under its own id through the normal remember path,
/// then keep its timestamps, confidence and enabled flag.
async fn insert_with_id(pool: &SqlitePool, row: &MemoryRow) -> Result<(), AxError> {
    crate::store::remember_with_id(
        pool,
        row.id.clone(),
        RememberInput {
            title: row.title.clone(),
            body: row.body.clone(),
            kind: Some(row.kind.clone()),
            tags: row.tags.clone(),
            files: row.files.clone(),
            source: Some(IMPORT_SOURCE.into()),
        },
    )
    .await?;
    sqlx::query(
        "UPDATE memories SET created_at = ?, updated_at = ?, confidence = ?, enabled = ? WHERE id = ?",
    )
    .bind(row.created_at)
    .bind(row.updated_at)
    .bind(row.confidence.clamp(0.0, 1.0))
    .bind(i64::from(row.enabled))
    .bind(&row.id)
    .execute(pool)
    .await
    .map_err(db_err)?;
    mark_imported(pool, row).await
}

/// Stamp an imported row as untrusted shared data, and hold instruction-like
/// text back as a candidate.
async fn mark_imported(pool: &SqlitePool, row: &MemoryRow) -> Result<(), AxError> {
    let instruction =
        crate::instruction_like::is_instruction_like(&format!("{} {}", row.title, row.body));
    sqlx::query(
        "UPDATE memories SET source = ?, origin = ?, trust_level = ?,
             status = CASE WHEN ? THEN ? ELSE status END
         WHERE id = ?",
    )
    .bind(IMPORT_SOURCE)
    .bind(crate::provenance::origin_for(IMPORT_SOURCE))
    .bind(crate::provenance::trust_level_for(IMPORT_SOURCE, &row.kind))
    .bind(instruction)
    .bind(crate::brain::STATUS_CANDIDATE)
    .bind(&row.id)
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
#[cfg(test)]
mod export_policy_tests {
    use super::*;

    fn row(kind: &str, tags: &[&str], enabled: bool) -> MemoryRow {
        MemoryRow {
            id: "id".into(),
            kind: kind.into(),
            title: "title".into(),
            body: "body".into(),
            tags: tags.iter().map(|s| (*s).to_owned()).collect(),
            files: Vec::new(),
            confidence: 1.0,
            source: "manual".into(),
            enabled,
            created_at: 0,
            updated_at: 0,
            tier: "t2".into(),
        }
    }

    #[test]
    fn shared_tag_cannot_override_private_or_local_flags() {
        for blocked in ["local", "noshare", "private", "LOCAL", "NoShare"] {
            assert!(!is_shared_export_candidate(
                &row("note", &["shared", blocked], true),
                "shared"
            ));
        }
        assert!(is_shared_export_candidate(
            &row("note", &["ShArEd"], true),
            "shared"
        ));
        assert!(!is_shared_export_candidate(
            &row("note", &["team"], true),
            "shared"
        ));
    }

    #[test]
    fn never_exports_disabled_or_recall_only_memories() {
        assert!(!is_shared_export_candidate(
            &row("note", &["shared"], false),
            "shared"
        ));
        for kind in ["turn", "doc"] {
            assert!(!is_shared_export_candidate(
                &row(kind, &["shared"], true),
                "shared"
            ));
        }
    }
}
