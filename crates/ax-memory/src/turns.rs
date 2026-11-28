//! Per-turn memories written by the agent turn hooks: local, kept 90 days, backed up before pruning.

use std::io::Write;
use std::path::Path;

use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

use crate::types::MemoryRow;

/// Memory kind of per-turn memories; skipped by the `<ax_memories>` inject and by export.
pub const TURN_KIND: &str = "turn";
/// `source` value of memories written by `ax turn-hook`.
pub const TURN_SOURCE: &str = "turn-hook";
/// Age after which `prune_turns` deletes a turn memory.
pub const TURN_RETENTION_DAYS: i64 = 90;

/// One agent turn that changed files or made a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRecord {
    pub id: String,
    pub title: String,
    pub body: String,
    pub files: Vec<String>,
}

fn db_err(e: impl std::fmt::Display) -> AxError {
    AxError::Database(DatabaseError::new(e.to_string()))
}

/// Cap and zstd-compress a hunk for storage inside a file_change body (base64).
pub const HUNK_CAP_BYTES: usize = 64 * 1024;

pub fn compress_hunk(text: &str) -> String {
    let redacted = redact_secrets(text);
    let capped = if redacted.len() > HUNK_CAP_BYTES {
        format!(
            "{}\n…truncated…",
            ax_utils::prefix_bytes(&redacted, HUNK_CAP_BYTES)
        )
    } else {
        redacted
    };
    let compressed = zstd::encode_all(capped.as_bytes(), 3).unwrap_or_default();
    // Hex keeps the payload printable without a base64 dependency.
    compressed.iter().map(|b| format!("{b:02x}")).collect()
}

pub const FILE_CHANGE_KIND: &str = "file_change";

/// Whether `path` already has a file_change memory (any content version) for
/// this session and turn. Sync capture and the turn-end hook report the same
/// edit; the hook skips a path sync already recorded, and sync skips a path
/// the hook already recorded.
async fn file_change_recorded(
    pool: &SqlitePool,
    path: &str,
    session_id: &str,
    turn_id: &str,
) -> Result<bool, AxError> {
    let files = serde_json::to_string(&vec![path]).map_err(|e| AxError::Other(e.to_string()))?;
    let found: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM memories
         WHERE kind = ? AND session_id = ? AND turn_id = ? AND files = ? LIMIT 1",
    )
    .bind(FILE_CHANGE_KIND)
    .bind(session_id)
    .bind(turn_id)
    .bind(&files)
    .fetch_optional(pool)
    .await
    .map_err(db_err)?;
    Ok(found.is_some())
}

fn file_change_id(source: &str) -> String {
    format!(
        "filechange-{}",
        &blake3::hash(source.as_bytes()).to_hex()[..16]
    )
}

/// Id of the file_change memory the turn-end hook writes (no content version).
fn hook_file_change_id(session_id: &str, turn_id: &str, path: &str) -> String {
    file_change_id(&format!("{session_id}\n{turn_id}\n{path}"))
}

async fn memory_exists(pool: &SqlitePool, id: &str) -> Result<bool, AxError> {
    let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM memories WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(db_err)?;
    Ok(found.is_some())
}

pub async fn save_file_changes(
    pool: &SqlitePool,
    paths: &[String],
    session_id: &str,
    turn_id: &str,
    now_ms: i64,
) -> Result<usize, AxError> {
    save_file_change_versions(pool, paths, "", session_id, turn_id, now_ms).await
}

/// One file_change memory per path, per turn and per `content_version`
/// (usually the file's content hash). The same content in the same turn is
/// stored once; a new version of the file is a new memory. With an empty
/// `content_version` (the turn-end hook) a path that already has a
/// file_change memory in this turn is skipped; with a `content_version`
/// (sync capture) a path the hook already recorded in this turn is skipped,
/// so either order leaves one memory per edit.
pub(crate) async fn save_file_change_versions(
    pool: &SqlitePool,
    paths: &[String],
    content_version: &str,
    session_id: &str,
    turn_id: &str,
    now_ms: i64,
) -> Result<usize, AxError> {
    let mut written = 0usize;
    for path in paths {
        let path = path.trim();
        if path.is_empty() {
            continue;
        }
        if content_version.is_empty() {
            if file_change_recorded(pool, path, session_id, turn_id).await? {
                continue;
            }
        } else if memory_exists(pool, &hook_file_change_id(session_id, turn_id, path)).await? {
            // The turn-end hook already recorded this path for this turn.
            continue;
        }
        let id = if content_version.is_empty() {
            hook_file_change_id(session_id, turn_id, path)
        } else {
            file_change_id(&format!(
                "{session_id}\n{turn_id}\n{path}\n{content_version}"
            ))
        };
        let title = redact_secrets(&format!("changed {path}"));
        let files =
            serde_json::to_string(&vec![path]).map_err(|e| AxError::Other(e.to_string()))?;
        let (vector, model) = crate::embed::embed_labeled(&format!("{title} {path}"));
        let embedding = crate::embed::embedding_to_blob(&vector);
        let result = sqlx::query(
            r#"INSERT OR IGNORE INTO memories
               (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, embedding, enabled, session_id, turn_id)
               VALUES (?, ?, ?, ?, '[]', ?, 1.0, 'turn', ?, ?, ?, 1, ?, ?)"#,
        )
        .bind(&id)
        .bind(FILE_CHANGE_KIND)
        .bind(&title)
        .bind(&title)
        .bind(&files)
        .bind(now_ms)
        .bind(now_ms)
        .bind(embedding)
        .bind(session_id)
        .bind(turn_id)
        .execute(pool)
        .await
        .map_err(db_err)?;
        if result.rows_affected() > 0 {
            crate::brain::stamp_memory(pool, &id, FILE_CHANGE_KIND, model).await?;
            if let Some(row) = crate::store::get(pool, &id).await? {
                crate::store::write_derived_columns(pool, &row)
                    .await
                    .map_err(db_err)?;
                crate::brain::register_cues(pool, &row, &[]).await?;
            }
            written += 1;
        }
    }
    Ok(written)
}

