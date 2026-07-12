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

CREATE TABLE IF NOT EXISTS agent_session_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  agent TEXT NOT NULL,
  session_id TEXT NOT NULL,
  read_calls INTEGER NOT NULL DEFAULT 0,
  grep_calls INTEGER NOT NULL DEFAULT 0,
  ax_calls INTEGER NOT NULL DEFAULT 0,
  session_input_tokens INTEGER,
  session_output_tokens INTEGER,
  model TEXT,
  source_mtime INTEGER NOT NULL,
  started_at INTEGER,
  ended_at INTEGER,
  UNIQUE(agent, session_id)
);
";

const MIGRATION_DROP_TOKEN_USAGE: &str = "DROP TABLE IF EXISTS token_usage";

/// Additive columns for databases created before v2.2. Failures for
/// already-existing columns are ignored.
const MIGRATION_ADD_COLUMNS: &[&str] = &[
    "ALTER TABLE mcp_call_log ADD COLUMN counterfactual_exact_files INTEGER",
    "ALTER TABLE agent_session_log ADD COLUMN session_output_tokens INTEGER",
    "ALTER TABLE agent_session_log ADD COLUMN model TEXT",
];

pub fn usage_db_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".ax").join(USAGE_DB_FILENAME))
        .unwrap_or_else(|| PathBuf::from(".ax").join(USAGE_DB_FILENAME))
}

pub async fn open_pool() -> Result<SqlitePool, AxError> {
    let path = usage_db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    }

    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .disable_statement_logging();

    let pool = SqlitePoolOptions::new()
        .max_connections(3)
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

    for stmt in MIGRATION_ADD_COLUMNS {
        // "duplicate column name" is expected on already-migrated databases.
        let _ = sqlx::query(stmt).execute(&pool).await;
    }

    Ok(pool)
}
