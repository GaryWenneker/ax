//! Durable conversation records, adapted from Pi Durable's session model.
//!
//! Ax does not call the model, so a compaction summary and a task step are
//! written by the caller. The originals stay searchable, a fork reads the
//! parent up to the fork point, and a task resume returns the last checkpoint
//! after the process restarts.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_BODY: usize = 8_000;
const MAX_FORK_DEPTH: usize = 16;
const MAX_SEARCH: i64 = 50;
const KINDS: &[&str] = &["user", "assistant", "tool", "system", "compaction", "reset", "hook"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurableEntry {
    pub id: i64,
    pub conversation_id: String,
    pub kind: String,
    pub body: String,
    pub head: Option<i64>,
}

pub async fn durable_apply(conversation: &str, request: &Value) -> Result<String, String> {
    let pool = crate::store::open_pool().await.map_err(|e| e.to_string())?;
    apply_with(&pool, conversation, request).await
}

/// Record a tool call when this chat already has a durable conversation.
pub async fn note_tool_if_open(conversation: &str, tool: &str, summary: &str) -> Result<(), String> {
    if conversation.is_empty() || tool.is_empty() {
        return Ok(());
    }
    let pool = crate::store::open_pool().await.map_err(|e| e.to_string())?;
    if !conversation_exists(&pool, conversation).await? {
        return Ok(());
    }
    let body = format!("{tool} {summary}");
    append_entry(&pool, conversation, "tool", &clip(&body), None).await?;
    fire_hooks(&pool, conversation, "tool").await?;
    Ok(())
}

pub(crate) async fn apply_with(pool: &SqlitePool, conversation: &str, request: &Value) -> Result<String, String> {
    let action = request.get("action").and_then(Value::as_str).unwrap_or("read");
    match action {
        "append" => {
            let kind = request.get("kind").and_then(Value::as_str).unwrap_or("user");
            let body = request.get("body").and_then(Value::as_str).unwrap_or("");
            if body.trim().is_empty() {
                return Err("append needs a body".into());
            }
            ensure_conversation(pool, conversation, None, None).await?;
            let id = append_entry(pool, conversation, kind, &clip(body), None).await?;
            fire_hooks(pool, conversation, "append").await?;
            Ok(format!("<ax_durable_entry id={id} kind={kind}>"))
        }
        "read" => Ok(render_entries("ax_durable", conversation, &working_entries(pool, conversation).await?)),
        "search" => {
            let query = request.get("query").and_then(Value::as_str).unwrap_or("").trim().to_string();
            if query.is_empty() {
                return Err("search needs a query".into());
            }
            let hits = search_entries(pool, conversation, &query).await?;
            Ok(render_entries("ax_durable_search", conversation, &hits))
        }
        "compact" => {
            let summary = request.get("summary").and_then(Value::as_str).unwrap_or("").trim().to_string();
            if summary.is_empty() {
                return Err("compact needs a summary".into());
            }
            ensure_conversation(pool, conversation, None, None).await?;
            let first_kept = request.get("first_kept").and_then(Value::as_i64);
            let id = append_entry(pool, conversation, "compaction", &clip(&summary), first_kept).await?;
            let head = first_kept.unwrap_or(id);
            sqlx::query("UPDATE ax_durable_entry SET head = ? WHERE id = ?")
                .bind(head)
                .bind(id)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?;
            fire_hooks(pool, conversation, "compact").await?;
            Ok(format!("<ax_durable_compact id={id} head={head}>"))
        }
        "fork" => {
            if !conversation_exists(pool, conversation).await? {
                return Err("nothing to fork".into());
            }
            let at = match request.get("at").and_then(Value::as_i64) {
                Some(at) => at,
                None => latest_entry(pool, conversation).await?.ok_or("nothing to fork")?,
            };
            if !entry_visible(pool, conversation, at).await? {
                return Err(format!("entry {at} is not in this conversation"));
            }
            let child = mint_id("dcv", conversation);
            ensure_conversation(pool, &child, Some(conversation), Some(at)).await?;
            copy_documents(pool, conversation, &child).await?;
            fire_hooks(pool, conversation, "fork").await?;
            Ok(format!("<ax_durable_fork parent={conversation} child={child} at={at}>"))
        }
        "handoff" => {
            let note = request.get("note").and_then(Value::as_str).unwrap_or("").trim().to_string();
            if note.is_empty() {
                return Err("handoff needs a note".into());
            }
            let child = mint_id("dcv", conversation);
            ensure_conversation(pool, &child, None, None).await?;
            append_entry(pool, &child, "reset", &clip(&note), None).await?;
            fire_hooks(pool, conversation, "handoff").await?;
            Ok(format!("<ax_durable_handoff parent={conversation} child={child}>"))
        }
        "doc_put" => {
            let kind = request.get("kind").and_then(Value::as_str).unwrap_or("");
            let body = request.get("body").cloned().unwrap_or(Value::Null);
            if kind.is_empty() || !kind.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                return Err("doc_put needs a kind".into());
            }
            let text = serde_json::to_string(&body).map_err(|e| e.to_string())?;
            if text.len() > MAX_BODY {
                return Err(format!("document exceeds {MAX_BODY} bytes"));
            }
            ensure_conversation(pool, conversation, None, None).await?;
            let now = chrono::Utc::now().timestamp();
            sqlx::query(
                "INSERT INTO ax_durable_document (conversation_id, kind, body, updated_at)
                 VALUES (?, ?, ?, ?)
                 ON CONFLICT(conversation_id, kind) DO UPDATE SET body = excluded.body, updated_at = excluded.updated_at",
            )
            .bind(conversation)
            .bind(kind)
            .bind(&text)
            .bind(now)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
            Ok(format!("<ax_durable_document kind={kind}>"))
        }
        "doc_get" => {
            let kind = request.get("kind").and_then(Value::as_str).unwrap_or("");
            let body = document(pool, conversation, kind).await?;
            Ok(body.unwrap_or_else(|| format!("<ax_durable_document kind={kind} missing>")))
        }
        "task_start" => {
            let kind = request.get("kind").and_then(Value::as_str).unwrap_or("");
            if kind.is_empty() {
                return Err("task_start needs a kind".into());
            }
            let input = request.get("input").cloned().unwrap_or_else(|| json!({}));
            ensure_conversation(pool, conversation, None, None).await?;
            let id = mint_id("tsk", conversation);
            let now = chrono::Utc::now().timestamp();
            sqlx::query(
                "INSERT INTO ax_durable_task (id, conversation_id, kind, status, phase, checkpoint, result, updated_at)
                 VALUES (?, ?, ?, 'running', 'start', ?, NULL, ?)",
            )
            .bind(&id)
            .bind(conversation)
            .bind(kind)
            .bind(serde_json::to_string(&input).map_err(|e| e.to_string())?)
            .bind(now)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
            Ok(format!("<ax_durable_task id={id} status=running phase=start>"))
        }
        "task_checkpoint" => {
            let id = task_id(request)?;
            let phase = request.get("phase").and_then(Value::as_str).unwrap_or("");
            let checkpoint = request.get("checkpoint").cloned().unwrap_or(Value::Null);
            if phase.is_empty() || !checkpoint.is_object() {
                return Err("task_checkpoint needs a phase and a checkpoint object".into());
            }
            update_task(pool, &id, "running", phase, &checkpoint, None).await?;
            Ok(format!("<ax_durable_task id={id} status=running phase={phase}>"))
        }
        "task_resume" => {
            let id = task_id(request)?;
            let row = task_row(pool, &id).await?;
            if row.status == "terminal" {
                return Err("task is finished".into());
            }
            Ok(format!(
                "<ax_durable_task id={id} status={} phase={}>\n{}",
                row.status, row.phase, row.checkpoint
            ))
        }
        "task_finish" => {
            let id = task_id(request)?;
            let result = request.get("result").cloned().unwrap_or(Value::Null);
            update_task(pool, &id, "terminal", "done", &json!({}), Some(&result)).await?;
            Ok(format!("<ax_durable_task id={id} status=terminal phase=done>"))
        }
        "hook" => {
            let event = request.get("event").and_then(Value::as_str).unwrap_or("");
            let name = request.get("name").and_then(Value::as_str).unwrap_or("");
            if event.is_empty() || name.is_empty() {
                return Err("hook needs an event and a name".into());
            }
            ensure_conversation(pool, conversation, None, None).await?;
            sqlx::query(
                "INSERT OR IGNORE INTO ax_durable_hook (conversation_id, event, name) VALUES (?, ?, ?)",
            )
            .bind(conversation)
            .bind(event)
            .bind(name)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
            Ok(format!("<ax_durable_hook event={event} name={name}>"))
        }
        other => Err(format!("unknown ax_durable action {other:?}")),
    }
}

