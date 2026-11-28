//! Derived metrics in the project SQLite database.
//!
//! Rows store hashes, names, counts, and estimates. They do not store prompts,
//! completions, credentials, tool arguments, or tool output.

use std::path::Path;

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
";

#[derive(Debug, Clone)]
pub struct PersistedSnapshot {
    pub sessions: Vec<SessionRow>,
    pub turns: Vec<TurnRow>,
    pub tools: Vec<ToolEconomicsRecord>,
    pub models: Vec<ModelRow>,
    pub optimizations: Vec<OptimizationRow>,
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

pub async fn flush(path: &Path, snapshot: &PersistedSnapshot) -> Result<(), String> {
    let pool = open(path).await?;
    let result = flush_into(&pool, snapshot).await;
    pool.close().await;
    result
}

async fn flush_into(pool: &SqlitePool, snapshot: &PersistedSnapshot) -> Result<(), String> {
    ensure_schema(pool).await?;
    // One transaction: one commit for the whole snapshot, and a failure part
    // way leaves the database as it was.
    let mut tx = pool.begin().await.map_err(|err| err.to_string())?;
    for session in &snapshot.sessions {
        sqlx::query(
            "INSERT INTO agent_sessions (id, project_id, created_at, updated_at, repository_root, active_branch, cache_namespace)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET created_at = CASE WHEN agent_sessions.created_at = 0 THEN excluded.created_at ELSE MIN(agent_sessions.created_at, excluded.created_at) END, updated_at = MAX(agent_sessions.updated_at, excluded.updated_at), repository_root = excluded.repository_root, active_branch = excluded.active_branch",
        )
        .bind(&session.id)
        .bind(&session.project_id)
        .bind(session.created_at as i64)
        .bind(session.updated_at as i64)
        .bind(&session.repository_root)
        .bind(&session.active_branch)
        .bind(&session.cache_namespace)
        .execute(&mut *tx)
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
        .execute(&mut *tx)
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
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
    }
    for model in &snapshot.models {
        sqlx::query(
            "INSERT INTO agent_model_calls (id, session_id, turn_id, provider, model, input_tokens, output_tokens, cached_input_tokens, estimated_cost, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET session_id = excluded.session_id, turn_id = excluded.turn_id, provider = excluded.provider, model = excluded.model, input_tokens = excluded.input_tokens, output_tokens = excluded.output_tokens, cached_input_tokens = excluded.cached_input_tokens, estimated_cost = excluded.estimated_cost, created_at = excluded.created_at",
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
        .execute(&mut *tx)
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
        .execute(&mut *tx)
        .await
        .map_err(|err| err.to_string())?;
    }
    tx.commit().await.map_err(|err| err.to_string())?;
    Ok(())
}

pub async fn load(path: &Path) -> Result<PersistedSnapshot, String> {
    // A report only reads: a missing database stays missing, and an existing
    // one is opened read-only and gets no tables added.
    if !tokio::fs::try_exists(path).await.unwrap_or(false) {
        return Ok(empty_snapshot());
    }
    let pool = open_read_only(path).await?;
    let result = load_from(&pool).await;
    pool.close().await;
    result
}

fn empty_snapshot() -> PersistedSnapshot {
    PersistedSnapshot {
        sessions: Vec::new(),
        turns: Vec::new(),
        tools: Vec::new(),
        models: Vec::new(),
        optimizations: Vec::new(),
    }
}

/// The agent tables that exist in the database behind `pool`.
async fn existing_tables(pool: &SqlitePool) -> Result<Vec<String>, String> {
    let rows = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN \
         ('agent_sessions', 'agent_turns', 'agent_tool_calls', 'agent_model_calls', 'agent_optimizations')",
    )
    .fetch_all(pool)
    .await
    .map_err(|err| err.to_string())?;
    Ok(rows.iter().map(|row| row.get::<String, _>(0)).collect())
}

/// Run `sql` when `table` exists; a table that was never created has no rows.
async fn rows_of(
    pool: &SqlitePool,
    tables: &[String],
    table: &str,
    sql: &'static str,
) -> Result<Vec<sqlx::sqlite::SqliteRow>, String> {
    if !tables.iter().any(|name| name == table) {
        return Ok(Vec::new());
    }
    sqlx::query(sql)
        .fetch_all(pool)
        .await
        .map_err(|err| err.to_string())
}

async fn load_from(pool: &SqlitePool) -> Result<PersistedSnapshot, String> {
    let tables = existing_tables(pool).await?;
    let session_rows = rows_of(
        pool,
        &tables,
        "agent_sessions",
        "SELECT id, project_id, created_at, updated_at, repository_root, active_branch, cache_namespace FROM agent_sessions",
    )
    .await?;
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
    let turn_rows = rows_of(
        pool,
        &tables,
        "agent_turns",
        "SELECT id, session_id, sequence, started_at, ended_at, input_tokens, output_tokens, estimated_cost FROM agent_turns",
    )
    .await?;
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
    let tool_rows = rows_of(
        pool,
        &tables,
        "agent_tool_calls",
        "SELECT id, session_id, turn_id, tool_name, input_hash, output_tokens, duration_ms, cache_hit, repeated, estimated_cost FROM agent_tool_calls",
    )
    .await?;
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
    let model_rows = rows_of(
        pool,
        &tables,
        "agent_model_calls",
        "SELECT id, session_id, turn_id, provider, model, input_tokens, output_tokens, cached_input_tokens, estimated_cost, created_at FROM agent_model_calls",
    )
    .await?;
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
    let optimization_rows = rows_of(
        pool,
        &tables,
        "agent_optimizations",
        "SELECT id, tool_call_id, alternative_tool, current_tokens, alternative_tokens, reduction_percent, confidence, reason, accepted FROM agent_optimizations",
    )
    .await?;
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
    })
}