/// Like `save_file_changes`, and stores a redacted capped zstd hunk in the body when provided.
/// A path that already has a file_change memory in this turn is skipped.
#[allow(clippy::too_many_arguments)]
pub async fn save_file_change_with_hunk(
    pool: &SqlitePool,
    path: &str,
    hunk: Option<&str>,
    before_hash: Option<&str>,
    after_hash: Option<&str>,
    session_id: &str,
    turn_id: &str,
    now_ms: i64,
) -> Result<usize, AxError> {
    let path = path.trim();
    if path.is_empty() || file_change_recorded(pool, path, session_id, turn_id).await? {
        return Ok(0);
    }
    let id = hook_file_change_id(session_id, turn_id, path);
    let title = redact_secrets(&format!("changed {path}"));
    let mut body = title.clone();
    if let Some(h) = hunk {
        let encoded = compress_hunk(h);
        body = format!(
            "{title}\nbefore={}\nafter={}\nhunk_zstd_hex={encoded}",
            before_hash.unwrap_or(""),
            after_hash.unwrap_or("")
        );
    }
    let files = serde_json::to_string(&vec![path]).map_err(|e| AxError::Other(e.to_string()))?;
    let (vector, model) = crate::embed::embed_labeled(&format!("{title} {path}"));
    let embedding = crate::embed::embedding_to_blob(&vector);
    let result = sqlx::query(
        r#"INSERT OR IGNORE INTO memories
           (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, embedding, enabled, session_id, turn_id)
           VALUES (?, ?, ?, ?, '[]', ?, 1.0, 'turn', ?, ?, ?, 1, ?, ?)"#,
    )
    .bind(&id)
    .bind(FILE_CHANGE_KIND)
    .bind(&title)
    .bind(&body)
    .bind(&files)
    .bind(now_ms)
    .bind(now_ms)
    .bind(embedding)
    .bind(session_id)
    .bind(turn_id)
    .execute(pool)
    .await
    .map_err(db_err)?;
    if result.rows_affected() > 0 {
        crate::brain::stamp_memory(pool, &id, FILE_CHANGE_KIND, model).await?;
        if let Some(row) = crate::store::get(pool, &id).await? {
            crate::store::write_derived_columns(pool, &row)
                .await
                .map_err(db_err)?;
            crate::brain::register_cues(pool, &row, &[]).await?;
        }
        return Ok(1);
    }
    Ok(0)
}

/// Insert `record` as a `turn` memory. Returns `false` when a memory with that id exists.
pub async fn save_turn(
    pool: &SqlitePool,
    record: &TurnRecord,
    now_ms: i64,
) -> Result<bool, AxError> {
    let (vector, model) = crate::embed::embed_labeled(&format!("{} {}", record.title, record.body));
    let embedding = crate::embed::embedding_to_blob(&vector);
    let files = serde_json::to_string(&record.files).map_err(|e| AxError::Other(e.to_string()))?;
    let result = sqlx::query(
        r#"INSERT OR IGNORE INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, embedding, enabled)
           VALUES (?, ?, ?, ?, '[]', ?, 1.0, ?, ?, ?, ?, 1)"#,
    )
    .bind(&record.id)
    .bind(TURN_KIND)
    .bind(&record.title)
    .bind(&record.body)
    .bind(files)
    .bind(TURN_SOURCE)
    .bind(now_ms)
    .bind(now_ms)
    .bind(embedding)
    .execute(pool)
    .await
    .map_err(db_err)?;
    if result.rows_affected() > 0 {
        crate::store::stamp_current_turn(pool, &record.id).await?;
        crate::store::link_follows_in_session(pool, &record.id).await?;
        crate::brain::stamp_memory(pool, &record.id, TURN_KIND, model).await?;
        if let Some(row) = crate::store::get(pool, &record.id).await? {
            crate::store::write_derived_columns(pool, &row)
                .await
                .map_err(db_err)?;
            crate::brain::register_cues(pool, &row, &[]).await?;
        }
    }
    Ok(result.rows_affected() > 0)
}

/// Separates the agent's final reply from the rest of a turn memory body; the outcome is last.
pub const OUTCOME_MARKER: &str = "\n\nOutcome: ";

/// The agent's final reply stored in a turn memory body.
pub fn turn_outcome(body: &str) -> Option<&str> {
    let (_, outcome) = body.split_once(OUTCOME_MARKER)?;
    Some(outcome).filter(|o| !o.is_empty())
}

