//! Explicit CLI turn lifecycle (no IDE hooks required).
//!
//! Snapshots live under `<project>/.ax/cli-turns/<session>/<turn_id>.json`.
//! Finish writes a turn memory even when no files changed (research turns).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::activity::{
    record_activity, ActivityRecord, ActivitySource, ActivityStatus, TokenMetrics,
};
use crate::context_cache::project_scope;

/// `source` for memories written by `ax turn finish`.
pub const TURN_SOURCE_CLI: &str = "cli-turn";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TurnState {
    Open,
    Finished,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TurnEvent {
    pub kind: String,
    pub body: String,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnSnapshot {
    pub turn_id: String,
    pub session_id: String,
    pub project_id: String,
    pub prompt: String,
    pub state: TurnState,
    pub started_at_ms: i64,
    pub head: Option<String>,
    pub events: Vec<TurnEvent>,
    pub decisions: Vec<String>,
    pub files: Vec<String>,
    pub symbols: Vec<String>,
    pub outcome: Option<String>,
    pub memory_id: Option<String>,
    pub finish_receipt: Option<Value>,
    /// Files git reported as dirty when the turn began, with a size and mtime
    /// fingerprint; finish leaves them out unless they changed during the turn.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dirty_at_begin: BTreeMap<String, String>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A new turn id: the session, the time, this process and a per-process
/// counter, so two begins in the same millisecond never share an id.
fn mint_turn_id(session: &str) -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let raw = format!("{session}\n{nanos}\n{}\n{seq}", std::process::id());
    format!("trn_{}", &sha256_hex(&raw)[..16])
}

/// Hex SHA-256 of `input`, used for stable short ids.
fn sha256_hex(input: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    hex::encode(h.finalize())
}

fn turns_dir(root: &Path) -> PathBuf {
    root.join(".ax").join("cli-turns")
}

/// `raw` when it is usable as one path segment under `.ax/cli-turns`: the
/// session-id rule (ASCII letters, digits and `-_.:`, no surrounding space) and
/// not made of dots only, so it cannot name a parent folder or an absolute path.
fn checked_id<'a>(raw: &'a str, what: &str) -> Result<&'a str, String> {
    let usable = crate::reuse_cache::usable_session(raw).as_deref() == Some(raw)
        && !raw.chars().all(|c| c == '.');
    if usable {
        Ok(raw)
    } else {
        Err(format!(
            "invalid {what} id {raw:?}: use 1-128 ASCII letters, digits, '-', '_', '.' or ':'"
        ))
    }
}

fn session_dir(root: &Path, session: &str) -> Result<PathBuf, String> {
    Ok(turns_dir(root).join(checked_id(session, "session")?))
}

fn snapshot_path(root: &Path, session: &str, turn_id: &str) -> Result<PathBuf, String> {
    let turn_id = checked_id(turn_id, "turn")?;
    Ok(session_dir(root, session)?.join(format!("{turn_id}.json")))
}

fn io_err(message: String) -> std::io::Error {
    std::io::Error::other(message)
}

fn encode_snapshot(snap: &TurnSnapshot) -> std::io::Result<Vec<u8>> {
    serde_json::to_vec_pretty(snap).map_err(|e| io_err(e.to_string()))
}