pub async fn record_advice(
    path: &Path,
    observation: &crate::types::ToolCallObservation,
    alternative: &ToolAlternative,
) -> Result<(), String> {
    let is_file = tokio::fs::metadata(path)
        .await
        .map(|meta| meta.is_file())
        .unwrap_or(false);
    if !is_file {
        return Ok(());
    }
    let pool = open(path).await?;
    let result = record_advice_into(&pool, observation, alternative).await;
    pool.close().await;
    result
}

async fn record_advice_into(
    pool: &SqlitePool,
    observation: &crate::types::ToolCallObservation,
    alternative: &ToolAlternative,
) -> Result<(), String> {
    ensure_schema(pool).await?;
    // The three rows are written together or not at all.
    let mut tx = pool.begin().await.map_err(|err| err.to_string())?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0);
    sqlx::query(
        "INSERT INTO agent_sessions (id, project_id, created_at, updated_at, cache_namespace)
         VALUES (?, 'read-guard', ?, ?, '')
         ON CONFLICT(id) DO UPDATE SET updated_at = MAX(agent_sessions.updated_at, excluded.updated_at)",
    )
    .bind(&observation.session_id)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
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
    .execute(&mut *tx)
    .await
    .map_err(|err| err.to_string())?;
    sqlx::query(
        "INSERT INTO agent_optimizations (id, tool_call_id, alternative_tool, current_tokens, alternative_tokens, reduction_percent, confidence, reason, accepted)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0)
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(format!("opt-{}", observation.call_id))
    .bind(&observation.call_id)
    .bind(&alternative.tool_name)
    .bind(alternative.estimated_current_tokens as i64)
    .bind(alternative.estimated_alternative_tokens as i64)
    .bind(alternative.estimated_reduction_percent)
    .bind(alternative.confidence.as_str())
    .bind(&alternative.reason)
    .execute(&mut *tx)
    .await
    .map_err(|err| err.to_string())?;
    tx.commit().await.map_err(|err| err.to_string())?;
    Ok(())
}