/// The agent request id stored on a turn (`generation_id`, or the Claude turn counter).
pub fn request_id_from_body(body: &str) -> Option<&str> {
    let before = body.split(OUTCOME_MARKER).next().unwrap_or(body);
    before.lines().find_map(|line| {
        let id = line.trim().strip_prefix("Request: ")?.trim();
        (!id.is_empty()).then_some(id)
    })
}

/// Starts the body line that ties a turn to its chat: `Conversation: <12 hex>`.
pub const CONVERSATION_PREFIX: &str = "Conversation: ";

/// The 12-hex chat key: a prefix of `blake3(conversation id)`, never the raw id.
pub fn conversation_key(conversation: &str) -> String {
    blake3::hash(conversation.as_bytes()).to_hex()[..12].to_string()
}

/// The chat key stored on a turn body.
pub fn conversation_from_body(body: &str) -> Option<&str> {
    let before = body.split(OUTCOME_MARKER).next().unwrap_or(body);
    before.lines().find_map(|line| {
        let key = line.strip_prefix(CONVERSATION_PREFIX)?.trim();
        (key.len() == 12 && key.chars().all(|c| c.is_ascii_hexdigit())).then_some(key)
    })
}

/// `body` with a `Conversation:` line added; `None` when it already has one.
pub fn insert_conversation_line(body: &str, key: &str) -> Option<String> {
    if conversation_from_body(body).is_some() {
        return None;
    }
    let head_end = body.find(OUTCOME_MARKER).unwrap_or(body.len());
    let head = &body[..head_end];
    let at = match head.find("\n\nRequest: ") {
        Some(start) => {
            let line_start = start + 2;
            head[line_start..]
                .find('\n')
                .map_or(head_end, |n| line_start + n)
        }
        None => ["\n\nFiles: ", "\n\nCommits:", "\n\nChanges:"]
            .iter()
            .filter_map(|marker| head.find(marker))
            .min()
            .unwrap_or(head_end),
    };
    Some(format!(
        "{}\n\n{CONVERSATION_PREFIX}{key}{}",
        &body[..at],
        &body[at..]
    ))
}

/// Add chat keys to turn memories that have none. Original rows are appended to
/// `backup_dir/turn-conversation-link-<today>.jsonl` first; nothing changes when that fails.
/// `updated_at` is kept. Returns how many memories changed.
pub async fn link_turn_conversations(
    pool: &SqlitePool,
    links: &[(String, String)],
    now_ms: i64,
    backup_dir: &Path,
) -> Result<u64, AxError> {
    let ids: Vec<String> = links.iter().map(|(id, _)| id.clone()).collect();
    let rows = crate::store::turn_rows_by_id(pool, &ids).await?;
    let mut originals = Vec::new();
    let mut updates = Vec::new();
    for row in rows {
        let Some((_, key)) = links.iter().find(|(id, _)| *id == row.id) else {
            continue;
        };
        if let Some(body) = insert_conversation_line(&row.body, key) {
            updates.push((row.id.clone(), body));
            originals.push(row);
        }
    }
    if updates.is_empty() {
        return Ok(0);
    }
    write_backup(backup_dir, "turn-conversation-link", now_ms, &originals)?;
    let mut tx = pool.begin().await.map_err(db_err)?;
    let mut changed = 0;
    for (id, body) in &updates {
        let result = sqlx::query("UPDATE memories SET body = ? WHERE id = ? AND kind = ?")
            .bind(body)
            .bind(id)
            .bind(TURN_KIND)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        changed += result.rows_affected();
    }
    tx.commit().await.map_err(db_err)?;
    Ok(changed)
}

/// How a file changed in a turn or commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeKind {
    Added,
    Modified,
    Deleted,
}

impl FileChangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
        }
    }

    pub fn letter(self) -> char {
        match self {
            Self::Added => 'A',
            Self::Modified => 'M',
            Self::Deleted => 'D',
        }
    }

    fn from_git_status(status: &str) -> Option<Self> {
        let code = status.chars().next()?;
        Some(match code {
            'A' => Self::Added,
            'D' => Self::Deleted,
            'R' | 'C' => return None,
            _ => Self::Modified,
        })
    }
}

/// `git diff` / `git show --name-status -z` records. A rename is a deletion plus an addition.
pub fn parse_git_name_status(bytes: &[u8]) -> Vec<(String, FileChangeKind)> {
    let text = String::from_utf8_lossy(bytes);
    let mut parts = text.split('\0').filter(|part| !part.is_empty());
    let mut out = Vec::new();
    while let Some(status) = parts.next() {
        if status.starts_with('R') || status.starts_with('C') {
            let Some(old) = parts.next() else { break };
            let Some(new) = parts.next() else { break };
            out.push((old.to_string(), FileChangeKind::Deleted));
            out.push((new.to_string(), FileChangeKind::Added));
            continue;
        }
        let Some(path) = parts.next() else { break };
        if let Some(kind) = FileChangeKind::from_git_status(status) {
            out.push((path.to_string(), kind));
        }
    }
    out
}

