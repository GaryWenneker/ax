//! ax-memory: persistent memory vault (decisions, fixes, conventions) in ax.db.
//!
//! Memories live in the project graph database and are recalled with FTS5
//! ranking combined with confidence decay, so fresh knowledge outranks stale.

pub mod access;
pub mod act_r;
pub mod anchors;
pub mod backfill;
pub mod backup;
pub mod bench;
pub mod bm25f;
pub mod brain;
pub mod capture;
pub mod embed;
pub mod episodes;
pub mod extract;
pub mod file_change_prune;
pub mod folder_sync;
pub mod format;
pub mod fusion;
pub mod history;
pub mod instruction_like;
pub mod lexicon;
pub mod pagerank;
pub mod pmi;
pub mod prospective;
pub mod provenance;
pub mod scope;
pub mod search_text;
pub mod store;
pub mod sync;
pub mod sync_capture;
pub mod turns;
pub mod types;

pub use access::{access_times, record_access, record_feedback};
pub use act_r::base_level_activation;
pub use anchors::{anchor_memory, auto_attach_memory, reanchor_renames, refresh_anchor_flags};
pub use backfill::{backfill_session_ids, BackfillReport};
pub use backup::{
    restore_from_backup, write_full_backup, write_incremental_backup, BackupResult, RestoreReport,
};
pub use bm25f::{bm25_term, bm25f_combine, B as BM25_B, K1 as BM25_K1};
pub use brain::{
    capture_activity_engrams, capture_graph_episodes, capture_graph_episodes_on, confirm_memory,
    consolidate, cues_for, normalize_cue, rebuild_vector_index, reembed_all_foreign, reembed_stale,
    search, suggested_triggers, Depth, SearchRequest,
};
pub use capture::{capture_git_history, GitCaptureResult};
pub use episodes::{record_episode, EpisodeInput};
pub use extract::{
    extract_bug_fix_lesson, extract_commit_decision, extract_revert_lesson, remember_lesson, Lesson,
};
pub use file_change_prune::{prune_excluded_file_changes, FileChangeArea, FileChangePruneReport};
pub use folder_sync::{
    doc_memory_id, is_recall_only, remove_folder_memories, sync_folder, FolderSyncReport, DOC_KIND,
    FOLDER_SOURCE, MAX_FOLDER_FILES, MAX_FOLDER_FILE_BYTES,
};
pub use format::{format_memories_inject_block, format_memory_match_titles};
pub use fusion::{fuse_scores, should_abstain, why_from_hits, WhyRecalled, ABSTAIN_THRESHOLD};
pub use history::{
    continuation_turns, format_history, format_turn_history_block, format_when, history,
    history_entry, is_history_question, parse_since, related_turns, HistoryEntry, HistoryQuery,
};
pub use instruction_like::is_instruction_like;
pub use lexicon::expand_token;
pub use pagerank::personalized_pagerank;
pub use pmi::expand_with_pmi;
pub use prospective::{parse_when, prospective_tags_for_path, when_matches, WhenClause};
pub use provenance::{origin_for, trust_level_for};
pub use scope::{assert_scope, scope_paths};
pub use search_text::normalize_search_text;
pub use store::{
    apply_memory_filter, delete, effective_confidence, find_similar, fts_query_from_text, get,
    link_catalog, link_memories, list, list_for_session, purge_memories, purge_memory, recall,
    recall_filtered, reindex_memory_fts, remember, session_ids, set_enabled, turn_rows, update,
    MEMORY_LINK_KINDS,
};
pub use sync::{
    default_shared_path, export_shared, import_shared, memory_sync_enabled, MemoryExportResult,
    MemoryImportResult,
};
pub use sync_capture::{
    capture_affected_paths, capture_sync_file_change, current_session_turn,
    record_versions_for_paths, FILE_CHANGE_BURST_LIMIT, FILE_CHANGE_BURST_LISTED,
};
pub use turns::{
    commit_revs, compress_hunk, conversation_from_body, conversation_key, file_changes_from_body,
    git_web_file_url, insert_conversation_line, link_turn_conversations, parse_git_name_status,
    parse_unified_diff, prune_turns, redact_secrets, request_id_from_body, safe_diff_path,
    save_file_change_with_hunk, save_file_changes, save_turn, turn_outcome, DiffFile, DiffHunk,
    DiffLine, DiffLineKind, FileChangeKind, ParsedDiff, TurnRecord, CONVERSATION_PREFIX,
    DIFF_LINE_CAP, FILE_CHANGE_KIND, HUNK_CAP_BYTES, OUTCOME_MARKER, TURN_KIND,
    TURN_RETENTION_DAYS, TURN_SOURCE,
};
pub use types::{MemoryFilter, MemoryMatch, MemoryRow, RememberInput, MEMORY_KINDS};

