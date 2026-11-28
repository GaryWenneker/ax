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

/// The files found under a folder root.
struct Walk {
    /// Candidate files as `rel -> size`, sorted.
    files: BTreeMap<String, u64>,
    /// A subfolder or entry could not be read, so files may exist that were not seen.
    incomplete: bool,
}

/// Walk `root` for candidate files. Symlinks are never followed. A root that is
/// missing or cannot be listed is an error, so a sync never mistakes an
/// unmounted drive for an empty folder.
fn candidates(root: &Path) -> std::io::Result<Walk> {
    let mut walk = Walk {
        files: BTreeMap::new(),
        incomplete: false,
    };
    let mut stack = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, rel)) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if rel.is_empty() => return Err(e),
            Err(_) => {
                walk.incomplete = true;
                continue;
            }
        };
        for entry in entries {
            let Ok(entry) = entry else {
                walk.incomplete = true;
                continue;
            };
            let Ok(meta) = entry.path().symlink_metadata() else {
                walk.incomplete = true;
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
                walk.files.insert(child, meta.len());
            }
        }
    }
    Ok(walk)
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

async fn existing(
    pool: &SqlitePool,
    folder: &str,
) -> Result<BTreeMap<String, (String, String)>, AxError> {
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

/// What one read of a folder produced.
struct FolderRead {
    /// Files read within the limits, as `(rel, text)`.
    files: Vec<(String, String)>,
    /// Files that exist but were not read (too large, over the file limit, or unreadable).
    skipped: Vec<String>,
    /// Part of the folder could not be listed.
    incomplete: bool,
}

/// Read every wanted file under `root` within the limits.
fn read_folder(root: &Path) -> std::io::Result<FolderRead> {
    let walk = candidates(root)?;
    let mut read = FolderRead {
        files: Vec::new(),
        skipped: Vec::new(),
        incomplete: walk.incomplete,
    };
    for (rel, size) in walk.files {
        if size > MAX_FOLDER_FILE_BYTES || read.files.len() >= MAX_FOLDER_FILES {
            read.skipped.push(rel);
            continue;
        }
        match std::fs::read_to_string(root.join(&rel)) {
            Ok(text) => read.files.push((rel, text)),
            Err(_) => read.skipped.push(rel),
        }
    }
    Ok(read)
}

/// Import `root` under the folder `folder`: add new files, update changed ones, remove gone ones.
///
/// A missing or unreadable root is an error and changes nothing. A file that
/// exists but was skipped (too large, over the file limit, unreadable) keeps
/// its memory as it was. When part of the folder could not be listed, no
/// memory is removed, because a file that was not seen is not known to be gone.
pub async fn sync_folder(
    pool: &SqlitePool,
    folder: &str,
    root: &Path,
) -> Result<FolderSyncReport, AxError> {
    let mut report = FolderSyncReport::default();
    let mut stale = existing(pool, folder).await?;
    let owned = root.to_path_buf();
    let read = tokio::task::spawn_blocking(move || read_folder(&owned))
        .await
        .map_err(|e| AxError::Other(format!("folder read task failed: {e}")))?
        .map_err(|e| AxError::Other(format!("cannot read folder {}: {e}", root.display())))?;
    report.skipped = read.skipped.len();
    for rel in &read.skipped {
        stale.remove(&doc_memory_id(folder, rel));
    }
    if read.incomplete {
        stale.clear();
    }
    for (rel, text) in read.files {
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