/// `Changes:` lines written on a turn (`M path`). Later lines win.
pub fn file_changes_from_body(body: &str) -> Vec<(String, FileChangeKind)> {
    let before = body.split(OUTCOME_MARKER).next().unwrap_or(body);
    let Some((_, section)) = before.split_once("\n\nChanges:") else {
        return Vec::new();
    };
    section
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (letter, path) = line.split_once(' ')?;
            let kind = FileChangeKind::from_git_status(letter)?;
            let path = path.trim();
            (!path.is_empty()).then(|| (path.to_string(), kind))
        })
        .collect()
}

/// Commit hashes named in the body, oldest first, plus a `git-<hash>` memory id.
pub fn commit_revs(body: &str, id: &str) -> Vec<String> {
    let mut newest_first = Vec::new();
    if let Some(rest) = id.strip_prefix("git-") {
        if is_commit_rev(rest) {
            newest_first.push(rest.to_string());
        }
    }
    let before = body.split(OUTCOME_MARKER).next().unwrap_or(body);
    if let Some((_, commits)) = before.split_once("\n\nCommits:") {
        for line in commits.lines() {
            let token = line
                .trim()
                .trim_start_matches("- ")
                .split_whitespace()
                .next()
                .unwrap_or("");
            if is_commit_rev(token) {
                newest_first.push(token.to_string());
            }
        }
    }
    newest_first.reverse();
    newest_first
}

fn is_commit_rev(token: &str) -> bool {
    (7..=40).contains(&token.len()) && token.chars().all(|c| c.is_ascii_hexdigit())
}

/// How many changed lines one file may show before that file is marked truncated.
pub const DIFF_LINE_CAP: usize = 4_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
}

impl DiffLineKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Context => "context",
            Self::Added => "added",
            Self::Removed => "removed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffFile {
    pub path: String,
    pub binary: bool,
    pub truncated: bool,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDiff {
    pub files: Vec<DiffFile>,
    pub truncated: bool,
}

/// A repo-relative path git may be asked to diff. Rejects options, absolutes, and `..`.
pub fn safe_diff_path(path: &str) -> bool {
    if path.is_empty() || path.len() > 500 || path.starts_with('-') {
        return false;
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return false;
    }
    if path.contains(['\0', '\n', '\r']) {
        return false;
    }
    path.split(['/', '\\'])
        .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Unified diff text into files, hunks, and line numbers. `line_cap` applies to each file on its own.
pub fn parse_unified_diff(text: &str, line_cap: usize) -> ParsedDiff {
    let mut files = Vec::new();
    let mut current: Option<DiffFile> = None;
    let mut old_no = 0u32;
    let mut new_no = 0u32;
    let mut in_hunk = false;
    let mut used = 0usize;
    let mut truncated = false;

    for line in text.lines() {
        if let Some(path) = path_from_git_header(line) {
            push_file(&mut files, current.take());
            current = Some(DiffFile {
                path,
                binary: false,
                truncated: false,
                hunks: Vec::new(),
            });
            in_hunk = false;
            used = 0;
            continue;
        }
        let Some(file) = current.as_mut() else {
            continue;
        };
        if line.starts_with("Binary files ") {
            file.binary = true;
            in_hunk = false;
            continue;
        }
        if let Some((old_start, new_start)) = parse_hunk_starts(line) {
            if file.truncated {
                in_hunk = false;
                continue;
            }
            file.hunks.push(DiffHunk {
                header: line.to_string(),
                lines: Vec::new(),
            });
            old_no = old_start;
            new_no = new_start;
            in_hunk = true;
            continue;
        }
        if !in_hunk || line.starts_with('\\') || file.truncated {
            continue;
        }
        if used >= line_cap {
            file.truncated = true;
            truncated = true;
            in_hunk = false;
            continue;
        }
        let Some(parsed) = classify_diff_line(line, &mut old_no, &mut new_no) else {
            continue;
        };
        if let Some(hunk) = file.hunks.last_mut() {
            hunk.lines.push(parsed);
            used += 1;
        }
    }
    push_file(&mut files, current);
    ParsedDiff { files, truncated }
}

fn push_file(files: &mut Vec<DiffFile>, file: Option<DiffFile>) {
    if let Some(file) = file {
        if !file.path.is_empty() {
            files.push(file);
        }
    }
}

fn path_from_git_header(line: &str) -> Option<String> {
    let rest = line.strip_prefix("diff --git ")?;
    let path = if let Some(idx) = rest.rfind(" \"b/") {
        rest[idx + 4..].trim_end_matches('"').to_string()
    } else {
        rest[rest.rfind(" b/")? + 3..].to_string()
    };
    (!path.is_empty()).then_some(path)
}

fn parse_hunk_starts(line: &str) -> Option<(u32, u32)> {
    let rest = line.strip_prefix("@@ ")?;
    let mut parts = rest.split_whitespace();
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let old_start = old.split(',').next()?.parse().ok()?;
    let new_start = new.split(',').next()?.parse().ok()?;
    Some((old_start, new_start))
}

fn classify_diff_line(line: &str, old_no: &mut u32, new_no: &mut u32) -> Option<DiffLine> {
    let (kind, old, new, text) = if let Some(text) = line.strip_prefix('+') {
        let n = *new_no;
        *new_no = new_no.saturating_add(1);
        (DiffLineKind::Added, None, line_no(n), text)
    } else if let Some(text) = line.strip_prefix('-') {
        let n = *old_no;
        *old_no = old_no.saturating_add(1);
        (DiffLineKind::Removed, line_no(n), None, text)
    } else {
        let text = line.strip_prefix(' ')?;
        let old = *old_no;
        let new = *new_no;
        *old_no = old_no.saturating_add(1);
        *new_no = new_no.saturating_add(1);
        (DiffLineKind::Context, line_no(old), line_no(new), text)
    };
    Some(DiffLine {
        kind,
        old_no: old,
        new_no: new,
        text: text.to_string(),
    })
}

fn line_no(n: u32) -> Option<u32> {
    (n > 0).then_some(n)
}

/// Web URL for `path` at `rev` on GitHub, Azure DevOps, GitLab, Bitbucket, or a GitHub-style host.
pub fn git_web_file_url(remote: &str, rev: &str, path: &str) -> Option<String> {
    if !is_commit_rev(rev) || !safe_diff_path(path) {
        return None;
    }
    let base = normalize_git_remote(remote)?;
    let encoded = encode_git_path(path);
    if let Some((org, project, repo)) = azure_repo(&base) {
        let query_path = encode_component(&format!("/{path}"));
        return Some(format!(
            "https://dev.azure.com/{org}/{project}/_git/{repo}?path={query_path}&version=GC{rev}"
        ));
    }
    if base.contains("://gitlab.") || base.contains("://gitlab.com/") {
        return Some(format!("{base}/-/blob/{rev}/{encoded}"));
    }
    if base.contains("://bitbucket.org/") {
        return Some(format!("{base}/src/{rev}/{encoded}"));
    }
    Some(format!("{base}/blob/{rev}/{encoded}"))
}

fn normalize_git_remote(remote: &str) -> Option<String> {
    let s = remote.trim().trim_end_matches('/').trim_end_matches(".git");
    if s.is_empty() {
        return None;
    }
    if let Some(rest) = s.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        return Some(format!("https://{host}/{path}"));
    }
    if let Some(rest) = s.strip_prefix("ssh://") {
        let rest = rest.strip_prefix("git@").unwrap_or(rest);
        return Some(format!("https://{rest}"));
    }
    if let Some(rest) = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
    {
        let scheme = if s.starts_with("https://") {
            "https"
        } else {
            "http"
        };
        let (userhost, path) = rest.split_once('/')?;
        let host = userhost.rsplit('@').next()?;
        return Some(format!("{scheme}://{host}/{path}"));
    }
    None
}

