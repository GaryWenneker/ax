//! Derived metrics in the project SQLite database.
//!
//! Rows store hashes, names, counts, and estimates. They do not store prompts,
//! completions, credentials, tool arguments, or tool output.

use std::path::Path;

use serde_json::json;
use sqlx::{sqlite::SqliteConnectOptions, Row, SqlitePool};

use crate::types::{Confidence, ToolAlternative, ToolEconomicsRecord};

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS agent_sessions (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    repository_root TEXT,
    active_branch TEXT,
    cache_namespace TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS agent_turns (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    started_at INTEGER,
    ended_at INTEGER,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    estimated_cost REAL
);
CREATE TABLE IF NOT EXISTS agent_tool_calls (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    input_hash TEXT NOT NULL,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    cache_hit INTEGER NOT NULL DEFAULT 0,
    repeated INTEGER NOT NULL DEFAULT 0,
    estimated_cost REAL
);
CREATE TABLE IF NOT EXISTS agent_model_calls (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    turn_id TEXT,
    provider TEXT,
    model TEXT,
    input_tokens INTEGER,
    output_tokens INTEGER,
    cached_input_tokens INTEGER,
    estimated_cost REAL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS agent_optimizations (
    id TEXT PRIMARY KEY,
    tool_call_id TEXT NOT NULL,
    alternative_tool TEXT NOT NULL,
    current_tokens INTEGER NOT NULL,
    alternative_tokens INTEGER NOT NULL,
    reduction_percent REAL NOT NULL,
    confidence TEXT NOT NULL,
    reason TEXT NOT NULL,
    accepted INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS agent_context_cache (
    key TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    estimated_tokens INTEGER NOT NULL,
    source_hashes TEXT NOT NULL,
    git_revision TEXT,
    policy_version TEXT,
    skill_version TEXT,
    graph_version TEXT
);
";

#[derive(Debug, Clone)]
pub struct PersistedSnapshot {
    pub sessions: Vec<SessionRow>,
    pub turns: Vec<TurnRow>,
    pub tools: Vec<ToolEconomicsRecord>,
    pub models: Vec<ModelRow>,
    pub optimizations: Vec<OptimizationRow>,
    pub cache_entries: Vec<CacheRow>,
}

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: String,
    pub project_id: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub repository_root: Option<String>,
    pub active_branch: Option<String>,
    pub cache_namespace: String,
}

#[derive(Debug, Clone)]
pub struct TurnRow {
    pub id: String,
    pub session_id: String,
    pub sequence: u32,
    pub started_at: Option<u64>,
    pub ended_at: Option<u64>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub estimated_cost: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct ModelRow {
    pub id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub estimated_cost: Option<f64>,
    pub created_at: u64,
}

#[derive(Debug, Clone)]
pub struct OptimizationRow {
    pub id: String,
    pub tool_call_id: String,
    pub alternative: ToolAlternative,
    pub accepted: bool,
}

#[derive(Debug, Clone)]
pub struct CacheRow {
    pub key: String,
    pub project_id: String,
    pub created_at: u64,
    pub estimated_tokens: u32,
    pub source_hashes: Vec<String>,
    pub git_revision: Option<String>,
    pub policy_version: String,
    pub skill_version: String,
    pub graph_version: String,
}

pub async fn flush(path: &Path, snapshot: &PersistedSnapshot) -> Result<(), String> {
    let pool = open(path).await?;
    ensure_schema(&pool).await?;
    for session in &snapshot.sessions {
        sqlx::query(
            "INSERT INTO agent_sessions (id, project_id, created_at, updated_at, repository_root, active_branch, cache_namespace)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET updated_at = excluded.updated_at, repository_root = excluded.repository_root, active_branch = excluded.active_branch",
        )
        .bind(&session.id)
        .bind(&session.project_id)
        .bind(session.created_at as i64)
        .bind(session.updated_at as i64)
        .bind(&session.repository_root)
        .bind(&session.active_branch)
        .bind(&session.cache_namespace)
        .execute(&pool)
        .await
        .map_err(|err| err.to_string())?;
    }
    for turn in &snapshot.turns {
        sqlx::query(
            "INSERT INTO agent_turns (id, session_id, sequence, started_at, ended_at, input_tokens, output_tokens, estimated_cost)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET ended_at = excluded.ended_at, input_tokens = excluded.input_tokens, output_tokens = excluded.output_tokens, estimated_cost = excluded.estimated_cost",
        )
        .bind(&turn.id)
        .bind(&turn.session_id)
        .bind(turn.sequence as i64)
        .bind(turn.started_at.map(|n| n as i64))
        .bind(turn.ended_at.map(|n| n as i64))
        .bind(turn.input_tokens as i64)
        .bind(turn.output_tokens as i64)
        .bind(turn.estimated_cost)
        .execute(&pool)
        .await
        .map_err(|err| err.to_string())?;
    }
    for tool in &snapshot.tools {
        sqlx::query(
            "INSERT INTO agent_tool_calls (id, session_id, turn_id, tool_name, input_hash, output_tokens, duration_ms, cache_hit, repeated, estimated_cost)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET output_tokens = excluded.output_tokens, duration_ms = excluded.duration_ms, cache_hit = excluded.cache_hit, repeated = excluded.repeated, estimated_cost = excluded.estimated_cost",
        )
        .bind(&tool.id)
        .bind(&tool.session_id)
        .bind(&tool.turn_id)
        .bind(&tool.tool_name)
        .bind(&tool.input_hash)
        .bind(tool.output_tokens as i64)
        .bind(tool.duration_ms as i64)
        .bind(tool.cache_hit as i64)
        .bind(tool.repeated as i64)
        .bind(tool.context_cost_usd)
        .execute(&pool)
        .await
        .map_err(|err| err.to_string())?;
    }
    for model in &snapshot.models {
        sqlx::query(
            "INSERT INTO agent_model_calls (id, session_id, turn_id, provider, model, input_tokens, output_tokens, cached_input_tokens, estimated_cost, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET estimated_cost = excluded.estimated_cost",
        )
        .bind(&model.id)
        .bind(&model.session_id)
        .bind(&model.turn_id)
        .bind(&model.provider)
        .bind(&model.model)
        .bind(model.input_tokens.map(|n| n as i64))
        .bind(model.output_tokens.map(|n| n as i64))
        .bind(model.cached_input_tokens.map(|n| n as i64))
        .bind(model.estimated_cost)
        .bind(model.created_at as i64)
        .execute(&pool)
        .await
        .map_err(|err| err.to_string())?;
    }
    for optimization in &snapshot.optimizations {
        sqlx::query(
            "INSERT INTO agent_optimizations (id, tool_call_id, alternative_tool, current_tokens, alternative_tokens, reduction_percent, confidence, reason, accepted)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET accepted = excluded.accepted",
        )
        .bind(&optimization.id)
        .bind(&optimization.tool_call_id)
        .bind(&optimization.alternative.tool_name)
        .bind(optimization.alternative.estimated_current_tokens as i64)
        .bind(optimization.alternative.estimated_alternative_tokens as i64)
        .bind(optimization.alternative.estimated_reduction_percent)
        .bind(optimization.alternative.confidence.as_str())
        .bind(&optimization.alternative.reason)
        .bind(optimization.accepted as i64)
        .execute(&pool)
        .await
        .map_err(|err| err.to_string())?;
    }
    for entry in &snapshot.cache_entries {
        let hashes = json!(entry.source_hashes).to_string();
        sqlx::query(
            "INSERT INTO agent_context_cache (key, project_id, created_at, estimated_tokens, source_hashes, git_revision, policy_version, skill_version, graph_version)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET estimated_tokens = excluded.estimated_tokens, source_hashes = excluded.source_hashes",
        )
        .bind(&entry.key)
        .bind(&entry.project_id)
        .bind(entry.created_at as i64)
        .bind(entry.estimated_tokens as i64)
        .bind(hashes)
        .bind(&entry.git_revision)
        .bind(&entry.policy_version)
        .bind(&entry.skill_version)
        .bind(&entry.graph_version)
        .execute(&pool)
        .await
        .map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub async fn load(path: &Path) -> Result<PersistedSnapshot, String> {
    if !path.exists() {
        return Ok(PersistedSnapshot {
            sessions: Vec::new(),
            turns: Vec::new(),
            tools: Vec::new(),
            models: Vec::new(),
            optimizations: Vec::new(),
            cache_entries: Vec::new(),
        });
    }
    let pool = open(path).await?;
    ensure_schema(&pool).await?;
    let session_rows = sqlx::query("SELECT id, project_id, created_at, updated_at, repository_root, active_branch, cache_namespace FROM agent_sessions")
        .fetch_all(&pool)
        .await
        .map_err(|err| err.to_string())?;
    let sessions = session_rows
        .iter()
        .map(|row| SessionRow {
            id: row.get("id"),
            project_id: row.get("project_id"),
            created_at: row.get::<i64, _>("created_at") as u64,
            updated_at: row.get::<i64, _>("updated_at") as u64,
            repository_root: row.get("repository_root"),
            active_branch: row.get("active_branch"),
            cache_namespace: row.get("cache_namespace"),
        })
        .collect();
    let turn_rows = sqlx::query("SELECT id, session_id, sequence, started_at, ended_at, input_tokens, output_tokens, estimated_cost FROM agent_turns")
        .fetch_all(&pool)
        .await
        .map_err(|err| err.to_string())?;
    let turns = turn_rows
        .iter()
        .map(|row| TurnRow {
            id: row.get("id"),
            session_id: row.get("session_id"),
            sequence: row.get::<i64, _>("sequence") as u32,
            started_at: row.get::<Option<i64>, _>("started_at").map(|n| n as u64),
            ended_at: row.get::<Option<i64>, _>("ended_at").map(|n| n as u64),
            input_tokens: row.get::<i64, _>("input_tokens") as u64,
            output_tokens: row.get::<i64, _>("output_tokens") as u64,
            estimated_cost: row.get("estimated_cost"),
        })
        .collect();
    let tool_rows = sqlx::query(
        "SELECT id, session_id, turn_id, tool_name, input_hash, output_tokens, duration_ms, cache_hit, repeated, estimated_cost FROM agent_tool_calls",
    )
    .fetch_all(&pool)
    .await
    .map_err(|err| err.to_string())?;
    let tools = tool_rows
        .iter()
        .map(|row| ToolEconomicsRecord {
            id: row.get("id"),
            session_id: row.get("session_id"),
            turn_id: row.get("turn_id"),
            tool_name: row.get("tool_name"),
            input_hash: row.get("input_hash"),
            input_bytes: None,
            output_bytes: None,
            output_tokens: row.get::<i64, _>("output_tokens") as u32,
            token_confidence: "estimated".into(),
            duration_ms: row.get::<i64, _>("duration_ms") as u64,
            execution_cost_usd: 0.0,
            context_cost_usd: row.get("estimated_cost"),
            cache_hit: row.get::<i64, _>("cache_hit") != 0,
            repeated: row.get::<i64, _>("repeated") != 0,
            alternative: None,
        })
        .collect();
    let model_rows = sqlx::query(
        "SELECT id, session_id, turn_id, provider, model, input_tokens, output_tokens, cached_input_tokens, estimated_cost, created_at FROM agent_model_calls",
    )
    .fetch_all(&pool)
    .await
    .map_err(|err| err.to_string())?;
    let models = model_rows
        .iter()
        .map(|row| ModelRow {
            id: row.get("id"),
            session_id: row.get("session_id"),
            turn_id: row.get("turn_id"),
            provider: row.get("provider"),
            model: row.get("model"),
            input_tokens: row.get::<Option<i64>, _>("input_tokens").map(|n| n as u64),
            output_tokens: row.get::<Option<i64>, _>("output_tokens").map(|n| n as u64),
            cached_input_tokens: row
                .get::<Option<i64>, _>("cached_input_tokens")
                .map(|n| n as u64),
            estimated_cost: row.get("estimated_cost"),
            created_at: row.get::<i64, _>("created_at") as u64,
        })
        .collect();
    let optimization_rows = sqlx::query(
        "SELECT id, tool_call_id, alternative_tool, current_tokens, alternative_tokens, reduction_percent, confidence, reason, accepted FROM agent_optimizations",
    )
    .fetch_all(&pool)
    .await
    .map_err(|err| err.to_string())?;
    let optimizations = optimization_rows
        .iter()
        .map(|row| {
            let confidence = match row.get::<String, _>("confidence").as_str() {
                "high" => Confidence::High,
                "medium" => Confidence::Medium,
                _ => Confidence::Low,
            };
            OptimizationRow {
                id: row.get("id"),
                tool_call_id: row.get("tool_call_id"),
                accepted: row.get::<i64, _>("accepted") != 0,
                alternative: ToolAlternative {
                    tool_name: row.get("alternative_tool"),
                    estimated_current_tokens: row.get::<i64, _>("current_tokens") as u32,
                    estimated_alternative_tokens: row.get::<i64, _>("alternative_tokens") as u32,
                    estimated_reduction_percent: row.get("reduction_percent"),
                    confidence,
                    reason: row.get("reason"),
                    rule: "stored".into(),
                },
            }
        })
        .collect();
    Ok(PersistedSnapshot {
        sessions,
        turns,
        tools,
        models,
        optimizations,
        cache_entries: Vec::new(),
    })
}

pub async fn record_advice(
    path: &Path,
    observation: &crate::types::ToolCallObservation,
    alternative: &ToolAlternative,
) -> Result<(), String> {
    if !path.is_file() {
        return Ok(());
    }
    let pool = open(path).await?;
    ensure_schema(&pool).await?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0);
    sqlx::query(
        "INSERT INTO agent_sessions (id, project_id, created_at, updated_at, cache_namespace)
         VALUES (?, 'read-guard', ?, ?, '')
         ON CONFLICT(id) DO UPDATE SET updated_at = excluded.updated_at",
    )
    .bind(&observation.session_id)
    .bind(now)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|err| err.to_string())?;
    sqlx::query(
        "INSERT INTO agent_tool_calls (id, session_id, turn_id, tool_name, input_hash, output_tokens, duration_ms, cache_hit, repeated, estimated_cost)
         VALUES (?, ?, ?, ?, 'read-guard', 0, 0, 0, 0, NULL)
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(&observation.call_id)
    .bind(&observation.session_id)
    .bind(&observation.turn_id)
    .bind(&observation.tool_name)
    .execute(&pool)
    .await
    .map_err(|err| err.to_string())?;
    sqlx::query(
        "INSERT INTO agent_optimizations (id, tool_call_id, alternative_tool, current_tokens, alternative_tokens, reduction_percent, confidence, reason, accepted)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0)",
    )
    .bind(format!("opt-{}", observation.call_id))
    .bind(&observation.call_id)
    .bind(&alternative.tool_name)
    .bind(alternative.estimated_current_tokens as i64)
    .bind(alternative.estimated_alternative_tokens as i64)
    .bind(alternative.estimated_reduction_percent)
    .bind(alternative.confidence.as_str())
    .bind(&alternative.reason)
    .execute(&pool)
    .await
    .map_err(|err| err.to_string())?;
    Ok(())
}

async fn open(path: &Path) -> Result<SqlitePool, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);
    SqlitePool::connect_with(options)
        .await
        .map_err(|err| err.to_string())
}

async fn ensure_schema(pool: &SqlitePool) -> Result<(), String> {
    for statement in SCHEMA.split(';') {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        sqlx::query(trimmed)
            .execute(pool)
            .await
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}
