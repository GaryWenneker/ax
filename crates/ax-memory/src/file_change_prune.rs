//! Remove automatic `changed <path>` memories for paths the index excludes.
//!
//! Older builds wrote one `file_change` memory for every path a sync or the
//! file watcher reported, including build output, `node_modules`, worktrees and
//! test artifacts. Capture now filters those paths; this cleans up the rows
//! written before that.

use std::collections::BTreeMap;

use serde::Serialize;
use sqlx::SqlitePool;

use ax_utils::errors::AxError;

use crate::turns::FILE_CHANGE_KIND;

/// Paths returned in [`FileChangePruneReport::sample`].
const SAMPLE_PATHS: usize = 10;
/// Areas returned in [`FileChangePruneReport::areas`].
const TOP_AREAS: usize = 10;

/// One directory area and how many matched memories name it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileChangeArea {
    pub area: String,
    pub count: usize,
}

/// What a prune found, and on `apply` what it removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileChangePruneReport {
    /// True when nothing was deleted.
    pub dry_run: bool,
    /// `file_change` memories inspected.
    pub scanned: usize,
    /// Memories whose every path is excluded by the index.
    pub matched: usize,
    /// Memories removed (0 on a dry run).
    pub purged: usize,
    /// Memories left in place: at least one path the index keeps.
    pub kept: usize,
    /// Most common two-segment directory areas among matched paths.
    pub areas: Vec<FileChangeArea>,
    /// A few matched paths, for a quick sanity check.
    pub sample: Vec<String>,
}

/// The paths a `file_change` memory names: its `files` list, else its `changed <path>` title.
fn memory_paths(title: &str, files_json: &str) -> Vec<String> {
    let files: Vec<String> = serde_json::from_str(files_json).unwrap_or_default();
    let files: Vec<String> = files
        .into_iter()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .collect();
    if !files.is_empty() {
        return files;
    }
    title
        .strip_prefix("changed ")
        .map(str::trim)
        .filter(|path| !path.is_empty() && !path.contains(' '))
        .map(|path| vec![path.to_string()])
        .unwrap_or_default()
}

fn area_of(path: &str) -> String {
    let mut parts = path.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), Some(_)) => format!("{a}/{b}"),
        (Some(a), Some(_), None) => a.to_string(),
        _ => ".".to_string(),
    }
}

/// Find `file_change` memories whose every path `excluded` rejects, and on
/// `apply` purge them with [`crate::store::purge_memories`] (index rows, cues,
/// links and vectors go too, and the id is tombstoned so import or restore
/// cannot bring it back). Other memory kinds are never touched.
///
/// `on_progress(done, total)` is called after each purged batch.
pub async fn prune_excluded_file_changes(
    pool: &SqlitePool,
    excluded: &mut dyn FnMut(&str) -> bool,
    apply: bool,
    on_progress: &mut dyn FnMut(usize, usize),
) -> Result<FileChangePruneReport, AxError> {
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT id, title, files FROM memories WHERE kind = ? ORDER BY id")
            .bind(FILE_CHANGE_KIND)
            .fetch_all(pool)
            .await
            .map_err(|e| AxError::Other(e.to_string()))?;
    let scanned = rows.len();
    let mut matched_ids = Vec::new();
    let mut areas: BTreeMap<String, usize> = BTreeMap::new();
    let mut sample = Vec::new();
    for (id, title, files) in rows {
        let paths = memory_paths(&title, &files);
        if paths.is_empty() || !paths.iter().all(|path| excluded(path)) {
            continue;
        }
        for path in &paths {
            *areas.entry(area_of(path)).or_default() += 1;
            if sample.len() < SAMPLE_PATHS {
                sample.push(path.clone());
            }
        }
        matched_ids.push(id);
    }
    let matched = matched_ids.len();
    let purged = if apply {
        crate::store::purge_memories(pool, &matched_ids, on_progress).await?
    } else {
        0
    };
    let mut areas: Vec<FileChangeArea> = areas
        .into_iter()
        .map(|(area, count)| FileChangeArea { area, count })
        .collect();
    areas.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.area.cmp(&b.area)));
    areas.truncate(TOP_AREAS);
    Ok(FileChangePruneReport {
        dry_run: !apply,
        scanned,
        matched,
        purged,
        kept: scanned - matched,
        areas,
        sample,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_come_from_files_then_the_title() {
        assert_eq!(memory_paths("changed x", r#"["a/b.rs"]"#), vec!["a/b.rs"]);
        assert_eq!(memory_paths("changed a/c.rs", "[]"), vec!["a/c.rs"]);
        assert!(memory_paths("changed 40 files in a", "[]").is_empty());
        assert!(memory_paths("something else", "not json").is_empty());
    }

    #[test]
    fn areas_use_two_directory_segments() {
        assert_eq!(area_of("target-dev/release/build/x"), "target-dev/release");
        assert_eq!(area_of("site/a.js"), "site");
        assert_eq!(area_of("Cargo.lock"), ".");
    }
}
