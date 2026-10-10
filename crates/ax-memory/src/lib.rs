//! ax-memory: persistent memory vault (decisions, fixes, conventions) in ax.db.
//!
//! Memories live in the project graph database and are recalled with FTS5
//! ranking combined with confidence decay, so fresh knowledge outranks stale.

pub mod capture;
pub mod embed;
pub mod folder_sync;
pub mod format;
pub mod history;
pub mod onnx;
pub mod store;
pub mod sync;
pub mod turns;
pub mod types;

pub use capture::{capture_git_history, GitCaptureResult};
pub use folder_sync::{
    doc_memory_id, is_recall_only, remove_folder_memories, sync_folder, FolderSyncReport, DOC_KIND,
    FOLDER_SOURCE, MAX_FOLDER_FILES, MAX_FOLDER_FILE_BYTES,
};
pub use format::{format_memories_inject_block, format_memory_match_titles};
pub use history::{
    continuation_turns, format_history, format_turn_history_block, format_when, history,
    history_entry, is_history_question, parse_since, related_turns, HistoryEntry, HistoryQuery,
};
pub use store::{
    delete, effective_confidence, find_similar, fts_query_from_text, get, link_catalog, list,
    recall, remember, set_enabled, turn_rows, update,
};
pub use sync::{
    default_shared_path, export_shared, import_shared, memory_sync_enabled, MemoryExportResult,
    MemoryImportResult,
};
pub use turns::{
    commit_revs, conversation_from_body, conversation_key, file_changes_from_body,
    git_web_file_url, insert_conversation_line, link_turn_conversations, parse_git_name_status,
    parse_unified_diff, prune_turns, redact_secrets, request_id_from_body, safe_diff_path,
    save_turn, turn_outcome, DiffFile, DiffHunk, DiffLine, DiffLineKind, FileChangeKind,
    ParsedDiff, TurnRecord, CONVERSATION_PREFIX, DIFF_LINE_CAP, OUTCOME_MARKER, TURN_KIND,
    TURN_RETENTION_DAYS, TURN_SOURCE,
};
pub use types::{MemoryMatch, MemoryRow, RememberInput, MEMORY_KINDS};

use ax_utils::errors::{AxError, DatabaseError};
use sqlx::SqlitePool;

/// Top memories for a user prompt, used by `ax_preflight` injection.
/// Only returns confident matches to keep the inject small. `turn` and `doc` memories are recall-only.
pub async fn recall_for_prompt(
    pool: &SqlitePool,
    prompt: &str,
    limit: usize,
) -> Result<Vec<MemoryMatch>, AxError> {
    let mut matches = recall(pool, prompt, (limit * 4).max(20)).await?;
    matches.retain(|m| m.score > 0.0 && !is_recall_only(&m.memory.kind));
    matches.truncate(limit);
    Ok(matches)
}

/// Prompt matches first. Newest durable memories fill any empty slots so the
/// vault is in preflight even when the question shares no words with it.
pub async fn memories_for_preflight(
    pool: &SqlitePool,
    prompt: &str,
    limit: usize,
) -> Vec<MemoryMatch> {
    let mut matched = recall_for_prompt(pool, prompt, limit)
        .await
        .unwrap_or_default();
    if matched.len() >= limit {
        return matched;
    }
    let Ok((rows, _)) = list(pool, 40, 0).await else {
        return matched;
    };
    let mut seen: std::collections::BTreeSet<String> =
        matched.iter().map(|m| m.memory.id.clone()).collect();
    for row in rows {
        if matched.len() >= limit {
            break;
        }
        if is_recall_only(&row.kind) || !row.enabled || !seen.insert(row.id.clone()) {
            continue;
        }
        matched.push(MemoryMatch {
            memory: row,
            score: 0.0,
        });
    }
    matched
}

/// Seed the memory vault from knowledge-graph metadata (languages, file
/// structure, top-level architecture). Used when there is no git history.
pub async fn seed_from_graph(pool: &SqlitePool) -> Result<usize, AxError> {
    let db_err = |e: sqlx::Error| AxError::Database(DatabaseError::new(e.to_string()));

    let lang_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT language, COUNT(*) FROM files GROUP BY language ORDER BY COUNT(*) DESC LIMIT 10",
    )
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    if lang_rows.is_empty() {
        return Ok(0);
    }

    let file_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM files")
        .fetch_one(pool)
        .await
        .map_err(db_err)?;
    let node_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes")
        .fetch_one(pool)
        .await
        .map_err(db_err)?;

    let langs: Vec<String> = lang_rows
        .iter()
        .map(|(l, c)| format!("{l} ({c} files)"))
        .collect();
    let lang_summary = langs.join(", ");

    let mut captured = 0usize;

    let arch_body = format!(
        "Project contains {file_count} files and {node_count} code symbols.\n\
         Languages: {lang_summary}."
    );
    let input = RememberInput {
        title: "Project architecture overview".into(),
        body: arch_body,
        kind: Some("architecture".into()),
        tags: vec!["auto-seed".into(), "architecture".into()],
        files: vec![],
        source: Some("graph-seed".into()),
    };
    if remember(pool, input).await.is_ok() {
        captured += 1;
    }

    let top_dirs: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT CASE WHEN INSTR(path, '/') > 0 THEN SUBSTR(path, 1, INSTR(path, '/') - 1) \
         ELSE path END AS dir FROM files ORDER BY dir LIMIT 15",
    )
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    if !top_dirs.is_empty() {
        let dirs: Vec<&str> = top_dirs.iter().map(|(d,)| d.as_str()).collect();
        let input = RememberInput {
            title: "Top-level directory structure".into(),
            body: format!("Root directories: {}", dirs.join(", ")),
            kind: Some("architecture".into()),
            tags: vec!["auto-seed".into(), "structure".into()],
            files: vec![],
            source: Some("graph-seed".into()),
        };
        if remember(pool, input).await.is_ok() {
            captured += 1;
        }
    }

    for (lang, count) in &lang_rows {
        if *count < 5 {
            continue;
        }
        let kinds: Vec<(String, i64)> = sqlx::query_as(
            "SELECT n.kind, COUNT(*) FROM nodes n JOIN files f ON n.file_path = f.path \
             WHERE f.language = ? GROUP BY n.kind ORDER BY COUNT(*) DESC LIMIT 5",
        )
        .bind(lang)
        .fetch_all(pool)
        .await
        .map_err(db_err)?;
        if kinds.is_empty() {
            continue;
        }
        let breakdown: Vec<String> = kinds
            .iter()
            .map(|(k, c)| format!("{c} {k}s"))
            .collect();
        let input = RememberInput {
            title: format!("{lang} codebase structure"),
            body: format!(
                "{lang}: {count} files containing {}.",
                breakdown.join(", ")
            ),
            kind: Some("architecture".into()),
            tags: vec!["auto-seed".into(), lang.clone()],
            files: vec![],
            source: Some("graph-seed".into()),
        };
        if remember(pool, input).await.is_ok() {
            captured += 1;
        }
    }

    Ok(captured)
}