/// Each store call runs one transaction, so one connection is enough; the
/// caller closes the pool when the call ends.
async fn open(path: &Path) -> Result<SqlitePool, String> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|err| err.to_string())?;
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);
    single_connection(options).await
}

async fn open_read_only(path: &Path) -> Result<SqlitePool, String> {
    let options = SqliteConnectOptions::new().filename(path).read_only(true);
    single_connection(options).await
}

async fn single_connection(options: SqliteConnectOptions) -> Result<SqlitePool, String> {
    sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|err| err.to_string())
}

/// Create the agent tables that are missing. When all of them exist this is
/// one read of `sqlite_master` and no DDL.
async fn ensure_schema(pool: &SqlitePool) -> Result<(), String> {
    if existing_tables(pool).await?.len() == SCHEMA.matches("CREATE TABLE").count() {
        return Ok(());
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolCallObservation;

    fn alternative(confidence: Confidence) -> ToolAlternative {
        ToolAlternative {
            tool_name: "ax_explore".into(),
            estimated_current_tokens: 4000,
            estimated_alternative_tokens: 600,
            estimated_reduction_percent: 85.0,
            confidence,
            reason: "graph answer".into(),
            rule: "rg-symbol".into(),
        }
    }

    fn snapshot() -> PersistedSnapshot {
        PersistedSnapshot {
            sessions: vec![SessionRow {
                id: "s1".into(),
                project_id: "p".into(),
                created_at: 10,
                updated_at: 20,
                repository_root: Some("/repo".into()),
                active_branch: Some("main".into()),
                cache_namespace: "ns".into(),
            }],
            turns: vec![TurnRow {
                id: "t1".into(),
                session_id: "s1".into(),
                sequence: 1,
                started_at: Some(11),
                ended_at: Some(19),
                input_tokens: 300,
                output_tokens: 40,
                estimated_cost: Some(0.5),
            }],
            tools: vec![ToolEconomicsRecord {
                id: "c1".into(),
                session_id: "s1".into(),
                turn_id: "t1".into(),
                tool_name: "rg".into(),
                input_hash: "h".into(),
                input_bytes: None,
                output_bytes: None,
                output_tokens: 4000,
                token_confidence: "estimated".into(),
                duration_ms: 7,
                execution_cost_usd: 0.0,
                context_cost_usd: Some(0.01),
                cache_hit: false,
                repeated: true,
                alternative: None,
            }],
            models: vec![ModelRow {
                id: "m1".into(),
                session_id: "s1".into(),
                turn_id: Some("t1".into()),
                provider: Some("anthropic".into()),
                model: Some("claude".into()),
                input_tokens: Some(300),
                output_tokens: Some(40),
                cached_input_tokens: Some(5),
                estimated_cost: Some(0.5),
                created_at: 12,
            }],
            optimizations: vec![
                OptimizationRow {
                    id: "o1".into(),
                    tool_call_id: "c1".into(),
                    alternative: alternative(Confidence::High),
                    accepted: true,
                },
                OptimizationRow {
                    id: "o2".into(),
                    tool_call_id: "c1".into(),
                    alternative: alternative(Confidence::Medium),
                    accepted: false,
                },
            ],
        }
    }

    #[tokio::test]
    async fn flush_then_load_round_trips_every_row_kind() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ax.db");
        flush(&db, &snapshot()).await.unwrap();
        let loaded = load(&db).await.unwrap();

        assert_eq!(loaded.sessions.len(), 1);
        let session = &loaded.sessions[0];
        assert_eq!(
            (
                session.created_at,
                session.updated_at,
                session.cache_namespace.as_str()
            ),
            (10, 20, "ns")
        );
        assert_eq!(session.repository_root.as_deref(), Some("/repo"));
        let turn = &loaded.turns[0];
        assert_eq!(
            (
                turn.sequence,
                turn.ended_at,
                turn.input_tokens,
                turn.output_tokens
            ),
            (1, Some(19), 300, 40)
        );
        let tool = &loaded.tools[0];
        assert_eq!((tool.output_tokens, tool.duration_ms), (4000, 7));
        assert!(tool.repeated && !tool.cache_hit);
        assert_eq!(tool.context_cost_usd, Some(0.01));
        let model = &loaded.models[0];
        assert_eq!(model.cached_input_tokens, Some(5));
        assert_eq!(model.model.as_deref(), Some("claude"));

        let mut optimizations = loaded.optimizations.clone();
        optimizations.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(optimizations.len(), 2);
        let first = &optimizations[0];
        assert_eq!(first.tool_call_id, "c1");
        assert!(first.accepted);
        assert_eq!(first.alternative.confidence, Confidence::High);
        assert_eq!(first.alternative.tool_name, "ax_explore");
        assert_eq!(first.alternative.estimated_current_tokens, 4000);
        assert_eq!(first.alternative.estimated_alternative_tokens, 600);
        assert_eq!(first.alternative.estimated_reduction_percent, 85.0);
        assert_eq!(first.alternative.reason, "graph answer");
        assert!(!optimizations[1].accepted);
        assert_eq!(optimizations[1].alternative.confidence, Confidence::Medium);
    }

    #[tokio::test]
    async fn load_of_a_missing_database_is_empty_and_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("absent.db");
        let loaded = load(&db).await.unwrap();
        assert!(loaded.sessions.is_empty() && loaded.optimizations.is_empty());
        assert!(!db.exists());
    }

    fn observation(call_id: &str) -> ToolCallObservation {
        ToolCallObservation {
            session_id: "conv-1".into(),
            turn_id: "gen-1".into(),
            call_id: call_id.into(),
            tool_name: "Read".into(),
            arguments: serde_json::json!({}),
            repository_state: String::new(),
            known_graph_entities: Vec::new(),
            indexed_paths: Vec::new(),
            output_text_for_estimate: None,
            output_token_estimate: None,
            output_bytes: None,
            duration_ms: 0,
            success: true,
        }
    }

    #[tokio::test]
    async fn record_advice_writes_session_call_and_optimization_into_an_existing_database() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ax.db");
        record_advice(&db, &observation("g1"), &alternative(Confidence::Low))
            .await
            .unwrap();
        assert!(!db.exists(), "a missing database is not created");

        flush(&db, &snapshot()).await.unwrap();
        record_advice(&db, &observation("g1"), &alternative(Confidence::Low))
            .await
            .unwrap();
        let loaded = load(&db).await.unwrap();
        assert!(loaded.sessions.iter().any(|s| s.id == "conv-1"));
        let call = loaded
            .tools
            .iter()
            .find(|t| t.id == "g1")
            .expect("call row");
        assert_eq!(
            (call.session_id.as_str(), call.turn_id.as_str()),
            ("conv-1", "gen-1")
        );
        assert_eq!(call.tool_name, "Read");
        let advice = loaded
            .optimizations
            .iter()
            .find(|o| o.tool_call_id == "g1")
            .expect("optimization row");
        assert_eq!(advice.id, "opt-g1");
        assert!(!advice.accepted);
        assert_eq!(advice.alternative.confidence, Confidence::Low);
        // The first run's rows are still there.
        assert!(loaded.optimizations.iter().any(|o| o.id == "o1"));
    }

    #[tokio::test]
    async fn a_flush_that_fails_part_way_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ax.db");
        let pool = open(&db).await.unwrap();
        ensure_schema(&pool).await.unwrap();
        sqlx::query(
            "CREATE TRIGGER refuse_optimizations BEFORE INSERT ON agent_optimizations
             BEGIN SELECT RAISE(ABORT, 'refused'); END",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let err = flush(&db, &snapshot()).await.unwrap_err();
        assert!(err.contains("refused"), "{err}");
        let loaded = load(&db).await.unwrap();
        assert!(loaded.sessions.is_empty(), "sessions were committed");
        assert!(loaded.turns.is_empty() && loaded.tools.is_empty());
        assert!(loaded.models.is_empty());
    }

    #[tokio::test]
    async fn record_advice_twice_for_the_same_call_keeps_one_row_and_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ax.db");
        flush(&db, &snapshot()).await.unwrap();
        for _ in 0..2 {
            record_advice(&db, &observation("g1"), &alternative(Confidence::Low))
                .await
                .unwrap();
        }
        let loaded = load(&db).await.unwrap();
        let rows = loaded
            .optimizations
            .iter()
            .filter(|o| o.tool_call_id == "g1")
            .count();
        assert_eq!(rows, 1);
    }

    #[tokio::test]
    async fn record_advice_that_fails_part_way_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ax.db");
        let pool = open(&db).await.unwrap();
        ensure_schema(&pool).await.unwrap();
        sqlx::query(
            "CREATE TRIGGER refuse_optimizations BEFORE INSERT ON agent_optimizations
             BEGIN SELECT RAISE(ABORT, 'refused'); END",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let err = record_advice(&db, &observation("g1"), &alternative(Confidence::Low))
            .await
            .unwrap_err();
        assert!(err.contains("refused"), "{err}");
        let loaded = load(&db).await.unwrap();
        assert!(loaded.sessions.is_empty() && loaded.tools.is_empty());
    }

    async fn raw_pool(db: &Path) -> SqlitePool {
        SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(db)
                .create_if_missing(true),
        )
        .await
        .unwrap()
    }

    async fn table_names(db: &Path) -> Vec<String> {
        let pool = raw_pool(db).await;
        let names =
            sqlx::query("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
                .fetch_all(&pool)
                .await
                .unwrap()
                .iter()
                .map(|row| row.get::<String, _>(0))
                .collect();
        pool.close().await;
        names
    }

    #[tokio::test]
    async fn load_reads_without_adding_tables_to_a_database_that_has_none() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ax.db");
        let pool = raw_pool(&db).await;
        sqlx::query("CREATE TABLE other (x INTEGER)")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let loaded = load(&db).await.unwrap();
        assert!(loaded.sessions.is_empty() && loaded.optimizations.is_empty());
        assert_eq!(table_names(&db).await, vec!["other".to_string()]);
    }

    #[tokio::test]
    async fn load_and_the_schema_check_do_not_wait_on_another_writer() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ax.db");
        flush(&db, &snapshot()).await.unwrap();

        // Another process holds the write lock for the whole check.
        let holder = raw_pool(&db).await;
        let mut writer = holder.acquire().await.unwrap();
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut *writer)
            .await
            .unwrap();

        let loaded = tokio::time::timeout(std::time::Duration::from_secs(2), load(&db))
            .await
            .expect("load waited on the writer")
            .unwrap();
        assert_eq!(loaded.sessions.len(), 1);
        let pool = open(&db).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), ensure_schema(&pool))
            .await
            .expect("ensure_schema waited on the writer")
            .unwrap();
        pool.close().await;

        sqlx::query("ROLLBACK").execute(&mut *writer).await.unwrap();
        drop(writer);
        holder.close().await;
    }

    #[tokio::test]
    async fn flush_adds_the_tables_a_partial_database_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ax.db");
        let pool = raw_pool(&db).await;
        sqlx::query("CREATE TABLE agent_sessions (id TEXT PRIMARY KEY, project_id TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, repository_root TEXT, active_branch TEXT, cache_namespace TEXT NOT NULL DEFAULT '')")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        flush(&db, &snapshot()).await.unwrap();
        let loaded = load(&db).await.unwrap();
        assert_eq!(loaded.sessions.len(), 1);
        assert_eq!(loaded.optimizations.len(), 2);
    }
}