fn azure_repo(base: &str) -> Option<(String, String, String)> {
    if let Some(rest) = base.strip_prefix("https://dev.azure.com/") {
        let mut parts = rest.split('/');
        let org = parts.next()?;
        let project = parts.next()?;
        let marker = parts.next()?;
        let repo = parts.next()?;
        if marker == "_git" && !repo.is_empty() && parts.next().is_none() {
            return Some((org.to_string(), project.to_string(), repo.to_string()));
        }
    }
    if let Some(rest) = base.strip_prefix("https://ssh.dev.azure.com/v3/") {
        let mut parts = rest.split('/');
        let org = parts.next()?;
        let project = parts.next()?;
        let repo = parts.next()?;
        if !repo.is_empty() && parts.next().is_none() {
            return Some((org.to_string(), project.to_string(), repo.to_string()));
        }
    }
    if let Some(rest) = base.strip_prefix("https://") {
        let (host, path) = rest.split_once('/')?;
        let org = host.strip_suffix(".visualstudio.com")?;
        let mut parts = path.split('/');
        let project = parts.next()?;
        let marker = parts.next()?;
        let repo = parts.next()?;
        if marker == "_git" && !repo.is_empty() && parts.next().is_none() {
            return Some((org.to_string(), project.to_string(), repo.to_string()));
        }
    }
    None
}

fn encode_git_path(path: &str) -> String {
    path.split('/')
        .map(encode_component)
        .collect::<Vec<_>>()
        .join("/")
}

fn encode_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Move `turn` and `file_change` memories older than `max_age_days` into the
/// permanent archive.
/// They stay fetchable by id and by cue. A JSONL copy is written first; nothing
/// changes when that write fails.
pub async fn prune_turns(
    pool: &SqlitePool,
    now_ms: i64,
    max_age_days: i64,
    backup_dir: &Path,
) -> Result<u64, AxError> {
    let cutoff = now_ms - max_age_days * 86_400_000;
    let expired = crate::store::expired_turn_rows(pool, cutoff).await?;
    if expired.is_empty() {
        return Ok(0);
    }
    write_backup(backup_dir, "turn-memories", now_ms, &expired)?;
    let mut tx = pool.begin().await.map_err(db_err)?;
    let mut archived = 0;
    for row in &expired {
        let result = sqlx::query(
            "UPDATE memories SET tier = 't3' WHERE id = ? AND kind IN (?, ?) AND COALESCE(tier, 't1') != 't3'",
        )
        .bind(&row.id)
        .bind(TURN_KIND)
        .bind(FILE_CHANGE_KIND)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        archived += result.rows_affected();
    }
    tx.commit().await.map_err(db_err)?;
    for row in &expired {
        crate::brain::index_archive_vector(pool, &row.id).await?;
    }
    Ok(archived)
}

