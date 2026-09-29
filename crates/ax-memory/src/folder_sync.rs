//! Import a folder's Markdown and text files as recall-only `doc` memories.

use std::collections::BTreeMap;
use std::path::Path;

use ax_utils::errors::{AxError, DatabaseError};
use serde::Serialize;
use sqlx::SqlitePool;

use crate::types::RememberInput;

pub const DOC_KIND: &str = "doc";
pub const FOLDER_SOURCE: &str = "folder";
pub const MAX_FOLDER_FILES: usize = 2000;
pub const MAX_FOLDER_FILE_BYTES: u64 = 256 * 1024;

const EXTENSIONS: [&str; 3] = ["md", "markdown", "txt"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct FolderSyncReport {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub skipped: usize,
}

/// `turn` and `doc` memories are found by recall only, never injected or exported.
pub fn is_recall_only(kind: &str) -> bool {
    kind == crate::turns::TURN_KIND || kind == DOC_KIND
}

fn id_prefix(folder: &str) -> String {
    format!("doc:{folder}:")
}

pub fn doc_memory_id(folder: &str, rel: &str) -> String {
    let hash = blake3::hash(rel.as_bytes()).to_hex();
    format!("{}{}", id_prefix(folder), &hash[..16])
}

fn db_err(e: sqlx::Error) -> AxError {
    AxError::Database(DatabaseError::new(e.to_string()))
}

fn skipped_dir(name: &str) -> bool {
    name.starts_with('.') || name == "node_modules" || name.starts_with("target")
}

fn wanted_file(name: &str) -> bool {
    !name.starts_with('.')
        && Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Candidate files as `rel -> size`, sorted. Symlinks are never followed.
fn candidates(root: &Path) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    let mut stack = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, rel)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.path().symlink_metadata() else {
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            let child = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if meta.is_dir() && !skipped_dir(&name) {
                stack.push((entry.path(), child));
            } else if meta.is_file() && wanted_file(&name) {
                out.insert(child, meta.len());
            }
        }
    }
    out
}

fn title_of(rel: &str, text: &str) -> String {
    text.lines()
        .find_map(|l| l.strip_prefix("# "))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| rel.rsplit('/').next().unwrap_or(rel))
        .chars()
        .take(120)
        .collect()
}

fn body_of(folder: &str, rel: &str, text: &str) -> String {
    format!("{}\n\nSource: folders/{folder}/{rel}", text.trim_end())
}

async fn existing(pool: &SqlitePool, folder: &str) -> Result<BTreeMap<String, (String, String)>, AxError> {
    let prefix = id_prefix(folder);
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, title, body FROM memories WHERE kind = ? AND substr(id, 1, ?) = ?",
    )
    .bind(DOC_KIND)
    .bind(prefix.chars().count() as i64)
    .bind(&prefix)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    Ok(rows.into_iter().map(|(id, t, b)| (id, (t, b))).collect())
}

/// Read every wanted file under `root` within the limits, plus how many were skipped.
fn read_folder(root: &Path) -> (Vec<(String, String)>, usize) {
    let mut files = Vec::new();
    let mut skipped = 0usize;
    for (rel, size) in candidates(root) {
        if size > MAX_FOLDER_FILE_BYTES || files.len() >= MAX_FOLDER_FILES {
            skipped += 1;
            continue;
        }
        match std::fs::read_to_string(root.join(&rel)) {
            Ok(text) => files.push((rel, text)),
            Err(_) => skipped += 1,
        }
    }
    (files, skipped)
}

/// Import `root` under the folder `folder`: add new files, update changed ones, remove gone ones.
pub async fn sync_folder(
    pool: &SqlitePool,
    folder: &str,
    root: &Path,
) -> Result<FolderSyncReport, AxError> {
    let mut report = FolderSyncReport::default();
    let mut stale = existing(pool, folder).await?;
    let owned = root.to_path_buf();
    let (files, skipped) = tokio::task::spawn_blocking(move || read_folder(&owned))
        .await
        .map_err(|e| AxError::Other(format!("folder read task failed: {e}")))?;
    report.skipped = skipped;
    for (rel, text) in files {
        let id = doc_memory_id(folder, &rel);
        let title = title_of(&rel, &text);
        let body = body_of(folder, &rel, &text);
        let input = RememberInput {
            title: title.clone(),
            body: body.clone(),
            kind: Some(DOC_KIND.into()),
            tags: vec![format!("folder:{folder}")],
            files: vec![format!("folders/{folder}/{rel}")],
            source: Some(FOLDER_SOURCE.into()),
        };
        match stale.remove(&id) {
            Some((t, b)) if t == title && b == body => {}
            Some(_) => {
                crate::store::update(pool, &id, input).await?;
                report.updated += 1;
            }
            None => {
                crate::store::remember_with_id(pool, id, input).await?;
                report.added += 1;
            }
        }
    }
    for id in stale.keys() {
        if crate::store::delete(pool, id).await? {
            report.removed += 1;
        }
    }
    Ok(report)
}

/// Delete every `doc` memory imported from `folder`. Returns how many were removed.
pub async fn remove_folder_memories(pool: &SqlitePool, folder: &str) -> Result<usize, AxError> {
    let mut removed = 0;
    for id in existing(pool, folder).await?.keys() {
        if crate::store::delete(pool, id).await? {
            removed += 1;
        }
    }
    Ok(removed)
}
