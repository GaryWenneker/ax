//! SQLite persistence for MCP context savings metrics.

use std::path::PathBuf;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous};
use sqlx::ConnectOptions;

use ax_utils::errors::{AxError, DatabaseError};

pub const USAGE_DB_FILENAME: &str = "usage.db";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS mcp_call_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  tool TEXT NOT NULL,
  project TEXT,
  response_chars INTEGER NOT NULL,
  response_tokens_est INTEGER NOT NULL,
  counterfactual_files INTEGER,
  counterfactual_tokens_est INTEGER,
  tokens_saved_est INTEGER,
  duration_ms INTEGER,
  ok INTEGER NOT NULL,
  savings_eligible INTEGER NOT NULL DEFAULT 0,
  counterfactual_exact_files INTEGER,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_mcp_call_log_created_at ON mcp_call_log(created_at);
CREATE INDEX IF NOT EXISTS idx_mcp_call_log_tool ON mcp_call_log(tool);

CREATE TABLE IF NOT EXISTS mcp_session_index (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT,
  tool TEXT NOT NULL,
  summary TEXT NOT NULL,
  original_tokens INTEGER NOT NULL,
  cache_id TEXT,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_mcp_session_index_created ON mcp_session_index(created_at);

CREATE TABLE IF NOT EXISTS mcp_context_cache (
  id TEXT PRIMARY KEY,
  tool TEXT NOT NULL,
  body TEXT NOT NULL,
  original_tokens INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS mcp_reuse_cache (
  reuse_key TEXT PRIMARY KEY,
  conversation TEXT NOT NULL,
  tool TEXT NOT NULL,
  args_summary TEXT NOT NULL,
  cache_id TEXT NOT NULL,
  body_bytes INTEGER NOT NULL,
  original_tokens INTEGER NOT NULL,
  files_json TEXT NOT NULL,
  index_fingerprint TEXT NOT NULL DEFAULT '',
  turn INTEGER NOT NULL,
  hits INTEGER NOT NULL DEFAULT 0,
  tokens_avoided INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_mcp_reuse_cache_conv ON mcp_reuse_cache(conversation, created_at);

CREATE TABLE IF NOT EXISTS mcp_working_context (
  scope TEXT PRIMARY KEY,
  objective TEXT NOT NULL DEFAULT '',
  facts TEXT NOT NULL DEFAULT '[]',
  files TEXT NOT NULL DEFAULT '[]',
  symbols TEXT NOT NULL DEFAULT '[]',
  decisions TEXT NOT NULL DEFAULT '[]',
  open_questions TEXT NOT NULL DEFAULT '[]',
  content_hash TEXT NOT NULL,
  index_fingerprint TEXT NOT NULL DEFAULT '',
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS ax_durable_conversation (
  id TEXT PRIMARY KEY,
  parent_id TEXT,
  fork_entry INTEGER,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS ax_durable_entry (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  conversation_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  body TEXT NOT NULL,
  head INTEGER,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ax_durable_entry_conv ON ax_durable_entry(conversation_id, id);

CREATE TABLE IF NOT EXISTS ax_durable_document (
  conversation_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  body TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (conversation_id, kind)
);

CREATE TABLE IF NOT EXISTS ax_durable_task (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  status TEXT NOT NULL,
  phase TEXT NOT NULL,
  checkpoint TEXT NOT NULL,
  result TEXT,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS ax_durable_hook (
  conversation_id TEXT NOT NULL,
  event TEXT NOT NULL,
  name TEXT NOT NULL,
  PRIMARY KEY (conversation_id, event, name)
);

CREATE TABLE IF NOT EXISTS agent_session_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  agent TEXT NOT NULL,
  session_id TEXT NOT NULL,
  read_calls INTEGER NOT NULL DEFAULT 0,
  grep_calls INTEGER NOT NULL DEFAULT 0,
  ax_calls INTEGER NOT NULL DEFAULT 0,
  session_input_tokens INTEGER,
  session_output_tokens INTEGER,
  cache_read_tokens INTEGER,
  cache_write_tokens INTEGER,
  model TEXT,
  source_mtime INTEGER NOT NULL,
  started_at INTEGER,
  ended_at INTEGER,
  UNIQUE(agent, session_id)
);

CREATE TABLE IF NOT EXISTS agent_usage_event (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  agent TEXT NOT NULL,
  session_id TEXT NOT NULL,
  turn_id TEXT NOT NULL DEFAULT '',
  project TEXT,
  provider TEXT,
  model TEXT,
  input_tokens INTEGER,
  output_tokens INTEGER,
  cache_read_tokens INTEGER,
  cache_write_tokens INTEGER,
  confidence TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE(agent, session_id, turn_id)
);
CREATE INDEX IF NOT EXISTS idx_agent_usage_event_created ON agent_usage_event(created_at);
";

const MIGRATION_DROP_TOKEN_USAGE: &str = "DROP TABLE IF EXISTS token_usage";

/// Additive columns for databases created before v2.2. Failures for
/// already-existing columns are ignored.
const MIGRATION_ADD_COLUMNS: &[&str] = &[
    "ALTER TABLE mcp_call_log ADD COLUMN counterfactual_exact_files INTEGER",
    "ALTER TABLE agent_session_log ADD COLUMN session_output_tokens INTEGER",
    "ALTER TABLE agent_session_log ADD COLUMN model TEXT",
    "ALTER TABLE mcp_call_log ADD COLUMN response_preview TEXT",
    "ALTER TABLE mcp_call_log ADD COLUMN counterfactual_preview TEXT",
    "ALTER TABLE mcp_reuse_cache ADD COLUMN index_fingerprint TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE agent_session_log ADD COLUMN cache_read_tokens INTEGER",
    "ALTER TABLE agent_session_log ADD COLUMN cache_write_tokens INTEGER",
    "ALTER TABLE model_price_daily ADD COLUMN cache_write_per_mtok REAL",
];

const PRICING_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS pricing_sync_meta (
  source TEXT PRIMARY KEY,
  last_attempt_at INTEGER,
  last_success_at INTEGER,
  last_success_date TEXT,
  status TEXT,
  error TEXT,
  models_count INTEGER
);

CREATE TABLE IF NOT EXISTS model_price_daily (
  date TEXT NOT NULL,
  source TEXT NOT NULL,
  model_id TEXT NOT NULL,
  display_name TEXT,
  provider TEXT,
  input_per_mtok REAL NOT NULL,
  output_per_mtok REAL NOT NULL,
  cache_read_per_mtok REAL,
  cache_write_per_mtok REAL,
  blended_3_to_1 REAL,
  context_length INTEGER,
  raw_json TEXT,
  PRIMARY KEY (date, source, model_id)
);
CREATE INDEX IF NOT EXISTS idx_model_price_daily_model ON model_price_daily(model_id, date);

CREATE TABLE IF NOT EXISTS model_benchmark_daily (
  date TEXT NOT NULL,
  source TEXT NOT NULL,
  model_id TEXT NOT NULL,
  display_name TEXT,
  intelligence REAL,
  coding REAL,
  agentic REAL,
  median_output_tps REAL,
  median_ttft_seconds REAL,
  PRIMARY KEY (date, source, model_id)
);

CREATE TABLE IF NOT EXISTS coding_agent_daily (
  date TEXT NOT NULL,
  agent TEXT NOT NULL,
  model TEXT,
  index_score REAL,
  cost_per_task REAL,
  time_per_task REAL,
  tokens_per_task REAL,
  raw_json TEXT,
  PRIMARY KEY (date, agent, model)
);
";

pub fn usage_db_path() -> PathBuf {
    if let Ok(path) = std::env::var("AX_USAGE_DB") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    ax_utils::paths::home_dir()
        .map(|h| h.join(".ax").join(USAGE_DB_FILENAME))
        .unwrap_or_else(|| PathBuf::from(".ax").join(USAGE_DB_FILENAME))
}

pub async fn open_pool() -> Result<SqlitePool, AxError> {
    open_pool_at(&usage_db_path()).await
}

pub(crate) async fn open_pool_at(path: &std::path::Path) -> Result<SqlitePool, AxError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    }

    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(180))
        .disable_statement_logging();

    let pool = SqlitePoolOptions::new()
        .max_connections(3)
        .acquire_timeout(std::time::Duration::from_secs(180))
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("PRAGMA busy_timeout = 180000")
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;

    sqlx::query(MIGRATION_DROP_TOKEN_USAGE)
        .execute(&pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(format!("usage migration: {e}"))))?;

    for stmt in SCHEMA.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query(stmt)
            .execute(&pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(format!("usage schema: {e}"))))?;
    }

    for stmt in PRICING_SCHEMA.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query(stmt)
            .execute(&pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(format!("pricing schema: {e}"))))?;
    }

    for stmt in MIGRATION_ADD_COLUMNS {
        // "duplicate column name" is expected on already-migrated databases.
        let _ = sqlx::query(stmt).execute(&pool).await;
    }

    Ok(pool)
}