fn write_backup(dir: &Path, stem: &str, now_ms: i64, rows: &[MemoryRow]) -> Result<(), AxError> {
    let io_err = |e: std::io::Error| AxError::Other(format!("turn memory backup failed: {e}"));
    let mut lines = String::new();
    for row in rows {
        lines.push_str(&serde_json::to_string(row).map_err(|e| AxError::Other(e.to_string()))?);
        lines.push('\n');
    }
    std::fs::create_dir_all(dir).map_err(io_err)?;
    let when = crate::history::format_when(now_ms);
    let day = when.get(..10).unwrap_or("undated");
    let path = dir.join(format!("{stem}-{day}.jsonl"));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(io_err)?;
    file.write_all(lines.as_bytes()).map_err(io_err)?;
    file.sync_all().map_err(io_err)
}

const REDACTED: &str = "[redacted]";
const TOKEN_PREFIXES: [&str; 6] = ["ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_"];
/// Vendor token prefixes (Slack, Stripe, GitLab, npm, Google) whose tokens may
/// be shorter than the generic 40-character rule.
const VENDOR_PREFIXES: [&str; 12] = [
    "xoxb-", "xoxp-", "xoxa-", "xoxr-", "xoxs-", "xapp-", "sk_live_", "sk_test_", "rk_live_",
    "glpat-", "npm_", "AIza",
];
/// A key that names a secret: equal to or ending with one of these
/// (case-insensitive), so `db_password`, `client_secret` and `API_KEY` match
/// while `max_tokens` and `author` do not.
const SECRET_KEY_SUFFIXES: [&str; 13] = [
    "password",
    "passwd",
    "pwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "api-key",
    "access_key",
    "private_key",
    "auth",
    "authorization",
    "credentials",
];
/// Authorization schemes: the value after them is the secret.
const AUTH_SCHEMES: [&str; 3] = ["Bearer", "Basic", "Token"];

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '/' | '=' | '.')
}

fn is_secret_key(key: &str) -> bool {
    let key = key.trim_start_matches('-').to_ascii_lowercase();
    !key.is_empty() && SECRET_KEY_SUFFIXES.iter().any(|s| key.ends_with(s))
}

fn is_jwt(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    token.starts_with("eyJ")
        && token.len() >= 20
        && parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '='))
        })
}

fn looks_like_secret(token: &str) -> bool {
    let len = token.len();
    if token.starts_with("sk-") && len >= 20 {
        return true;
    }
    if TOKEN_PREFIXES.iter().any(|p| token.starts_with(p)) && len >= 24 {
        return true;
    }
    if VENDOR_PREFIXES.iter().any(|p| token.starts_with(p)) && len >= 20 {
        return true;
    }
    if is_jwt(token) {
        return true;
    }
    if token.starts_with("AKIA")
        && len == 20
        && token
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return true;
    }
    if len < 40 {
        return false;
    }
    if token.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    token
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
        && token.chars().any(|c| c.is_ascii_digit())
        && token.chars().any(|c| c.is_ascii_uppercase())
        && token.chars().any(|c| c.is_ascii_lowercase())
}

/// `password=value` → `password=[redacted]`; `None` when `token` is not such an assignment.
fn redact_assignment(token: &str) -> Option<String> {
    let (key, value) = token.split_once('=')?;
    (is_secret_key(key) && !value.is_empty()).then(|| format!("{key}={REDACTED}"))
}

/// The separator between a key and its value: `:` or `=`, with optional
/// spaces and quotes around it (`password: x`, `"password": "x"`, `key='x'`).
fn is_key_value_separator(separator: &str) -> bool {
    let core: String = separator
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '"' | '\''))
        .collect();
    core == ":" || core == "="
}

/// `scheme://user:password@host` → `scheme://user:[redacted]@host`.
fn redact_url_credentials(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let (head, tail) = rest.split_at(at + 3);
        out.push_str(head);
        let authority_end = tail
            .find(|c: char| {
                c == '/'
                    || c == '?'
                    || c == '#'
                    || c.is_whitespace()
                    || matches!(c, '"' | '\'' | '<' | '>')
            })
            .unwrap_or(tail.len());
        let authority = &tail[..authority_end];
        match authority.rfind('@') {
            Some(at_sign) => {
                let userinfo = &authority[..at_sign];
                match userinfo.split_once(':') {
                    Some((user, password)) if !password.is_empty() => {
                        out.push_str(user);
                        out.push(':');
                        out.push_str(REDACTED);
                        out.push_str(&authority[at_sign..]);
                    }
                    _ => out.push_str(authority),
                }
            }
            None => out.push_str(authority),
        }
        rest = &tail[authority_end..];
    }
    out.push_str(rest);
    out
}

