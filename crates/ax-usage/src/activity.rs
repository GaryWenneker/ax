//! Transport-agnostic AX activity events (CLI, MCP, hooks, agent-reported).
//!
//! Events live in `usage.db` and are dual-written to the verbose log so Command
//! Center Logging can show them without inventing MCP provenance.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;

use crate::context_cache::project_scope;
use crate::domain_log::log_domain_event;
use crate::store::open_pool;

pub const ACTIVITY_SCHEMA_VERSION: i32 = 1;

const ACTIVITY_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS ax_activity_event (
  event_id TEXT PRIMARY KEY,
  idempotency_key TEXT NOT NULL,
  schema_version INTEGER NOT NULL DEFAULT 1,
  timestamp_ms INTEGER NOT NULL,
  source TEXT NOT NULL,
  agent TEXT,
  project_id TEXT NOT NULL,
  session_id TEXT,
  turn_id TEXT,
  parent_event_id TEXT,
  operation TEXT NOT NULL,
  status TEXT NOT NULL,
  duration_ms INTEGER,
  summary TEXT NOT NULL,
  files_json TEXT NOT NULL DEFAULT '[]',
  symbols_json TEXT NOT NULL DEFAULT '[]',
  commit_ids_json TEXT NOT NULL DEFAULT '[]',
  token_input INTEGER,
  token_output INTEGER,
  token_provenance TEXT NOT NULL DEFAULT 'unknown',
  error TEXT,
  UNIQUE(project_id, idempotency_key)
);
CREATE INDEX IF NOT EXISTS idx_ax_activity_project_ts
  ON ax_activity_event(project_id, timestamp_ms DESC);
CREATE INDEX IF NOT EXISTS idx_ax_activity_session
  ON ax_activity_event(session_id, timestamp_ms DESC);
CREATE INDEX IF NOT EXISTS idx_ax_activity_source
  ON ax_activity_event(source, timestamp_ms DESC);
";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivitySource {
    Cli,
    Mcp,
    Hook,
    Agent,
}

impl ActivitySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Mcp => "mcp",
            Self::Hook => "hook",
            Self::Agent => "agent",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "cli" => Some(Self::Cli),
            "mcp" => Some(Self::Mcp),
            "hook" => Some(Self::Hook),
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityStatus {
    Success,
    Failed,
    Incomplete,
}

impl ActivityStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Incomplete => "incomplete",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetrics {
    pub input: Option<i64>,
    pub output: Option<i64>,
    /// `unknown` | `measured` | `estimated` — never invent numbers.
    pub provenance: String,
}