struct TaskRow {
    status: String,
    phase: String,
    checkpoint: String,
}

fn task_id(request: &Value) -> Result<String, String> {
    request
        .get("task")
        .and_then(Value::as_str)
        .filter(|id| id.starts_with("tsk_"))
        .map(str::to_string)
        .ok_or_else(|| "task id is missing".into())
}

async fn update_task(
    pool: &SqlitePool,
    id: &str,
    status: &str,
    phase: &str,
    checkpoint: &Value,
    result: Option<&Value>,
) -> Result<(), String> {
    let current = task_row(pool, id).await?;
    if current.status == "terminal" {
        return Err("task is finished".into());
    }
    let now = chrono::Utc::now().timestamp();
    let result_text = result.map(serde_json::to_string).transpose().map_err(|e| e.to_string())?;
    sqlx::query(
        "UPDATE ax_durable_task
         SET status = ?, phase = ?, checkpoint = ?, result = COALESCE(?, result), updated_at = ?
         WHERE id = ?",
    )
    .bind(status)
    .bind(phase)
    .bind(serde_json::to_string(checkpoint).map_err(|e| e.to_string())?)
    .bind(result_text)
    .bind(now)
    .bind(id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

async fn task_row(pool: &SqlitePool, id: &str) -> Result<TaskRow, String> {
    let row: Option<(String, String, String)> =
        sqlx::query_as("SELECT status, phase, checkpoint FROM ax_durable_task WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    let Some((status, phase, checkpoint)) = row else {
        return Err("task not found".into());
    };
    Ok(TaskRow { status, phase, checkpoint })
}

fn clip(body: &str) -> String {
    let mut out = String::new();
    for ch in body.chars() {
        if out.len() + ch.len_utf8() > MAX_BODY {
            break;
        }
        out.push(ch);
    }
    out
}

fn mint_id(prefix: &str, seed: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(1);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let stamp = chrono::Utc::now().timestamp_micros();
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    hasher.update(n.to_le_bytes());
    hasher.update(stamp.to_le_bytes());
    let digest = hasher.finalize();
    let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    format!("{prefix}_{hex}")
}

async fn conversation_exists(pool: &SqlitePool, id: &str) -> Result<bool, String> {
    let n: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM ax_durable_conversation WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(n.is_some())
}

async fn ensure_conversation(
    pool: &SqlitePool,
    id: &str,
    parent: Option<&str>,
    fork_entry: Option<i64>,
) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        "INSERT OR IGNORE INTO ax_durable_conversation (id, parent_id, fork_entry, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(id)
    .bind(parent)
    .bind(fork_entry)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

async fn append_entry(
    pool: &SqlitePool,
    conversation: &str,
    kind: &str,
    body: &str,
    head: Option<i64>,
) -> Result<i64, String> {
    if !KINDS.contains(&kind) {
        return Err(format!("unknown entry kind {kind:?}"));
    }
    let now = chrono::Utc::now().timestamp();
    let result = sqlx::query(
        "INSERT INTO ax_durable_entry (conversation_id, kind, body, head, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(conversation)
    .bind(kind)
    .bind(body)
    .bind(head)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(result.last_insert_rowid())
}

async fn latest_entry(pool: &SqlitePool, conversation: &str) -> Result<Option<i64>, String> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM ax_durable_entry WHERE conversation_id = ? ORDER BY id DESC LIMIT 1")
            .bind(conversation)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    Ok(row.map(|r| r.0))
}

async fn entry_visible(pool: &SqlitePool, conversation: &str, id: i64) -> Result<bool, String> {
    for (scope, max_id) in ancestry(pool, conversation).await? {
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM ax_durable_entry WHERE conversation_id = ? AND id = ? AND (? IS NULL OR id <= ?)",
        )
        .bind(&scope)
        .bind(id)
        .bind(max_id)
        .bind(max_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;
        if row.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn load_entries(pool: &SqlitePool, conversation: &str, max_id: Option<i64>) -> Result<Vec<DurableEntry>, String> {
    let rows: Vec<(i64, String, String, String, Option<i64>)> = sqlx::query_as(
        "SELECT id, conversation_id, kind, body, head FROM ax_durable_entry
         WHERE conversation_id = ? AND (? IS NULL OR id <= ?)
         ORDER BY id",
    )
    .bind(conversation)
    .bind(max_id)
    .bind(max_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(id, conversation_id, kind, body, head)| DurableEntry { id, conversation_id, kind, body, head })
        .collect())
}

fn hide_compacted(entries: Vec<DurableEntry>) -> Vec<DurableEntry> {
    let Some(compaction) = entries.iter().rev().find(|e| e.kind == "compaction") else {
        return entries;
    };
    let head = compaction.head.unwrap_or(compaction.id);
    let compaction_id = compaction.id;
    entries.into_iter().filter(|e| e.id >= head || e.id == compaction_id).collect()
}

async fn parent_of(pool: &SqlitePool, id: &str) -> Result<Option<(String, i64)>, String> {
    let row: Option<(Option<String>, Option<i64>)> =
        sqlx::query_as("SELECT parent_id, fork_entry FROM ax_durable_conversation WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    Ok(row.and_then(|(parent, at)| parent.zip(at)))
}

async fn ancestry(pool: &SqlitePool, conversation: &str) -> Result<Vec<(String, Option<i64>)>, String> {
    let mut chain = Vec::new();
    let mut current = conversation.to_string();
    for _ in 0..MAX_FORK_DEPTH {
        if let Some((parent, at)) = parent_of(pool, &current).await? {
            chain.push((parent.clone(), Some(at)));
            current = parent;
        } else {
            break;
        }
    }
    chain.reverse();
    chain.push((conversation.to_string(), None));
    Ok(chain)
}

async fn working_entries(pool: &SqlitePool, conversation: &str) -> Result<Vec<DurableEntry>, String> {
    let mut out = Vec::new();
    for (id, max_id) in ancestry(pool, conversation).await? {
        let slice = hide_compacted(load_entries(pool, &id, max_id).await?);
        out.extend(slice);
    }
    Ok(out)
}

async fn search_entries(pool: &SqlitePool, conversation: &str, query: &str) -> Result<Vec<DurableEntry>, String> {
    let needle = query.to_lowercase();
    let mut hits = Vec::new();
    for (id, max_id) in ancestry(pool, conversation).await? {
        for entry in load_entries(pool, &id, max_id).await? {
            if needle.is_empty() || entry.body.to_lowercase().contains(&needle) {
                hits.push(entry);
            }
            if hits.len() as i64 >= MAX_SEARCH {
                return Ok(hits);
            }
        }
    }
    Ok(hits)
}

async fn document(pool: &SqlitePool, conversation: &str, kind: &str) -> Result<Option<String>, String> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT body FROM ax_durable_document WHERE conversation_id = ? AND kind = ?")
            .bind(conversation)
            .bind(kind)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
    Ok(row.map(|r| r.0))
}

async fn copy_documents(pool: &SqlitePool, parent: &str, child: &str) -> Result<(), String> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT kind, body FROM ax_durable_document WHERE conversation_id = ?")
            .bind(parent)
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().timestamp();
    for (kind, body) in rows {
        sqlx::query(
            "INSERT OR IGNORE INTO ax_durable_document (conversation_id, kind, body, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(child)
        .bind(kind)
        .bind(body)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn fire_hooks(pool: &SqlitePool, conversation: &str, event: &str) -> Result<(), String> {
    let names: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM ax_durable_hook WHERE conversation_id = ? AND event = ? ORDER BY name")
            .bind(conversation)
            .bind(event)
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;
    for (name,) in names {
        append_entry(pool, conversation, "hook", &format!("{event} {name}"), None).await?;
    }
    Ok(())
}

fn render_entries(tag: &str, conversation: &str, entries: &[DurableEntry]) -> String {
    let mut out = format!("<{tag} conversation={conversation}>");
    for entry in entries {
        let head = entry.head.map(|h| format!(" head={h}")).unwrap_or_default();
        out.push_str(&format!("\n#{} {}{head}\n{}", entry.id, entry.kind, entry.body));
    }
    out.push_str(&format!("\n</{tag}>"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::open_pool_at;
    use serde_json::json;

    async fn pool() -> SqlitePool {
        let dir = std::env::temp_dir().join(format!("ax-durable-{}-{}", std::process::id(), chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)));
        std::fs::create_dir_all(&dir).unwrap();
        open_pool_at(&dir.join("usage.db")).await.unwrap()
    }

    #[tokio::test]
    async fn compact_hides_the_working_view_and_search_still_finds_the_original() {
        let pool = pool().await;
        apply_with(&pool, "chat", &json!({"action":"append","kind":"user","body":"the secret token is pine"})).await.unwrap();
        let err = apply_with(&pool, "chat", &json!({"action":"compact","summary":""})).await.unwrap_err();
        assert!(err.contains("compact needs a summary"), "{err}");
        apply_with(&pool, "chat", &json!({"action":"compact","summary":"User mentioned a token."})).await.unwrap();
        let view = apply_with(&pool, "chat", &json!({"action":"read"})).await.unwrap();
        assert!(view.contains("User mentioned a token."), "{view}");
        assert!(!view.contains("pine"), "{view}");
        let found = apply_with(&pool, "chat", &json!({"action":"search","query":"pine"})).await.unwrap();
        assert!(found.contains("pine"), "{found}");
    }

    #[tokio::test]
    async fn fork_reads_the_parent_up_to_the_point_and_copies_documents() {
        let pool = pool().await;
        apply_with(&pool, "parent", &json!({"action":"append","kind":"user","body":"before"})).await.unwrap();
        let marked = apply_with(&pool, "parent", &json!({"action":"append","kind":"user","body":"marker"})).await.unwrap();
        let at: i64 = marked.split("id=").nth(1).unwrap().chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap();
        apply_with(&pool, "parent", &json!({"action":"append","kind":"user","body":"after the fork"})).await.unwrap();
        apply_with(&pool, "parent", &json!({"action":"doc_put","kind":"plan","body":{"step":"auth"}})).await.unwrap();
        let forked = apply_with(&pool, "parent", &json!({"action":"fork","at":at})).await.unwrap();
        let child = forked.split("child=").nth(1).unwrap().split(" at=").next().unwrap();
        let view = apply_with(&pool, child, &json!({"action":"read"})).await.unwrap();
        assert!(view.contains("before") && view.contains("marker"), "{view}");
        assert!(!view.contains("after the fork"), "{view}");
        let doc = apply_with(&pool, child, &json!({"action":"doc_get","kind":"plan"})).await.unwrap();
        assert!(doc.contains("auth"), "{doc}");
        let parent = apply_with(&pool, "parent", &json!({"action":"read"})).await.unwrap();
        assert!(parent.contains("after the fork"), "{parent}");
    }

    #[tokio::test]
    async fn handoff_starts_clean_and_a_task_resumes_its_checkpoint_after_reopen() {
        let dir = std::env::temp_dir().join(format!("ax-durable-reopen-{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(1)));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("usage.db");
        let pool = open_pool_at(&path).await.unwrap();
        apply_with(&pool, "parent", &json!({"action":"append","kind":"user","body":"old thread"})).await.unwrap();
        let handed = apply_with(&pool, "parent", &json!({"action":"handoff","note":"Continue auth"})).await.unwrap();
        let child = handed.split("child=").nth(1).unwrap().trim_end_matches('>');
        let view = apply_with(&pool, child, &json!({"action":"read"})).await.unwrap();
        assert!(view.contains("Continue auth") && !view.contains("old thread"), "{view}");
        let parent = apply_with(&pool, "parent", &json!({"action":"read"})).await.unwrap();
        assert!(parent.contains("old thread"), "{parent}");

        let started = apply_with(&pool, child, &json!({"action":"task_start","kind":"summarize","input":{"reason":"overflow"}})).await.unwrap();
        let task = started.split("id=").nth(1).unwrap().split(" status=").next().unwrap();
        apply_with(&pool, child, &json!({"action":"task_checkpoint","task":task,"phase":"summarize","checkpoint":{"tail":12}})).await.unwrap();
        drop(pool);
        let reopened = open_pool_at(&path).await.unwrap();
        let resumed = apply_with(&reopened, child, &json!({"action":"task_resume","task":task})).await.unwrap();
        assert!(resumed.contains("phase=summarize") && resumed.contains("\"tail\":12"), "{resumed}");
        apply_with(&reopened, child, &json!({"action":"task_finish","task":task,"result":{"ok":true}})).await.unwrap();
        let err = apply_with(&reopened, child, &json!({"action":"task_resume","task":task})).await.unwrap_err();
        assert!(err.contains("task is finished"), "{err}");
    }

    #[tokio::test]
    async fn a_hook_records_an_entry_when_its_event_runs() {
        let pool = pool().await;
        apply_with(&pool, "chat", &json!({"action":"hook","event":"compact","name":"audit"})).await.unwrap();
        apply_with(&pool, "chat", &json!({"action":"append","kind":"user","body":"hello"})).await.unwrap();
        apply_with(&pool, "chat", &json!({"action":"compact","summary":"Said hello."})).await.unwrap();
        let view = apply_with(&pool, "chat", &json!({"action":"read"})).await.unwrap();
        assert!(view.contains("hook") && view.contains("compact audit"), "{view}");
    }
}