/// Replace likely secrets with `[redacted]`: API keys and vendor tokens, JWTs,
/// long hex/base64 runs, values of secret-named keys (`password=`,
/// `"api_key": "…"`, `client_secret: …`, `token = …`), `Authorization:
/// Bearer …` values and the password in `scheme://user:password@host`.
pub fn redact_secrets(text: &str) -> String {
    /// What the previous tokens said about the next one.
    #[derive(Clone, Copy, PartialEq)]
    enum Expect {
        Nothing,
        /// A secret-named key: a value follows after `:` or `=`.
        Key,
        /// A value follows after whitespace (`password = x`, `Bearer x`).
        Value,
        /// Like `Value`, after an authorization scheme was already seen.
        SchemeValue,
    }
    let text = redact_url_credentials(text);
    let mut out = String::with_capacity(text.len());
    let mut expect = Expect::Nothing;
    let mut rest = text.as_str();
    while let Some(start) = rest.find(is_token_char) {
        let separator = &rest[..start];
        out.push_str(separator);
        let tail = &rest[start..];
        let end = tail.find(|c: char| !is_token_char(c)).unwrap_or(tail.len());
        let run = &tail[..end];
        let token = run.trim_end_matches('.');
        let dots = &run[token.len()..];
        let quiet_separator = separator
            .chars()
            .all(|c| c.is_whitespace() || matches!(c, '"' | '\''));
        let is_value = !token.is_empty()
            && match expect {
                Expect::Nothing => false,
                Expect::Key => is_key_value_separator(separator),
                Expect::Value | Expect::SchemeValue => quiet_separator,
            };
        expect = if expect == Expect::Key && token == "=" && quiet_separator {
            // `password = value`: the value comes next.
            out.push_str(token);
            Expect::Value
        } else if is_value && expect != Expect::SchemeValue && AUTH_SCHEMES.contains(&token) {
            out.push_str(token);
            Expect::SchemeValue
        } else if is_value {
            out.push_str(REDACTED);
            Expect::Nothing
        } else if token == "Bearer" {
            out.push_str(token);
            Expect::SchemeValue
        } else if let Some(redacted) = redact_assignment(token) {
            out.push_str(&redacted);
            Expect::Nothing
        } else if looks_like_secret(token) {
            out.push_str(REDACTED);
            Expect::Nothing
        } else {
            out.push_str(token);
            if is_secret_key(token) {
                Expect::Key
            } else {
                Expect::Nothing
            }
        };
        out.push_str(dots);
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod file_change_tests {
    use super::{commit_revs, file_changes_from_body, parse_git_name_status, FileChangeKind};

    #[test]
    fn name_status_marks_add_edit_delete_and_rename() {
        let raw = b"M\0src/a.rs\0D\0old.rs\0A\0new.rs\0R100\0from.rs\0to.rs\0";
        assert_eq!(
            parse_git_name_status(raw),
            vec![
                ("src/a.rs".into(), FileChangeKind::Modified),
                ("old.rs".into(), FileChangeKind::Deleted),
                ("new.rs".into(), FileChangeKind::Added),
                ("from.rs".into(), FileChangeKind::Deleted),
                ("to.rs".into(), FileChangeKind::Added),
            ]
        );
    }

    #[test]
    fn body_changes_and_commit_revs_are_read_back() {
        let body = "prompt\n\nCommits:\n- 9183e498c Keep the hook.\n\nChanges:\nM .husky/commit-msg\nD commitlint.config.js\n\nOutcome: done";
        assert_eq!(
            file_changes_from_body(body),
            vec![
                (".husky/commit-msg".into(), FileChangeKind::Modified),
                ("commitlint.config.js".into(), FileChangeKind::Deleted),
            ]
        );
        assert_eq!(
            commit_revs(body, "git-abc1234"),
            vec!["9183e498c".to_string(), "abc1234".to_string()]
        );
    }
}

#[cfg(test)]
mod diff_tests {
    use super::{git_web_file_url, parse_unified_diff, safe_diff_path, DiffLineKind};

    #[test]
    fn unified_diff_numbers_added_and_removed_lines() {
        let patch = "\
diff --git a/src/a.rs b/src/a.rs
index 111..222 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -10,3 +10,4 @@ fn keep() {
 context
-old line
+new line
+another
 context
";
        let parsed = parse_unified_diff(patch, 80);
        assert!(!parsed.truncated);
        assert_eq!(parsed.files[0].path, "src/a.rs");
        let lines = &parsed.files[0].hunks[0].lines;
        assert_eq!(lines[0].kind, DiffLineKind::Context);
        assert_eq!(
            (lines[0].old_no, lines[0].new_no, lines[0].text.as_str()),
            (Some(10), Some(10), "context")
        );
        assert_eq!(lines[1].kind, DiffLineKind::Removed);
        assert_eq!((lines[1].old_no, lines[1].new_no), (Some(11), None));
        assert_eq!(lines[2].kind, DiffLineKind::Added);
        assert_eq!((lines[2].old_no, lines[2].new_no), (None, Some(11)));
        assert_eq!(lines[3].new_no, Some(12));
        assert_eq!((lines[4].old_no, lines[4].new_no), (Some(12), Some(13)));
    }

    #[test]
    fn new_file_and_quoted_path_and_binary() {
        let added = "diff --git a/new.rs b/new.rs\n--- /dev/null\n+++ b/new.rs\n@@ -0,0 +1,2 @@\n+one\n+two\n";
        let parsed = parse_unified_diff(added, 80);
        assert_eq!(parsed.files[0].hunks[0].lines[0].new_no, Some(1));
        assert_eq!(parsed.files[0].hunks[0].lines[0].old_no, None);
        let quoted = "diff --git \"a/my file.rs\" \"b/my file.rs\"\n@@ -1 +1 @@\n-a\n+b\n";
        assert_eq!(parse_unified_diff(quoted, 80).files[0].path, "my file.rs");
        let binary =
            "diff --git a/img.png b/img.png\nBinary files a/img.png and b/img.png differ\n";
        let parsed = parse_unified_diff(binary, 80);
        assert!(parsed.files[0].binary);
        assert!(parsed.files[0].hunks.is_empty());
    }

    #[test]
    fn line_cap_stops_the_snippet() {
        let mut patch = "diff --git a/a.rs b/a.rs\n@@ -1,5 +1,5 @@\n".to_string();
        for i in 0..10 {
            patch.push_str(&format!(" line {i}\n"));
        }
        let parsed = parse_unified_diff(&patch, 3);
        assert!(parsed.truncated);
        assert_eq!(parsed.files[0].hunks[0].lines.len(), 3);
        assert_eq!(parsed.files[0].hunks[0].lines[0].text, "line 0");
        assert!(parsed.files[0].truncated);
    }

    #[test]
    fn line_cap_leaves_the_next_file_intact() {
        let patch = "\
diff --git a/a.rs b/a.rs
@@ -1,1 +1,3 @@
-a
+b
+c
diff --git a/b.rs b/b.rs
@@ -1 +1 @@
-x
+y
";
        let parsed = parse_unified_diff(patch, 2);
        assert!(parsed.files[0].truncated);
        assert_eq!(parsed.files[0].hunks[0].lines.len(), 2);
        assert!(!parsed.files[1].truncated);
        assert_eq!(parsed.files[1].path, "b.rs");
        assert_eq!(parsed.files[1].hunks[0].lines.len(), 2);
        assert!(parsed.files[0].hunks[0].header.starts_with("@@"));
    }

    #[test]
    fn unsafe_diff_paths_are_rejected() {
        assert!(safe_diff_path(".husky/commit-msg"));
        assert!(!safe_diff_path("../etc/passwd"));
        assert!(!safe_diff_path("/etc/passwd"));
        assert!(!safe_diff_path("-rf"));
        assert!(!safe_diff_path(""));
        assert!(!safe_diff_path("a/../b"));
    }

    #[test]
    fn file_urls_cover_github_azure_devops_gitlab_and_bitbucket() {
        assert_eq!(
            git_web_file_url("git@github.com:org/repo.git", "9183e498c", "src/a.rs").as_deref(),
            Some("https://github.com/org/repo/blob/9183e498c/src/a.rs")
        );
        assert_eq!(
            git_web_file_url(
                "https://dev.azure.com/VfPf-NL/Frontends-Algemeen/_git/Pf_Portal",
                "9183e498c",
                ".husky/commit-msg"
            )
            .as_deref(),
            Some("https://dev.azure.com/VfPf-NL/Frontends-Algemeen/_git/Pf_Portal?path=%2F.husky%2Fcommit-msg&version=GC9183e498c")
        );
        assert_eq!(
            git_web_file_url(
                "git@ssh.dev.azure.com:v3/org/project/repo",
                "abc1234",
                "src/a.rs"
            )
            .as_deref(),
            Some(
                "https://dev.azure.com/org/project/_git/repo?path=%2Fsrc%2Fa.rs&version=GCabc1234"
            )
        );
        assert_eq!(
            git_web_file_url("https://gitlab.com/group/sub/repo.git", "abc1234", "a.rs").as_deref(),
            Some("https://gitlab.com/group/sub/repo/-/blob/abc1234/a.rs")
        );
        assert_eq!(
            git_web_file_url("git@bitbucket.org:org/repo.git", "abc1234", "my file.rs").as_deref(),
            Some("https://bitbucket.org/org/repo/src/abc1234/my%20file.rs")
        );
        assert!(git_web_file_url("git@github.com:org/repo.git", "not-a-rev", "a.rs").is_none());
        assert!(git_web_file_url("git@github.com:org/repo.git", "abc1234", "../x").is_none());
    }
}

#[cfg(test)]
mod hunk_tests {
    use super::{compress_hunk, write_backup, HUNK_CAP_BYTES};

    fn decompress(hex: &str) -> String {
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        String::from_utf8(zstd::decode_all(bytes.as_slice()).unwrap()).unwrap()
    }

    #[test]
    fn a_large_hunk_is_cut_on_a_char_boundary() {
        // Two ASCII bytes shift the 3-byte chars so the cap lands mid-char.
        let text = format!("xx{}", "€".repeat(HUNK_CAP_BYTES));
        assert!(!text.is_char_boundary(HUNK_CAP_BYTES));
        let out = decompress(&compress_hunk(&text));
        assert!(out.ends_with("\n…truncated…"));
        let kept = out.trim_end_matches("\n…truncated…");
        assert!(kept.len() <= HUNK_CAP_BYTES);
        assert!(kept.len() > HUNK_CAP_BYTES - 4);
    }

    #[test]
    fn a_backup_with_an_unformattable_time_does_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        write_backup(dir.path(), "turns", i64::MAX, &[]).unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