impl Default for TokenMetrics {
    fn default() -> Self {
        Self {
            input: None,
            output: None,
            provenance: "unknown".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub schema_version: i32,
    pub event_id: String,
    pub idempotency_key: String,
    pub timestamp_ms: i64,
    pub source: ActivitySource,
    pub agent: Option<String>,
    pub project_id: String,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub parent_event_id: Option<String>,
    pub operation: String,
    pub status: ActivityStatus,
    pub duration_ms: Option<i64>,
    pub summary: String,
    pub files: Vec<String>,
    pub symbols: Vec<String>,
    pub commit_ids: Vec<String>,
    pub token_metrics: TokenMetrics,
    pub error: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ActivityRow {
    event_id: String,
    idempotency_key: String,
    schema_version: i32,
    timestamp_ms: i64,
    source: String,
    agent: Option<String>,
    project_id: String,
    session_id: Option<String>,
    turn_id: Option<String>,
    parent_event_id: Option<String>,
    operation: String,
    status: String,
    duration_ms: Option<i64>,
    summary: String,
    files_json: String,
    symbols_json: String,
    commit_ids_json: String,
    token_input: Option<i64>,
    token_output: Option<i64>,
    token_provenance: String,
    error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ActivityRecord {

    pub idempotency_key: String,
    pub source: Option<ActivitySource>,
    pub agent: Option<String>,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub parent_event_id: Option<String>,
    pub operation: String,
    pub status: Option<ActivityStatus>,
    pub duration_ms: Option<i64>,
    pub summary: String,
    pub files: Vec<String>,
    pub symbols: Vec<String>,
    pub commit_ids: Vec<String>,
    pub token_metrics: TokenMetrics,
    pub error: Option<String>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn mint_event_id() -> String {
    format!("evt_{}", uuid_ish())
}

fn uuid_ish() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    now_ms().hash(&mut h);
    std::process::id().hash(&mut h);
    format!("{:016x}{:08x}", h.finish(), now_ms() as u32)
}

/// Ensure the activity table exists (idempotent).
pub async fn ensure_activity_schema(pool: &SqlitePool) -> Result<(), String> {
    for stmt in ACTIVITY_SCHEMA
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        sqlx::query(stmt)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn to_json_array(items: &[String]) -> String {
    serde_json::to_string(items).unwrap_or_else(|_| "[]".into())
}

fn from_json_array(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn looks_like_secret(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    if lower.contains("password=") || lower.contains("token=") || lower.contains("api_key=") {
        return true;
    }
    if token.starts_with("sk-") && token.len() > 20 {
        return true;
    }
    token.len() >= 32
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn redact_summary(summary: &str) -> String {
    let mut out = String::new();
    for token in summary.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        if let Some((k, _)) = token.split_once('=') {
            let key = k.to_ascii_lowercase();
            if key.contains("password") || key.contains("token") || key.contains("secret") {
                out.push_str(k);
                out.push_str("=[redacted]");
                continue;
            }
        }
        if looks_like_secret(token) {
            out.push_str("[redacted]");
        } else {
            out.push_str(token);
        }
    }
    out.chars().take(500).collect()
}

/// Record one activity event. Idempotent on `(project_id, idempotency_key)`.
pub async fn record_activity(
    root: &Path,
    mut record: ActivityRecord,
) -> Result<ActivityEvent, String> {
    if record.idempotency_key.trim().is_empty() {
        return Err("idempotency_key required".into());
    }
    if record.operation.trim().is_empty() {
        return Err("operation required".into());
    }
    let source = record.source.unwrap_or(ActivitySource::Cli);
    let status = record.status.unwrap_or(ActivityStatus::Success);
    let project_id = project_scope(root);
    let summary = redact_summary(&record.summary);
    if summary.is_empty() {
        record.summary = record.operation.clone();
    }
    let summary = redact_summary(&record.summary);
    let event = ActivityEvent {
        schema_version: ACTIVITY_SCHEMA_VERSION,
        event_id: mint_event_id(),
        idempotency_key: record.idempotency_key.clone(),
        timestamp_ms: now_ms(),
        source,
        agent: record.agent.clone(),
        project_id: project_id.clone(),
        session_id: record.session_id.clone(),
        turn_id: record.turn_id.clone(),
        parent_event_id: record.parent_event_id.clone(),
        operation: record.operation.clone(),
        status,
        duration_ms: record.duration_ms,
        summary: summary.clone(),
        files: record.files.clone(),
        symbols: record.symbols.clone(),
        commit_ids: record.commit_ids.clone(),
        token_metrics: record.token_metrics.clone(),
        error: record.error.clone().map(|e| redact_summary(&e)),
    };

    let pool = open_pool().await.map_err(|e| e.to_string())?;
    ensure_activity_schema(&pool).await?;

    let result = sqlx::query(
        "INSERT OR IGNORE INTO ax_activity_event (
            event_id, idempotency_key, schema_version, timestamp_ms, source, agent,
            project_id, session_id, turn_id, parent_event_id, operation, status,
            duration_ms, summary, files_json, symbols_json, commit_ids_json,
            token_input, token_output, token_provenance, error
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&event.event_id)
    .bind(&event.idempotency_key)
    .bind(event.schema_version)
    .bind(event.timestamp_ms)
    .bind(event.source.as_str())
    .bind(&event.agent)
    .bind(&event.project_id)
    .bind(&event.session_id)
    .bind(&event.turn_id)
    .bind(&event.parent_event_id)
    .bind(&event.operation)
    .bind(event.status.as_str())
    .bind(event.duration_ms)
    .bind(&event.summary)
    .bind(to_json_array(&event.files))
    .bind(to_json_array(&event.symbols))
    .bind(to_json_array(&event.commit_ids))
    .bind(event.token_metrics.input)
    .bind(event.token_metrics.output)
    .bind(&event.token_metrics.provenance)
    .bind(&event.error)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let inserted = result.rows_affected() > 0;
    let stored = load_by_idempotency(&pool, &project_id, &event.idempotency_key)
        .await?
        .unwrap_or(event);
    if inserted {
        dual_write_verbose(root, &stored);
    }
    Ok(stored)
}

async fn load_by_idempotency(
    pool: &SqlitePool,
    project_id: &str,
    key: &str,
) -> Result<Option<ActivityEvent>, String> {
    let row: Option<ActivityRow> = sqlx::query_as(
        "SELECT event_id, idempotency_key, schema_version, timestamp_ms, source, agent,
                project_id, session_id, turn_id, parent_event_id, operation, status,
                duration_ms, summary, files_json, symbols_json, commit_ids_json,
                token_input, token_output, token_provenance, error
         FROM ax_activity_event
         WHERE project_id = ? AND idempotency_key = ?",
    )
    .bind(project_id)
    .bind(key)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(row.map(row_to_event))
}

fn row_to_event(r: ActivityRow) -> ActivityEvent {
    ActivityEvent {
        event_id: r.event_id,
        idempotency_key: r.idempotency_key,
        schema_version: r.schema_version,
        timestamp_ms: r.timestamp_ms,
        source: ActivitySource::parse(&r.source).unwrap_or(ActivitySource::Cli),
        agent: r.agent,
        project_id: r.project_id,
        session_id: r.session_id,
        turn_id: r.turn_id,
        parent_event_id: r.parent_event_id,
        operation: r.operation,
        status: match r.status.as_str() {
            "failed" => ActivityStatus::Failed,
            "incomplete" => ActivityStatus::Incomplete,
            _ => ActivityStatus::Success,
        },
        duration_ms: r.duration_ms,
        summary: r.summary,
        files: from_json_array(&r.files_json),
        symbols: from_json_array(&r.symbols_json),
        commit_ids: from_json_array(&r.commit_ids_json),
        token_metrics: TokenMetrics {
            input: r.token_input,
            output: r.token_output,
            provenance: r.token_provenance,
        },
        error: r.error,
    }
}

fn dual_write_verbose(root: &Path, event: &ActivityEvent) {
    let domain = match event.source {
        ActivitySource::Cli => "cli",
        ActivitySource::Mcp => "mcp-activity",
        ActivitySource::Hook => "hook",
        ActivitySource::Agent => "agent",
    };
    let mut msg = format!(
        "activity op={} status={} source={} id={} key={}",
        event.operation,
        event.status.as_str(),
        event.source.as_str(),
        event.event_id,
        event.idempotency_key
    );
    if let Some(session) = &event.session_id {
        msg.push_str(&format!(" session={session}"));
    }
    if let Some(turn) = &event.turn_id {
        msg.push_str(&format!(" turn={turn}"));
    }
    if let Some(ms) = event.duration_ms {
        msg.push_str(&format!(" duration_ms={ms}"));
    }
    msg.push_str(&format!(" summary={}", event.summary.replace('\n', " ")));
    log_domain_event(Some(root), domain, msg);
}

/// List recent activity for a project (newest first).
pub async fn list_activity(
    root: &Path,
    source: Option<ActivitySource>,
    session_id: Option<&str>,
    limit: i64,
) -> Result<Vec<ActivityEvent>, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    ensure_activity_schema(&pool).await?;
    let project_id = project_scope(root);
    let limit = limit.clamp(1, 500);

    let rows: Vec<ActivityRow> = if let (Some(src), Some(session)) = (source, session_id) {
        sqlx::query_as(
            "SELECT event_id, idempotency_key, schema_version, timestamp_ms, source, agent,
                    project_id, session_id, turn_id, parent_event_id, operation, status,
                    duration_ms, summary, files_json, symbols_json, commit_ids_json,
                    token_input, token_output, token_provenance, error
             FROM ax_activity_event
             WHERE project_id = ? AND source = ? AND session_id = ?
             ORDER BY timestamp_ms DESC LIMIT ?",
        )
        .bind(&project_id)
        .bind(src.as_str())
        .bind(session)
        .bind(limit)
        .fetch_all(&pool)
        .await
        .map_err(|e| e.to_string())?
    } else if let Some(src) = source {
        sqlx::query_as(
            "SELECT event_id, idempotency_key, schema_version, timestamp_ms, source, agent,
                    project_id, session_id, turn_id, parent_event_id, operation, status,
                    duration_ms, summary, files_json, symbols_json, commit_ids_json,
                    token_input, token_output, token_provenance, error
             FROM ax_activity_event
             WHERE project_id = ? AND source = ?
             ORDER BY timestamp_ms DESC LIMIT ?",
        )
        .bind(&project_id)
        .bind(src.as_str())
        .bind(limit)
        .fetch_all(&pool)
        .await
        .map_err(|e| e.to_string())?
    } else if let Some(session) = session_id {
        sqlx::query_as(
            "SELECT event_id, idempotency_key, schema_version, timestamp_ms, source, agent,
                    project_id, session_id, turn_id, parent_event_id, operation, status,
                    duration_ms, summary, files_json, symbols_json, commit_ids_json,
                    token_input, token_output, token_provenance, error
             FROM ax_activity_event
             WHERE project_id = ? AND session_id = ?
             ORDER BY timestamp_ms DESC LIMIT ?",
        )
        .bind(&project_id)
        .bind(session)
        .bind(limit)
        .fetch_all(&pool)
        .await
        .map_err(|e| e.to_string())?
    } else {
        sqlx::query_as(
            "SELECT event_id, idempotency_key, schema_version, timestamp_ms, source, agent,
                    project_id, session_id, turn_id, parent_event_id, operation, status,
                    duration_ms, summary, files_json, symbols_json, commit_ids_json,
                    token_input, token_output, token_provenance, error
             FROM ax_activity_event
             WHERE project_id = ?
             ORDER BY timestamp_ms DESC LIMIT ?",
        )
        .bind(&project_id)
        .bind(limit)
        .fetch_all(&pool)
        .await
        .map_err(|e| e.to_string())?
    };

    Ok(rows.into_iter().map(row_to_event).collect())
}

/// Convenience: record a CLI command lifecycle event.
pub async fn record_cli_command(
    root: &Path,
    command: &str,
    session_id: Option<&str>,
    ok: bool,
    duration_ms: Option<i64>,
    summary: &str,
) -> Result<ActivityEvent, String> {
    let key = format!(
        "cli:{}:{}:{}",
        command,
        session_id.unwrap_or("-"),
        now_ms()
    );
    record_activity(
        root,
        ActivityRecord {
            idempotency_key: key,
            source: Some(ActivitySource::Cli),
            operation: format!("command.{command}"),
            status: Some(if ok {
                ActivityStatus::Success
            } else {
                ActivityStatus::Failed
            }),
            duration_ms,
            summary: summary.to_string(),
            session_id: session_id.map(str::to_owned),
            ..ActivityRecord::default()
        },
    )
    .await
}

/// JSON view used by CLI `--json` and web APIs.
pub fn event_to_json(event: &ActivityEvent) -> Value {
    serde_json::json!({
        "schemaVersion": event.schema_version,
        "eventId": event.event_id,
        "idempotencyKey": event.idempotency_key,
        "timestamp": chrono_like(event.timestamp_ms),
        "timestampMs": event.timestamp_ms,
        "source": event.source.as_str(),
        "agent": event.agent,
        "projectId": event.project_id,
        "sessionId": event.session_id,
        "turnId": event.turn_id,
        "parentEventId": event.parent_event_id,
        "operation": event.operation,
        "status": event.status.as_str(),
        "durationMs": event.duration_ms,
        "summary": event.summary,
        "files": event.files,
        "symbols": event.symbols,
        "commitIds": event.commit_ids,
        "tokenMetrics": {
            "input": event.token_metrics.input,
            "output": event.token_metrics.output,
            "provenance": event.token_metrics.provenance,
        },
        "error": event.error,
    })
}

fn chrono_like(ms: i64) -> String {
    use chrono::{TimeZone, Utc};
    Utc.timestamp_millis_opt(ms)
        .single()
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::open_pool_at;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SEQ: AtomicUsize = AtomicUsize::new(0);
    async fn tmp_root() -> (std::path::PathBuf, tokio::sync::MutexGuard<'static, ()>) {
        let guard = crate::store::USAGE_ENV_LOCK.lock().await;
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("ax-activity-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("usage.db");
        std::env::set_var("AX_USAGE_DB", db.display().to_string());
        let _ = open_pool_at(&db).await.unwrap();
        (dir, guard)
    }

    #[tokio::test]
    async fn records_and_is_idempotent() {
        let (root, _guard) = tmp_root().await;
        let a = record_activity(
            &root,
            ActivityRecord {
                idempotency_key: "k1".into(),
                operation: "graph.explore".into(),
                summary: "explored auth".into(),
                source: Some(ActivitySource::Cli),
                session_id: Some("sess-1".into()),
                ..ActivityRecord::default()
            },
        )
        .await
        .unwrap();
        let b = record_activity(
            &root,
            ActivityRecord {
                idempotency_key: "k1".into(),
                operation: "graph.explore".into(),
                summary: "explored auth again".into(),
                source: Some(ActivitySource::Cli),
                session_id: Some("sess-1".into()),
                ..ActivityRecord::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(a.event_id, b.event_id);
        assert_eq!(a.summary, "explored auth");
        let listed = list_activity(&root, Some(ActivitySource::Cli), Some("sess-1"), 10)
            .await
            .unwrap();
        assert_eq!(listed.len(), 1);
    }

    #[tokio::test]
    async fn redacts_secrets_in_summary() {
        let (root, _guard) = tmp_root().await;
        let event = record_activity(
            &root,
            ActivityRecord {
                idempotency_key: "secret-1".into(),
                operation: "turn.event".into(),
                summary: "token=sk-abcdefghijklmnopqrstuvwxyz012345".into(),
                source: Some(ActivitySource::Agent),
                ..ActivityRecord::default()
            },
        )
        .await
        .unwrap();
        assert!(
            event.summary.contains("[redacted]") || !event.summary.contains("sk-abcdef"),
            "summary={}",
            event.summary
        );
    }
}
