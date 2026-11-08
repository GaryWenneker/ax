//! Explicit CLI turn lifecycle (no IDE hooks required).
//!
//! Snapshots live under `<project>/.ax/cli-turns/<session>/<turn_id>.json`.
//! Finish writes a turn memory even when no files changed (research turns).

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
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn mint_turn_id(session: &str) -> String {
    let raw = format!("{session}\n{}", now_ms());
    format!("trn_{}", &blake3_hex(&raw)[..16])
}

fn blake3_hex(input: &str) -> String {
    // Avoid a new blake3 dep in ax-usage: use a stable hash via DefaultHasher + salt.
    // Prefer blake3 when available through std isn't; use sha2 already in crate.
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    hex::encode(h.finalize())
}

fn turns_dir(root: &Path) -> PathBuf {
    root.join(".ax").join("cli-turns")
}

fn snapshot_path(root: &Path, session: &str, turn_id: &str) -> PathBuf {
    turns_dir(root).join(session).join(format!("{turn_id}.json"))
}

fn write_snapshot(root: &Path, snap: &TurnSnapshot) -> Result<(), String> {
    let path = snapshot_path(root, &snap.session_id, &snap.turn_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_vec_pretty(snap).map_err(|e| e.to_string())?;
    fs::write(&tmp, body).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

fn read_snapshot(root: &Path, session: &str, turn_id: &str) -> Result<TurnSnapshot, String> {
    let path = snapshot_path(root, session, turn_id);
    let raw = fs::read_to_string(&path).map_err(|e| format!("turn not found: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

fn git_head(root: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn git_changed_since(root: &Path, head: Option<&str>) -> Vec<String> {
    let mut args = vec!["diff", "--name-only"];
    if let Some(h) = head {
        args.push(h);
    }
    let out = match std::process::Command::new("git")
        .args(&args)
        .current_dir(root)
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    let mut files: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    if let Ok(status) = std::process::Command::new("git")
        .args(["status", "--porcelain", "-uall"])
        .current_dir(root)
        .output()
    {
        for line in String::from_utf8_lossy(&status.stdout).lines() {
            let path = line.get(3..).unwrap_or("").trim();
            if !path.is_empty() {
                files.push(path.to_string());
            }
        }
    }
    files.sort();
    files.dedup();
    files.truncate(200);
    files
}

fn git_commits_since(root: &Path, head: Option<&str>) -> Vec<String> {
    let Some(h) = head else {
        return Vec::new();
    };
    let range = format!("{h}..HEAD");
    let out = match std::process::Command::new("git")
        .args(["log", "--format=%H", &range])
        .current_dir(root)
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    String::from_utf8_lossy(&out.stdout)
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
    if session_id.trim().is_empty() {
        return Err("session id required".into());
    }
    let turn_id = mint_turn_id(session_id);
    let snap = TurnSnapshot {
        turn_id: turn_id.clone(),
        session_id: session_id.to_string(),
        project_id: project_scope(root),
        prompt: clip(&crate_redact(prompt), 8_000),
        state: TurnState::Open,
        started_at_ms: now_ms(),
        head: git_head(root),
        events: Vec::new(),
        decisions: Vec::new(),
        files: Vec::new(),
        symbols: Vec::new(),
        outcome: None,
        memory_id: None,
        finish_receipt: None,
    };
    write_snapshot(root, &snap)?;
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

fn crate_redact(text: &str) -> String {
    // Lightweight local redaction (mirrors activity redaction).
    let mut out = String::new();
    for token in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        if token.starts_with("sk-") && token.len() > 20 {
            out.push_str("[redacted]");
        } else if let Some((k, _)) = token.split_once('=') {
            let key = k.to_ascii_lowercase();
            if key.contains("password") || key.contains("token") || key.contains("secret") {
                out.push_str(k);
                out.push_str("=[redacted]");
            } else {
                out.push_str(token);
            }
        } else {
            out.push_str(token);
        }
    }
    out
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
    let mut snap = read_snapshot(root, session_id, turn_id)?;
    if snap.state != TurnState::Open {
        return Err(format!("turn is {:?}; cannot append events", snap.state));
    }
    let kind = if input.kind.trim().is_empty() {
        "note".into()
    } else {
        input.kind.trim().to_string()
    };
    snap.events.push(TurnEvent {
        kind: kind.clone(),
        body: clip(&crate_redact(&input.body), 4_000),
        at_ms: now_ms(),
    });
    for f in input.files {
        if !f.is_empty() && !snap.files.iter().any(|x| x == &f) {
            snap.files.push(f);
        }
    }
    for s in input.symbols {
        if !s.is_empty() && !snap.symbols.iter().any(|x| x == &s) {
            snap.symbols.push(s);
        }
    }
    for d in input.decisions {
        if !d.is_empty() && !snap.decisions.iter().any(|x| x == &d) {
            snap.decisions.push(d);
        }
    }
    write_snapshot(root, &snap)?;
    let _ = record_activity(
        root,
        ActivityRecord {
            idempotency_key: format!(
                "turn.event:{}:{}:{}",
                turn_id,
                kind,
                snap.events.len()
            ),
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
    let mut snap = read_snapshot(root, session_id, turn_id)?;
    if snap.state != TurnState::Open {
        return Err(format!("turn is {:?}; cannot checkpoint", snap.state));
    }
    snap.events.push(TurnEvent {
        kind: "checkpoint".into(),
        body: clip(&crate_redact(note), 4_000),
        at_ms: now_ms(),
    });
    write_snapshot(root, &snap)?;
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
        });
    }
    if snap.state == TurnState::Failed {
        return Err("turn already failed".into());
    }

    let changed = git_changed_since(root, snap.head.as_deref());
    let commits = git_commits_since(root, snap.head.as_deref());
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
    let outcome = clip(&crate_redact(&input.outcome), 20_000);
    snap.outcome = Some(outcome.clone());

    let memory_id = save_cli_turn_memory(pool, &snap, &commits, &outcome).await?;
    snap.memory_id = memory_id.clone();

    let mut semantic_ids = Vec::new();
    for mem in &input.semantic_memories {
        if mem.body.trim().is_empty() {
            continue;
        }
        // Classification is deterministic from explicit kind; uncertain kinds become notes.
        let kind = normalize_kind(&mem.kind);
        match save_semantic_memory(pool, mem, kind, session_id, turn_id).await {
            Ok(id) => semantic_ids.push(id),
            Err(e) => {
                // Surface but do not abort the turn finish after the turn memory exists.
                tracing::warn!("semantic memory skipped: {e}");
            }
        }
    }

    let receipt = TurnFinishReceipt {
        turn_id: turn_id.to_string(),
        session_id: session_id.to_string(),
        memory_id: memory_id.clone(),
        files: snap.files.clone(),
        commits: commits.clone(),
        state: "finished".into(),
        idempotent: false,
        semantic_memory_ids: semantic_ids.clone(),
    };
    snap.state = TurnState::Finished;
    snap.finish_receipt = Some(serde_json::to_value(&receipt).unwrap_or(json!({})));
    write_snapshot(root, &snap)?;

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

    if !input.open_questions.is_empty() {
        let _ = crate::working_context_apply(
            root,
            session_id,
            &json!({
                "action": "add",
                "open_questions": input.open_questions,
                "decisions": input.decisions,
            }),
            "",
        )
        .await;
    } else if !input.decisions.is_empty() {
        let _ = crate::working_context_apply(
            root,
            session_id,
            &json!({
                "action": "add",
                "decisions": input.decisions,
            }),
            "",
        )
        .await;
    }

    Ok(receipt)
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
        body.push_str(&format!("\n\nFiles: {}", snap.files.iter().take(30).cloned().collect::<Vec<_>>().join(", ")));
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
    body.push_str(&blake3_hex(&snap.session_id)[..12]);
    if !outcome.is_empty() {
        body.push_str("\n\nOutcome: ");
        body.push_str(outcome);
    }

    let id = format!("turn-{}", &blake3_hex(&format!("{}:{}", snap.session_id, snap.turn_id))[..16]);
    let files_json = serde_json::to_string(&snap.files).unwrap_or_else(|_| "[]".into());
    let now = now_ms();
    let result = sqlx::query(
        r#"INSERT OR IGNORE INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, embedding, enabled)
           VALUES (?, 'turn', ?, ?, '[]', ?, 1.0, ?, ?, ?, NULL, 1)"#,
    )
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

    if result.rows_affected() > 0 {
        Ok(Some(id))
    } else {
        Ok(Some(id)) // already existed — idempotent
    }
}

async fn save_semantic_memory(
    pool: &SqlitePool,
    mem: &SemanticMemoryInput,
    kind: &str,
    session_id: &str,
    turn_id: &str,
) -> Result<String, String> {
    let title = if mem.title.trim().is_empty() {
        clip(&mem.body, 80)
    } else {
        clip(&mem.title, 120)
    };
    let body = crate_redact(&mem.body);
    let id = format!(
        "mem-{}",
        &blake3_hex(&format!("{session_id}:{turn_id}:{kind}:{title}:{body}"))[..16]
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
    if result.rows_affected() == 0 {
        // Duplicate content — report existing id without rewriting.
        return Ok(id);
    }
    Ok(id)
}

/// Mark a turn failed without writing a successful memory.
pub async fn turn_fail(
    root: &Path,
    session_id: &str,
    turn_id: &str,
    reason: &str,
) -> Result<TurnSnapshot, String> {
    let mut snap = read_snapshot(root, session_id, turn_id)?;
    if snap.state != TurnState::Open {
        return Err(format!("turn is {:?}; cannot fail", snap.state));
    }
    snap.state = TurnState::Failed;
    snap.outcome = Some(clip(&crate_redact(reason), 4_000));
    write_snapshot(root, &snap)?;
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
pub fn turn_status(
    root: &Path,
    session_id: &str,
    turn_id: Option<&str>,
) -> Result<Value, String> {
    if let Some(id) = turn_id {
        let snap = read_snapshot(root, session_id, id)?;
        return Ok(serde_json::to_value(snap).map_err(|e| e.to_string())?);
    }
    let dir = turns_dir(root).join(session_id);
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

    async fn fixture() -> (PathBuf, SqlitePool) {
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
        std::env::set_var(
            "AX_USAGE_DB",
            dir.join("usage.db").display().to_string(),
        );
        let _ = open_pool_at(&dir.join("usage.db")).await.unwrap();
        (dir, pool)
    }

    #[tokio::test]
    async fn research_turn_without_files_is_saved() {
        let (root, pool) = fixture().await;
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
    async fn incomplete_turn_is_visible() {
        let (root, _pool) = fixture().await;
        let begin = turn_begin(&root, "sess-b", "open question", None)
            .await
            .unwrap();
        let status = turn_status(&root, "sess-b", None).unwrap();
        let open = status["open"].as_array().unwrap();
        assert!(open.iter().any(|v| v.as_str() == Some(begin.turn_id.as_str())));
    }
}