/// Writes a new turn's snapshot under the turn's lock; an existing snapshot
/// for the same id is an error, never overwritten.
fn create_snapshot(root: &Path, snap: &TurnSnapshot) -> Result<(), String> {
    let path = snapshot_path(root, &snap.session_id, &snap.turn_id)?;
    ax_utils::update_locked(&path, |current| match current {
        Some(_) => Err(io_err(format!("turn {} already exists", snap.turn_id))),
        None => encode_snapshot(snap).map(Some),
    })
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Reads, changes and writes one turn's snapshot while holding the turn's
/// lock, so concurrent writers to the same turn never drop each other's
/// changes. `change` sees the current snapshot; its error leaves the file as it was.
fn modify_snapshot<F>(
    root: &Path,
    session: &str,
    turn_id: &str,
    change: F,
) -> Result<TurnSnapshot, String>
where
    F: FnOnce(&mut TurnSnapshot) -> Result<(), String>,
{
    let path = snapshot_path(root, session, turn_id)?;
    if !path.is_file() {
        return Err(format!("turn not found: {turn_id}"));
    }
    let mut saved = None;
    ax_utils::update_locked(&path, |current| {
        let raw = current.ok_or_else(|| io_err(format!("turn not found: {turn_id}")))?;
        let mut snap: TurnSnapshot =
            serde_json::from_slice(&raw).map_err(|e| io_err(e.to_string()))?;
        change(&mut snap).map_err(io_err)?;
        let body = encode_snapshot(&snap)?;
        saved = Some(snap);
        Ok(Some(body))
    })
    .map_err(|e| e.to_string())?;
    saved.ok_or_else(|| format!("turn not found: {turn_id}"))
}

fn read_snapshot(root: &Path, session: &str, turn_id: &str) -> Result<TurnSnapshot, String> {
    let path = snapshot_path(root, session, turn_id)?;
    let raw = fs::read_to_string(&path).map_err(|e| format!("turn not found: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

/// How long one git call may take before the turn goes on without its answer.
const GIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Most changed files a turn records; more are counted in a receipt warning.
const MAX_TURN_FILES: usize = 200;

/// Stdout of a successful `git` call in `root`, or `None` when git is missing,
/// fails, or runs past [`GIT_TIMEOUT`] (the child is killed).
async fn git(root: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let child = tokio::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(GIT_TIMEOUT, child).await {
        Ok(Ok(out)) if out.status.success() => Some(out.stdout),
        Ok(_) => None,
        Err(_) => {
            tracing::warn!("git {} timed out after {GIT_TIMEOUT:?}", args.join(" "));
            None
        }
    }
}

async fn git_head(root: &Path) -> Option<String> {
    let out = git(root, &["rev-parse", "HEAD"]).await?;
    let s = String::from_utf8_lossy(&out).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Paths in NUL-separated `git status --porcelain=v1 -z` output. A rename or
/// copy record is followed by its source path, which is skipped: the record's
/// own path is the one that exists now.
fn parse_status_z(raw: &[u8]) -> Vec<String> {
    let mut paths = Vec::new();
    let mut records = raw.split(|b| *b == 0);
    while let Some(record) = records.next() {
        if record.len() < 4 {
            continue;
        }
        let (x, y) = (record[0], record[1]);
        paths.push(String::from_utf8_lossy(&record[3..]).into_owned());
        if matches!(x, b'R' | b'C') || matches!(y, b'R' | b'C') {
            records.next();
        }
    }
    paths
}

/// Paths in NUL-separated `git diff --name-only -z` output.
fn parse_names_z(raw: &[u8]) -> Vec<String> {
    raw.split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect()
}

/// Where `root` sits inside its repository, as git prints it (`app/`, or empty
/// when `root` is the repository top level).
async fn git_prefix(root: &Path) -> Option<String> {
    let out = git(root, &["rev-parse", "--show-prefix"]).await?;
    Some(
        String::from_utf8_lossy(&out)
            .trim_end_matches(['\n', '\r'])
            .to_string(),
    )
}

/// Every path under `root` that git reports as different from `head` or the
/// index, untracked included. Paths are relative to `root`, also when `root` is
/// a subfolder of the repository; changes outside `root` are left out.
async fn git_dirty_paths(root: &Path, head: Option<&str>) -> Vec<String> {
    let mut files = Vec::new();
    let Some(prefix) = git_prefix(root).await else {
        return files;
    };
    let mut diff = vec!["diff", "--name-only", "-z", "--relative"];
    if let Some(h) = head {
        diff.push(h);
    }
    diff.extend(["--", "."]);
    if let Some(out) = git(root, &diff).await {
        files.extend(parse_names_z(&out));
    }
    // `git status` prints paths from the top level even with a pathspec.
    let status = ["status", "--porcelain=v1", "-z", "-uall", "--", "."];
    if let Some(out) = git(root, &status).await {
        files.extend(
            parse_status_z(&out)
                .into_iter()
                .filter_map(|path| path.strip_prefix(prefix.as_str()).map(str::to_owned))
                .filter(|path| !path.is_empty()),
        );
    }
    files.sort();
    files.dedup();
    files
}

/// Size and modification time of `path` under `root`, or `missing`.
fn file_fingerprint(root: &Path, path: &str) -> String {
    match fs::symlink_metadata(root.join(path)) {
        Ok(meta) => {
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            format!("{}:{modified}", meta.len())
        }
        Err(_) => "missing".into(),
    }
}

/// Files that were already dirty when the turn began, with their fingerprints.
async fn dirty_snapshot(root: &Path, head: Option<&str>) -> BTreeMap<String, String> {
    git_dirty_paths(root, head)
        .await
        .into_iter()
        .map(|path| {
            let fingerprint = file_fingerprint(root, &path);
            (path, fingerprint)
        })
        .collect()
}

/// Files changed since the turn began: dirty now, minus files that were dirty at
/// the start and are unchanged since. The second value is how many files there
/// were before the list was cut to [`MAX_TURN_FILES`].
async fn git_changed_since(
    root: &Path,
    head: Option<&str>,
    dirty_at_begin: &BTreeMap<String, String>,
) -> (Vec<String>, usize) {
    let mut files: Vec<String> = git_dirty_paths(root, head)
        .await
        .into_iter()
        .filter(|path| {
            dirty_at_begin
                .get(path)
                .is_none_or(|before| *before != file_fingerprint(root, path))
        })
        .collect();
    let total = files.len();
    files.truncate(MAX_TURN_FILES);
    (files, total)
}

async fn git_commits_since(root: &Path, head: Option<&str>) -> Vec<String> {
    let Some(h) = head else {
        return Vec::new();
    };
    let range = format!("{h}..HEAD");
    let Some(out) = git(root, &["log", "--format=%H", &range]).await else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(50)
        .map(str::to_owned)
        .collect()
}

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// Begin a turn: create snapshot + activity event.
pub async fn turn_begin(
    root: &Path,
    session_id: &str,
    prompt: &str,
    agent: Option<&str>,
) -> Result<TurnSnapshot, String> {
    checked_id(session_id, "session")?;
    let turn_id = mint_turn_id(session_id);
    let head = git_head(root).await;
    let dirty_at_begin = dirty_snapshot(root, head.as_deref()).await;
    let snap = TurnSnapshot {
        turn_id: turn_id.clone(),
        session_id: session_id.to_string(),
        project_id: project_scope(root),
        prompt: clip(&crate::redact::redact_text(prompt), 8_000),
        state: TurnState::Open,
        started_at_ms: now_ms(),
        head,
        events: Vec::new(),
        decisions: Vec::new(),
        files: Vec::new(),
        symbols: Vec::new(),
        outcome: None,
        memory_id: None,
        finish_receipt: None,
        dirty_at_begin,
    };
    create_snapshot(root, &snap)?;
    let _ = record_activity(
        root,
        ActivityRecord {
            idempotency_key: format!("turn.begin:{}:{}", session_id, turn_id),
            source: Some(ActivitySource::Cli),
            agent: agent.map(str::to_owned),
            session_id: Some(session_id.to_string()),
            turn_id: Some(turn_id),
            operation: "turn.started".into(),
            status: Some(ActivityStatus::Incomplete),
            summary: format!("turn begin: {}", clip(prompt, 120)),
            ..ActivityRecord::default()
        },
    )
    .await;
    Ok(snap)
}

#[derive(Debug, Clone, Default)]
pub struct TurnEventInput {
    pub kind: String,
    pub body: String,
    pub files: Vec<String>,
    pub symbols: Vec<String>,
    pub decisions: Vec<String>,
}

/// Append an agent-reported event to an open turn.
pub async fn turn_event(
    root: &Path,
    session_id: &str,
    turn_id: &str,
    input: TurnEventInput,
) -> Result<TurnSnapshot, String> {
    let kind = if input.kind.trim().is_empty() {
        "note".to_string()
    } else {
        input.kind.trim().to_string()
    };
    let snap = modify_snapshot(root, session_id, turn_id, |snap| {
        if snap.state != TurnState::Open {
            return Err(format!("turn is {:?}; cannot append events", snap.state));
        }
        snap.events.push(TurnEvent {
            kind: kind.clone(),
            body: clip(&crate::redact::redact_text(&input.body), 4_000),
            at_ms: now_ms(),
        });
        for f in &input.files {
            if !f.is_empty() && !snap.files.iter().any(|x| x == f) {
                snap.files.push(f.clone());
            }
        }
        for s in &input.symbols {
            if !s.is_empty() && !snap.symbols.iter().any(|x| x == s) {
                snap.symbols.push(s.clone());
            }
        }
        for d in &input.decisions {
            if !d.is_empty() && !snap.decisions.iter().any(|x| x == d) {
                snap.decisions.push(d.clone());
            }
        }
        Ok(())
    })?;
    let _ = record_activity(
        root,
        ActivityRecord {
            idempotency_key: format!("turn.event:{}:{}:{}", turn_id, kind, snap.events.len()),
            source: Some(ActivitySource::Agent),
            session_id: Some(session_id.to_string()),
            turn_id: Some(turn_id.to_string()),
            operation: format!("turn.event.{kind}"),
            status: Some(ActivityStatus::Success),
            summary: clip(&input.body, 160),
            files: snap.files.clone(),
            symbols: snap.symbols.clone(),
            ..ActivityRecord::default()
        },
    )
    .await;
    Ok(snap)
}

/// Checkpoint open turn (activity + durable-friendly JSON).
pub async fn turn_checkpoint(
    root: &Path,
    session_id: &str,
    turn_id: &str,
    note: &str,
) -> Result<TurnSnapshot, String> {
    let snap = modify_snapshot(root, session_id, turn_id, |snap| {
        if snap.state != TurnState::Open {
            return Err(format!("turn is {:?}; cannot checkpoint", snap.state));
        }
        snap.events.push(TurnEvent {
            kind: "checkpoint".into(),
            body: clip(&crate::redact::redact_text(note), 4_000),
            at_ms: now_ms(),
        });
        Ok(())
    })?;
    let _ = record_activity(
        root,
        ActivityRecord {
            idempotency_key: format!("turn.checkpoint:{}:{}", turn_id, snap.events.len()),
            source: Some(ActivitySource::Cli),
            session_id: Some(session_id.to_string()),
            turn_id: Some(turn_id.to_string()),
            operation: "task.checkpointed".into(),
            status: Some(ActivityStatus::Incomplete),
            summary: clip(note, 160),
            ..ActivityRecord::default()
        },
    )
    .await;
    Ok(snap)
}

#[derive(Debug, Clone, Default)]
pub struct TurnFinishInput {
    pub outcome: String,
    pub decisions: Vec<String>,
    pub open_questions: Vec<String>,
    pub semantic_memories: Vec<SemanticMemoryInput>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SemanticMemoryInput {
    pub kind: String,
    pub title: String,
    pub body: String,
    pub files: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnFinishReceipt {
    pub turn_id: String,
    pub session_id: String,
    pub memory_id: Option<String>,
    pub files: Vec<String>,
    pub commits: Vec<String>,
    pub state: String,
    pub idempotent: bool,
    pub semantic_memory_ids: Vec<String>,
    /// Steps after the turn memory that did not complete, such as the working-context write.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Finish a turn. Idempotent: a second finish returns the prior receipt.
pub async fn turn_finish(
    root: &Path,
    pool: &SqlitePool,
    session_id: &str,
    turn_id: &str,
    input: TurnFinishInput,
) -> Result<TurnFinishReceipt, String> {
    let mut snap = read_snapshot(root, session_id, turn_id)?;
    if snap.state == TurnState::Finished {
        if let Some(receipt) = snap.finish_receipt.clone() {
            let mut parsed: TurnFinishReceipt =
                serde_json::from_value(receipt).map_err(|e| e.to_string())?;
            parsed.idempotent = true;
            return Ok(parsed);
        }
        return Ok(TurnFinishReceipt {
            turn_id: turn_id.to_string(),
            session_id: session_id.to_string(),
            memory_id: snap.memory_id.clone(),
            files: snap.files.clone(),
            commits: Vec::new(),
            state: "finished".into(),
            idempotent: true,
            semantic_memory_ids: Vec::new(),
            warnings: Vec::new(),
        });
    }
    if snap.state == TurnState::Failed {
        return Err("turn already failed".into());
    }

    let (changed, changed_total) =
        git_changed_since(root, snap.head.as_deref(), &snap.dirty_at_begin).await;
    let commits = git_commits_since(root, snap.head.as_deref()).await;
    let mut warnings = Vec::new();
    if changed_total > changed.len() {
        warnings.push(format!(
            "changed files cut to {} of {changed_total}",
            changed.len()
        ));
    }
    for f in &changed {
        if !snap.files.iter().any(|x| x == f) {
            snap.files.push(f.clone());
        }
    }
    for d in &input.decisions {
        if !d.is_empty() && !snap.decisions.iter().any(|x| x == d) {
            snap.decisions.push(d.clone());
        }
    }
    let outcome = clip(&crate::redact::redact_text(&input.outcome), 20_000);
    snap.outcome = Some(outcome.clone());

    // Written before the turn is marked finished, so a finished turn always has
    // its memory. Its id comes from the session and turn, so racing finishes
    // share one row; the winner rewrites it below.
    let memory_id = save_cli_turn_memory(pool, &snap, &commits, &outcome, false).await?;
    snap.memory_id = memory_id.clone();

    let mut semantic_ids = Vec::new();
    let mut inserted_ids = Vec::new();
    for mem in &input.semantic_memories {
        if mem.body.trim().is_empty() {
            continue;
        }
        // Classification is deterministic from explicit kind; uncertain kinds become notes.
        let kind = normalize_kind(&mem.kind);
        match save_semantic_memory(pool, mem, kind, session_id, turn_id).await {
            Ok((id, inserted)) => {
                if inserted {
                    inserted_ids.push(id.clone());
                }
                semantic_ids.push(id);
            }
            Err(e) => {
                // Surface but do not abort the turn finish after the turn memory exists.
                tracing::warn!("semantic memory skipped: {e}");
            }
        }
    }

    // Write the finish onto the snapshot as it is now, under the turn's lock:
    // events added while finish ran are kept, and a finish that won a race
    // with this one is returned as the receipt instead of being overwritten.
    let mut receipt = None;
    let mut already = None;
    let snap = modify_snapshot(root, session_id, turn_id, |current| {
        match current.state {
            TurnState::Failed => return Err("turn already failed".into()),
            TurnState::Finished => {
                already = current.finish_receipt.clone();
                return Ok(());
            }
            TurnState::Open => {}
        }
        for f in &snap.files {
            if !current.files.iter().any(|x| x == f) {
                current.files.push(f.clone());
            }
        }
        for d in &snap.decisions {
            if !current.decisions.iter().any(|x| x == d) {
                current.decisions.push(d.clone());
            }
        }
        current.outcome = Some(outcome.clone());
        current.memory_id = memory_id.clone();
        let made = TurnFinishReceipt {
            turn_id: turn_id.to_string(),
            session_id: session_id.to_string(),
            memory_id: memory_id.clone(),
            files: current.files.clone(),
            commits: commits.clone(),
            state: "finished".into(),
            idempotent: false,
            semantic_memory_ids: semantic_ids.clone(),
            warnings: warnings.clone(),
        };
        current.state = TurnState::Finished;
        current.finish_receipt = Some(serde_json::to_value(&made).unwrap_or(json!({})));
        receipt = Some(made);
        Ok(())
    })?;
    let Some(receipt) = receipt else {
        let mut parsed: TurnFinishReceipt = match already {
            Some(value) => serde_json::from_value(value).map_err(|e| e.to_string())?,
            None => TurnFinishReceipt {
                turn_id: turn_id.to_string(),
                session_id: session_id.to_string(),
                memory_id: snap.memory_id.clone(),
                files: snap.files.clone(),
                commits: Vec::new(),
                state: "finished".into(),
                idempotent: true,
                semantic_memory_ids: Vec::new(),
                warnings: Vec::new(),
            },
        };
        parsed.idempotent = true;
        // Another finish won: drop the semantic memories only this one added.
        for id in inserted_ids
            .iter()
            .filter(|id| !parsed.semantic_memory_ids.contains(id))
        {
            if let Err(error) = sqlx::query("DELETE FROM memories WHERE id = ?1")
                .bind(id)
                .execute(pool)
                .await
            {
                tracing::warn!("stray semantic memory {id} not removed: {error}");
            }
        }
        return Ok(parsed);
    };
    // A losing finish may have written the shared turn memory first; store this one's.
    if let Err(error) = save_cli_turn_memory(pool, &snap, &commits, &outcome, true).await {
        tracing::warn!("turn memory not rewritten: {error}");
    }

    let activity_summary = match &memory_id {
        Some(mid) => format!("{} memory={mid}", clip(&outcome, 160)),
        None => clip(&outcome, 200),
    };
    let _ = record_activity(
        root,
        ActivityRecord {
            idempotency_key: format!("turn.finish:{turn_id}"),
            source: Some(ActivitySource::Cli),
            session_id: Some(session_id.to_string()),
            turn_id: Some(turn_id.to_string()),
            operation: "turn.completed".into(),
            status: Some(ActivityStatus::Success),
            summary: activity_summary,
            files: snap.files.clone(),
            commit_ids: commits,
            token_metrics: TokenMetrics::default(),
            ..ActivityRecord::default()
        },
    )
    .await;

    if let Err(error) = crate::spec_registry::on_turn_finished(
        pool,
        root,
        &snap.project_id,
        session_id,
        turn_id,
        &snap.prompt,
        &outcome,
        &snap.files,
    )
    .await
    {
        tracing::warn!("spec capture skipped: {error}");
    }

    let mut receipt = receipt;
    if let Err(error) = note_working_context(root, pool, session_id, &input).await {
        tracing::warn!("working context not updated: {error}");
        let warning = format!("working context not updated: {error}");
        receipt.warnings.push(warning.clone());
        // Keep the warning in the stored receipt, so a retried finish reports it too.
        let stored = modify_snapshot(root, session_id, turn_id, |current| {
            if let Some(value) = current.finish_receipt.as_mut() {
                let mut parsed: TurnFinishReceipt =
                    serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
                if !parsed.warnings.contains(&warning) {
                    parsed.warnings.push(warning.clone());
                }
                *value = serde_json::to_value(&parsed).map_err(|e| e.to_string())?;
            }
            Ok(())
        });
        if let Err(error) = stored {
            tracing::warn!("finish receipt warning not stored: {error}");
        }
    }

    Ok(receipt)
}

/// Store the turn's decisions and open questions in the session's working context, stamped
/// with the current index fingerprint (the store refuses notes without one).
async fn note_working_context(
    root: &Path,
    pool: &SqlitePool,
    session_id: &str,
    input: &TurnFinishInput,
) -> Result<(), String> {
    if (input.decisions.is_empty() && input.open_questions.is_empty()) || !crate::reuse_enabled() {
        return Ok(());
    }
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT path, content_hash FROM files")
        .fetch_all(pool)
        .await
        .map_err(|e| format!("index unavailable ({e}); retry after ax_sync"))?;
    let index: crate::IndexHashes = rows.into_iter().collect();
    let fingerprint = crate::index_fingerprint(&index);
    crate::working_context_apply(
        root,
        session_id,
        &json!({
            "action": "add",
            "decisions": input.decisions,
            "open_questions": input.open_questions,
        }),
        &fingerprint,
    )
    .await
    .map(|_| ())
}

fn normalize_kind(kind: &str) -> &'static str {
    match kind.trim().to_ascii_lowercase().as_str() {
        "decision" | "decisions" => "decision",
        "bug_fix" | "bugfix" | "fix" => "bug_fix",
        "architecture" | "arch" => "architecture",
        "convention" | "conventions" => "convention",
        "note" | "notes" => "note",
        _ => "note",
    }
}

async fn save_cli_turn_memory(
    pool: &SqlitePool,
    snap: &TurnSnapshot,
    commits: &[String],
    outcome: &str,
    overwrite: bool,
) -> Result<Option<String>, String> {
    let title = snap
        .prompt
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| clip(l, 80))
        .unwrap_or_else(|| "Agent research turn".into());

    let mut body = snap.prompt.clone();
    if !snap.files.is_empty() {
        body.push_str(&format!(
            "\n\nFiles: {}",
            snap.files
                .iter()
                .take(30)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !commits.is_empty() {
        body.push_str("\n\nCommits:");
        for c in commits.iter().take(20) {
            body.push_str(&format!("\n- {c}"));
        }
    }
    if !snap.decisions.is_empty() {
        body.push_str("\n\nDecisions:");
        for d in &snap.decisions {
            body.push_str(&format!("\n- {d}"));
        }
    }
    if !snap.events.is_empty() {
        body.push_str("\n\nEvents:");
        for e in snap.events.iter().take(40) {
            body.push_str(&format!("\n- [{}] {}", e.kind, clip(&e.body, 200)));
        }
    }
    body.push_str("\n\nConversation: ");
    body.push_str(&sha256_hex(&snap.session_id)[..12]);
    if !outcome.is_empty() {
        body.push_str("\n\nOutcome: ");
        body.push_str(outcome);
    }

    let id = format!(
        "turn-{}",
        &sha256_hex(&format!("{}:{}", snap.session_id, snap.turn_id))[..16]
    );
    let files_json = serde_json::to_string(&snap.files).unwrap_or_else(|_| "[]".into());
    let now = now_ms();
    let sql = if overwrite {
        r#"INSERT INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, embedding, enabled)
           VALUES (?, 'turn', ?, ?, '[]', ?, 1.0, ?, ?, ?, NULL, 1)
           ON CONFLICT(id) DO UPDATE SET title = excluded.title, body = excluded.body,
               files = excluded.files, updated_at = excluded.updated_at"#
    } else {
        r#"INSERT OR IGNORE INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, embedding, enabled)
           VALUES (?, 'turn', ?, ?, '[]', ?, 1.0, ?, ?, ?, NULL, 1)"#
    };
    sqlx::query(sql)
        .bind(&id)
        .bind(&title)
        .bind(&body)
        .bind(&files_json)
        .bind(TURN_SOURCE_CLI)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    // The id is returned whether the row was inserted or already present.
    Ok(Some(id))
}

async fn save_semantic_memory(
    pool: &SqlitePool,
    mem: &SemanticMemoryInput,
    kind: &str,
    session_id: &str,
    turn_id: &str,
) -> Result<(String, bool), String> {
    let title = if mem.title.trim().is_empty() {
        clip(&mem.body, 80)
    } else {
        clip(&mem.title, 120)
    };
    let body = crate::redact::redact_text(&mem.body);
    let id = format!(
        "mem-{}",
        &sha256_hex(&format!("{session_id}:{turn_id}:{kind}:{title}:{body}"))[..16]
    );
    let tags = serde_json::to_string(&mem.tags).unwrap_or_else(|_| "[]".into());
    let files = serde_json::to_string(&mem.files).unwrap_or_else(|_| "[]".into());
    let now = now_ms();
    let result = sqlx::query(
        r#"INSERT OR IGNORE INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, embedding, enabled)
           VALUES (?, ?, ?, ?, ?, ?, 0.85, ?, ?, ?, NULL, 1)"#,
    )
    .bind(&id)
    .bind(kind)
    .bind(&title)
    .bind(&body)
    .bind(&tags)
    .bind(&files)
    .bind(TURN_SOURCE_CLI)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    // Duplicate content reports the existing id without rewriting it.
    Ok((id, result.rows_affected() > 0))
}

/// Mark a turn failed without writing a successful memory.
pub async fn turn_fail(
    root: &Path,
    session_id: &str,
    turn_id: &str,
    reason: &str,
) -> Result<TurnSnapshot, String> {
    let snap = modify_snapshot(root, session_id, turn_id, |snap| {
        if snap.state != TurnState::Open {
            return Err(format!("turn is {:?}; cannot fail", snap.state));
        }
        snap.state = TurnState::Failed;
        snap.outcome = Some(clip(&crate::redact::redact_text(reason), 4_000));
        Ok(())
    })?;
    let _ = record_activity(
        root,
        ActivityRecord {
            idempotency_key: format!("turn.fail:{turn_id}"),
            source: Some(ActivitySource::Cli),
            session_id: Some(session_id.to_string()),
            turn_id: Some(turn_id.to_string()),
            operation: "turn.failed".into(),
            status: Some(ActivityStatus::Failed),
            summary: clip(reason, 200),
            error: Some(clip(reason, 500)),
            ..ActivityRecord::default()
        },
    )
    .await;
    Ok(snap)
}

/// Status of a turn, or list open turns for a session when turn_id is None.
pub fn turn_status(root: &Path, session_id: &str, turn_id: Option<&str>) -> Result<Value, String> {
    if let Some(id) = turn_id {
        let snap = read_snapshot(root, session_id, id)?;
        return serde_json::to_value(snap).map_err(|e| e.to_string());
    }
    let dir = session_dir(root, session_id)?;
    let mut open = Vec::new();
    let mut finished = Vec::new();
    let mut failed = Vec::new();
    if dir.is_dir() {
        for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(snap) = serde_json::from_str::<TurnSnapshot>(&raw) {
                    match snap.state {
                        TurnState::Open => open.push(snap.turn_id),
                        TurnState::Finished => finished.push(snap.turn_id),
                        TurnState::Failed => failed.push(snap.turn_id),
                    }
                }
            }
        }
    }
    Ok(json!({
        "sessionId": session_id,
        "open": open,
        "finished": finished,
        "failed": failed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::open_pool_at;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SEQ: AtomicUsize = AtomicUsize::new(0);

    /// Holds [`crate::store::USAGE_ENV_LOCK`] and puts `AX_USAGE_DB` back as it
    /// was when dropped.
    struct UsageDbEnv {
        previous: Option<std::ffi::OsString>,
        _lock: tokio::sync::MutexGuard<'static, ()>,
    }

    impl Drop for UsageDbEnv {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => std::env::set_var("AX_USAGE_DB", value),
                None => std::env::remove_var("AX_USAGE_DB"),
            }
        }
    }

    async fn fixture() -> (PathBuf, SqlitePool, UsageDbEnv) {
        let lock = crate::store::USAGE_ENV_LOCK.lock().await;
        let guard = UsageDbEnv {
            previous: std::env::var_os("AX_USAGE_DB"),
            _lock: lock,
        };
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("ax-turn-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".ax")).unwrap();
        // Minimal memories table for finish tests.
        let db = dir.join(".ax").join("ax.db");
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite://{}?mode=rwc", db.display()))
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE memories (
                id TEXT PRIMARY KEY, kind TEXT, title TEXT, body TEXT, tags TEXT,
                files TEXT, confidence REAL, source TEXT, created_at INTEGER,
                updated_at INTEGER, embedding BLOB, enabled INTEGER)",
        )
        .execute(&pool)
        .await
        .unwrap();
        std::env::set_var("AX_USAGE_DB", dir.join("usage.db").display().to_string());
        let _ = open_pool_at(&dir.join("usage.db")).await.unwrap();
        (dir, pool, guard)
    }

    #[tokio::test]
    async fn research_turn_without_files_is_saved() {
        let (root, pool, _guard) = fixture().await;
        let begin = turn_begin(&root, "sess-a", "Why does auth fail after refresh?", None)
            .await
            .unwrap();
        turn_event(
            &root,
            "sess-a",
            &begin.turn_id,
            TurnEventInput {
                kind: "finding".into(),
                body: "Token cookie cleared on soft reload".into(),
                ..TurnEventInput::default()
            },
        )
        .await
        .unwrap();
        let receipt = turn_finish(
            &root,
            &pool,
            "sess-a",
            &begin.turn_id,
            TurnFinishInput {
                outcome: "Root cause: session cookie SameSite mismatch.".into(),
                decisions: vec!["Use Lax SameSite for SPA refresh".into()],
                ..TurnFinishInput::default()
            },
        )
        .await
        .unwrap();
        assert!(receipt.memory_id.is_some());
        assert!(receipt.files.is_empty());
        let again = turn_finish(
            &root,
            &pool,
            "sess-a",
            &begin.turn_id,
            TurnFinishInput {
                outcome: "should not rewrite".into(),
                ..TurnFinishInput::default()
            },
        )
        .await
        .unwrap();
        assert!(again.idempotent);
        assert_eq!(again.memory_id, receipt.memory_id);
    }

    #[tokio::test]
    async fn redaction_keeps_the_line_structure_of_prompts_and_outcomes() {
        let (root, pool, _guard) = fixture().await;
        let begin = turn_begin(
            &root,
            "sess-c",
            "Draft the rollout\n\nUse TOKEN=hunter2secret for staging.",
            None,
        )
        .await
        .unwrap();
        assert_eq!(
            begin.prompt,
            "Draft the rollout\n\nUse TOKEN=[redacted] for staging."
        );
        let outcome = "# Rollout\n\n## Steps\n\n- one\n- two\n\n```sh\nax up\n```";
        turn_finish(
            &root,
            &pool,
            "sess-c",
            &begin.turn_id,
            TurnFinishInput {
                outcome: outcome.into(),
                ..TurnFinishInput::default()
            },
        )
        .await
        .unwrap();
        let (title, body): (String, String) =
            sqlx::query_as("SELECT title, body FROM memories WHERE source != 'semantic' LIMIT 1")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(title, "Draft the rollout");
        assert!(body.contains(outcome), "{body}");
        assert!(!body.contains("hunter2secret"), "{body}");
    }

    #[tokio::test]
    async fn finish_stores_decisions_and_open_questions_in_the_working_context() {
        let (root, pool, _guard) = fixture().await;
        sqlx::query("CREATE TABLE files (path TEXT PRIMARY KEY, content_hash TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO files (path, content_hash) VALUES ('src/a.rs', 'h1')")
            .execute(&pool)
            .await
            .unwrap();
        let begin = turn_begin(&root, "sess-wc", "decide cookies", None)
            .await
            .unwrap();
        let receipt = turn_finish(
            &root,
            &pool,
            "sess-wc",
            &begin.turn_id,
            TurnFinishInput {
                outcome: "done".into(),
                decisions: vec!["Use Lax SameSite".into()],
                open_questions: vec!["Does Safari keep the cookie?".into()],
                ..TurnFinishInput::default()
            },
        )
        .await
        .unwrap();
        assert!(receipt.warnings.is_empty(), "{:?}", receipt.warnings);
        let notes = crate::working_context_apply(&root, "sess-wc", &json!({"action": "get"}), "x")
            .await
            .unwrap();
        assert!(notes.contains("Use Lax SameSite"), "{notes}");
        assert!(notes.contains("Does Safari keep the cookie?"), "{notes}");
    }

    #[tokio::test]
    async fn finish_reports_a_working_context_write_it_could_not_make() {
        let (root, pool, _guard) = fixture().await;
        let begin = turn_begin(&root, "sess-wc2", "no index", None)
            .await
            .unwrap();
        let receipt = turn_finish(
            &root,
            &pool,
            "sess-wc2",
            &begin.turn_id,
            TurnFinishInput {
                outcome: "done".into(),
                decisions: vec!["Keep it".into()],
                ..TurnFinishInput::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(receipt.warnings.len(), 1, "{:?}", receipt.warnings);
        assert!(
            receipt.warnings[0].contains("working context"),
            "{:?}",
            receipt.warnings
        );
        // A retried finish is told the same.
        let again = turn_finish(
            &root,
            &pool,
            "sess-wc2",
            &begin.turn_id,
            TurnFinishInput::default(),
        )
        .await
        .unwrap();
        assert!(again.idempotent);
        assert_eq!(again.warnings, receipt.warnings);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn concurrent_finishes_leave_one_turn_memory_and_no_stray_rows() {
        let (root, pool, _guard) = fixture().await;
        let begin = turn_begin(&root, "sess-race", "race", None).await.unwrap();
        let mut tasks = Vec::new();
        for worker in 0..6 {
            let (root, pool, turn) = (root.clone(), pool.clone(), begin.turn_id.clone());
            tasks.push(tokio::spawn(async move {
                turn_finish(
                    &root,
                    &pool,
                    "sess-race",
                    &turn,
                    TurnFinishInput {
                        outcome: format!("outcome {worker}"),
                        semantic_memories: vec![SemanticMemoryInput {
                            kind: "note".into(),
                            title: format!("note {worker}"),
                            body: format!("body {worker}"),
                            ..SemanticMemoryInput::default()
                        }],
                        ..TurnFinishInput::default()
                    },
                )
                .await
                .unwrap()
            }));
        }
        let mut receipts = Vec::new();
        for task in tasks {
            receipts.push(task.await.unwrap());
        }
        let winners: Vec<_> = receipts.iter().filter(|r| !r.idempotent).collect();
        assert_eq!(winners.len(), 1, "{receipts:?}");
        let winner = winners[0];
        let turn_rows: Vec<(String, String)> =
            sqlx::query_as("SELECT id, body FROM memories WHERE kind = 'turn'")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(turn_rows.len(), 1, "{turn_rows:?}");
        assert_eq!(Some(&turn_rows[0].0), winner.memory_id.as_ref());
        let snap = read_snapshot(&root, "sess-race", &begin.turn_id).unwrap();
        let outcome = snap.outcome.unwrap();
        assert!(
            turn_rows[0].1.contains(&outcome),
            "{} / {outcome}",
            turn_rows[0].1
        );
        let semantic: Vec<String> =
            sqlx::query_scalar("SELECT id FROM memories WHERE kind != 'turn' ORDER BY id")
                .fetch_all(&pool)
                .await
                .unwrap();
        let mut expected = winner.semantic_memory_ids.clone();
        expected.sort();
        assert_eq!(semantic, expected);
    }

    fn run_git(root: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }

    fn git_repo(root: &Path) {
        run_git(root, &["init", "-q"]);
        fs::write(
            root.join(".git").join("info").join("exclude"),
            ".ax/\nusage.db*\n",
        )
        .unwrap();
        fs::write(root.join("a.rs"), "a\n").unwrap();
        fs::write(root.join("dirty.rs"), "d\n").unwrap();
        fs::write(root.join("kept.rs"), "k\n").unwrap();
        run_git(root, &["add", "."]);
        run_git(root, &["commit", "-q", "-m", "init"]);
    }

    async fn finish_files(root: &Path, pool: &SqlitePool, turn: &TurnSnapshot) -> Vec<String> {
        turn_finish(
            root,
            pool,
            &turn.session_id,
            &turn.turn_id,
            TurnFinishInput {
                outcome: "done".into(),
                ..TurnFinishInput::default()
            },
        )
        .await
        .unwrap()
        .files
    }

    #[test]
    fn status_records_skip_the_source_path_of_renames_and_copies() {
        let raw = b"R  new.rs\0old.rs\0 M src/a b.rs\0?? d\xc3\xa9.txt\0 C copy.rs\0orig.rs\0";
        assert_eq!(
            parse_status_z(raw),
            vec!["new.rs", "src/a b.rs", "d\u{e9}.txt", "copy.rs"]
        );
        assert_eq!(parse_names_z(b"a.rs\0b c.rs\0"), vec!["a.rs", "b c.rs"]);
    }

    #[tokio::test]
    async fn renamed_and_quoted_paths_are_recorded_as_real_paths() {
        let (root, pool, _guard) = fixture().await;
        git_repo(&root);
        let begin = turn_begin(&root, "sess-git1", "rename", None)
            .await
            .unwrap();
        run_git(&root, &["mv", "a.rs", "b.rs"]);
        fs::write(root.join("my file é.txt"), "x").unwrap();
        let files = finish_files(&root, &pool, &begin).await;
        assert!(files.contains(&"b.rs".to_string()), "{files:?}");
        assert!(files.contains(&"my file é.txt".to_string()), "{files:?}");
        for file in &files {
            assert!(root.join(file).exists(), "not a real path: {file:?}");
        }
    }

    #[tokio::test]
    async fn files_dirty_before_the_turn_are_not_attributed_to_it() {
        let (root, pool, _guard) = fixture().await;
        git_repo(&root);
        fs::create_dir_all(root.join("scratch")).unwrap();
        fs::write(root.join("scratch").join("notes.txt"), "old").unwrap();
        fs::write(root.join("dirty.rs"), "dirty before\n").unwrap();
        fs::write(root.join("kept.rs"), "dirty before\n").unwrap();
        let begin = turn_begin(&root, "sess-git2", "edit", None).await.unwrap();
        // Edited again during the turn: it is part of this turn.
        fs::write(
            root.join("dirty.rs"),
            "dirty before, then edited in the turn\n",
        )
        .unwrap();
        fs::write(root.join("new.rs"), "n\n").unwrap();
        let files = finish_files(&root, &pool, &begin).await;
        assert!(files.contains(&"new.rs".to_string()), "{files:?}");
        assert!(files.contains(&"dirty.rs".to_string()), "{files:?}");
        assert!(!files.contains(&"kept.rs".to_string()), "{files:?}");
        assert!(!files.iter().any(|f| f.starts_with("scratch")), "{files:?}");
    }

    #[tokio::test]
    async fn a_project_in_a_subfolder_of_the_repo_records_root_relative_paths() {
        let (root, pool, _guard) = fixture().await;
        // The repository top level is `root`; the ax project is `root/app`.
        git_repo(&root);
        let project = root.join("app");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("dirty.rs"), "d\n").unwrap();
        fs::write(project.join("kept.rs"), "k\n").unwrap();
        fs::write(project.join("a.rs"), "a\n").unwrap();
        run_git(&root, &["add", "."]);
        run_git(&root, &["commit", "-q", "-m", "app"]);
        fs::write(project.join("dirty.rs"), "dirty before\n").unwrap();
        fs::write(project.join("kept.rs"), "dirty before\n").unwrap();
        let begin = turn_begin(&project, "sess-sub", "edit", None)
            .await
            .unwrap();
        fs::write(
            project.join("dirty.rs"),
            "dirty before, then edited in the turn\n",
        )
        .unwrap();
        fs::write(project.join("my file é.txt"), "x").unwrap();
        run_git(&project, &["mv", "a.rs", "b.rs"]);
        // A change outside the project folder is not part of this project.
        fs::write(root.join("new-outside.rs"), "o\n").unwrap();
        let files = finish_files(&project, &pool, &begin).await;
        assert!(files.contains(&"dirty.rs".to_string()), "{files:?}");
        assert!(files.contains(&"b.rs".to_string()), "{files:?}");
        assert!(files.contains(&"my file é.txt".to_string()), "{files:?}");
        assert!(!files.contains(&"kept.rs".to_string()), "{files:?}");
        for file in &files {
            assert!(project.join(file).exists(), "not a real path: {file:?}");
        }
    }

    #[tokio::test]
    async fn session_and_turn_ids_cannot_leave_the_turns_folder() {
        let (root, pool, _guard) = fixture().await;
        let outside = root.join("outside");
        for bad in ["../../outside", "..", ".", "/tmp/abs", "a/b", "", " s "] {
            assert!(
                turn_begin(&root, bad, "p", None).await.is_err(),
                "session {bad:?} was accepted"
            );
            assert!(turn_status(&root, bad, None).is_err(), "status {bad:?}");
        }
        assert!(
            !outside.exists(),
            "a snapshot was written outside .ax/cli-turns"
        );

        // A snapshot-shaped file outside the turns folder must not be readable
        // or rewritable through a turn id.
        let begin = turn_begin(&root, "sess-p", "p", None).await.unwrap();
        let mut planted = begin.clone();
        planted.turn_id = "planted".into();
        fs::create_dir_all(root.join(".ax")).unwrap();
        fs::write(
            root.join(".ax").join("planted.json"),
            serde_json::to_vec(&planted).unwrap(),
        )
        .unwrap();
        let escape = "../../planted";
        assert!(turn_status(&root, "sess-p", Some(escape)).is_err());
        assert!(turn_fail(&root, "sess-p", escape, "x").await.is_err());
        assert!(
            turn_event(&root, "sess-p", escape, TurnEventInput::default())
                .await
                .is_err()
        );
        assert!(turn_checkpoint(&root, "sess-p", escape, "x").await.is_err());
        assert!(
            turn_finish(&root, &pool, "sess-p", escape, TurnFinishInput::default())
                .await
                .is_err()
        );
        let after: TurnSnapshot =
            serde_json::from_slice(&fs::read(root.join(".ax").join("planted.json")).unwrap())
                .unwrap();
        assert_eq!(after.state, TurnState::Open);
    }

    #[tokio::test]
    async fn incomplete_turn_is_visible() {
        let (root, _pool, _guard) = fixture().await;
        let begin = turn_begin(&root, "sess-b", "open question", None)
            .await
            .unwrap();
        let status = turn_status(&root, "sess-b", None).unwrap();
        let open = status["open"].as_array().unwrap();
        assert!(open
            .iter()
            .any(|v| v.as_str() == Some(begin.turn_id.as_str())));
    }

    #[test]
    fn turn_ids_minted_in_the_same_millisecond_differ() {
        let ids: std::collections::HashSet<String> =
            (0..1_000).map(|_| mint_turn_id("sess-same")).collect();
        assert_eq!(ids.len(), 1_000);
    }

    #[tokio::test]
    async fn begin_never_overwrites_an_existing_turn() {
        let (root, _pool, _guard) = fixture().await;
        let first = turn_begin(&root, "sess-o", "first", None).await.unwrap();
        let mut copy = first.clone();
        copy.prompt = "second".into();
        assert!(create_snapshot(&root, &copy).is_err());
        let kept = read_snapshot(&root, "sess-o", &first.turn_id).unwrap();
        assert_eq!(kept.prompt, "first");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn concurrent_events_on_one_turn_are_all_kept() {
        let (root, _pool, _guard) = fixture().await;
        let begin = turn_begin(&root, "sess-c", "parallel", None).await.unwrap();
        let mut tasks = Vec::new();
        for worker in 0..8 {
            let root = root.clone();
            let turn = begin.turn_id.clone();
            tasks.push(tokio::spawn(async move {
                for n in 0..25 {
                    turn_event(
                        &root,
                        "sess-c",
                        &turn,
                        TurnEventInput {
                            kind: "note".into(),
                            body: format!("w{worker}-e{n}"),
                            ..TurnEventInput::default()
                        },
                    )
                    .await
                    .unwrap();
                }
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        let snap = read_snapshot(&root, "sess-c", &begin.turn_id).unwrap();
        assert_eq!(snap.events.len(), 200);
    }
}