use ax_utils::errors::{AxError, DatabaseError};
use sqlx::SqlitePool;

/// Top memories for a user prompt, used by `ax_preflight` injection.
/// Only returns confident matches to keep the inject small. `turn` and `doc` memories are recall-only.
pub async fn recall_for_prompt(
    pool: &SqlitePool,
    prompt: &str,
    limit: usize,
) -> Result<Vec<MemoryMatch>, AxError> {
    recall_for_prompt_near(pool, prompt, &[], limit).await
}

/// Same inject as [`recall_for_prompt`], with a named cue outranking vector hits
/// and open files boosting memories that name those files.
pub async fn recall_for_prompt_near(
    pool: &SqlitePool,
    prompt: &str,
    files: &[String],
    limit: usize,
) -> Result<Vec<MemoryMatch>, AxError> {
    let mut kept = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let t0 = std::time::Instant::now();
    let cue_hits = brain::prompt_cues(pool, prompt, limit)
        .await
        .unwrap_or_default();
    let t_cues = t0.elapsed();
    for hit in cue_hits {
        // prompt_cues already filtered to hot/active injectable kinds.
        if seen.insert(hit.memory.id.clone()) {
            kept.push(hit);
        }
    }
    let t1 = std::time::Instant::now();
    let mut matches =
        store::recall_scoped(pool, prompt, (limit * 4).max(20), files, false, false).await?;
    let t_scoped = t1.elapsed();
    matches.retain(|m| m.score > 0.0 && !is_recall_only(&m.memory.kind));
    for hit in matches {
        if brain::is_injectable(pool, &hit.memory.id)
            .await
            .unwrap_or(false)
            && seen.insert(hit.memory.id.clone())
        {
            kept.push(hit);
        }
    }
    kept.sort_by(|a, b| {
        let sa = (a.score * 100.0).round() as i64;
        let sb = (b.score * 100.0).round() as i64;
        sb.cmp(&sa).then_with(|| a.memory.id.cmp(&b.memory.id))
    });
    kept.truncate(limit);
    if std::env::var_os("AX_MEMORY_PROFILE").is_some() {
        eprintln!(
            "recall_for_prompt_near cues={t_cues:?} scoped={t_scoped:?} total={:?}",
            t0.elapsed()
        );
    }
    Ok(kept)
}

/// One archive row for a policy removal, restore, or first sighting. A repeat id is a no-op.
pub async fn archive_policy_memory(
    pool: &SqlitePool,
    id: &str,
    title: &str,
    body: &str,
) -> Result<(), AxError> {
    if store::get(pool, id).await?.is_some() {
        return Ok(());
    }
    store::remember_with_id(
        pool,
        id.to_string(),
        RememberInput {
            title: title.to_string(),
            body: body.to_string(),
            kind: Some("policy".into()),
            tags: vec!["policy".into()],
            files: Vec::new(),
            source: Some("policy-history".into()),
        },
    )
    .await?;
    Ok(())
}

pub async fn remember_with_triggers(
    pool: &SqlitePool,
    input: RememberInput,
    triggers: &[String],
) -> Result<MemoryRow, AxError> {
    let row = remember(pool, input).await?;
    brain::register_cues(pool, &row, triggers).await?;
    Ok(row)
}

/// Prompt matches first. Newest durable memories fill any empty slots so the
/// vault is in preflight even when the question shares no words with it.
pub async fn memories_for_preflight(
    pool: &SqlitePool,
    prompt: &str,
    limit: usize,
) -> Vec<MemoryMatch> {
    recall_for_prompt(pool, prompt, limit)
        .await
        .unwrap_or_default()
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
            tags: {
                let mut tags = vec!["auto-seed".into(), "structure".into()];
                tags.extend(dirs.iter().map(|dir| (*dir).to_string()));
                tags
            },
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
        let breakdown: Vec<String> = kinds.iter().map(|(k, c)| format!("{c} {k}s")).collect();
        let input = RememberInput {
            title: format!("{lang} codebase structure"),
            body: format!("{lang}: {count} files containing {}.", breakdown.join(", ")),
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
