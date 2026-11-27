//! SQLite persistence for MCP context savings metrics.

use std::path::PathBuf;

use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};
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
  created_at INTEGER NOT NULL,
  replica_origin TEXT,
  transport TEXT NOT NULL DEFAULT 'mcp'
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
CREATE INDEX IF NOT EXISTS idx_mcp_session_index_session ON mcp_session_index(session_id, created_at);

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

CREATE TABLE IF NOT EXISTS ax_durable_scope (
  project TEXT NOT NULL,
  session TEXT NOT NULL,
  conversation TEXT NOT NULL,
  PRIMARY KEY (project, session)
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
  replica_origin TEXT,
  UNIQUE(agent, session_id, turn_id)
);
CREATE INDEX IF NOT EXISTS idx_agent_usage_event_created ON agent_usage_event(created_at);

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

const MIGRATION_DROP_TOKEN_USAGE: &str = "DROP TABLE IF EXISTS token_usage";

/// Additive columns for older databases. A "duplicate column name" failure is
/// expected and ignored; any other failure is an error.
const MIGRATION_ADD_COLUMNS: &[&str] = &[
    "ALTER TABLE mcp_context_cache ADD COLUMN project TEXT",
    "ALTER TABLE mcp_call_log ADD COLUMN counterfactual_exact_files INTEGER",
    "ALTER TABLE agent_session_log ADD COLUMN session_output_tokens INTEGER",
    "ALTER TABLE agent_session_log ADD COLUMN model TEXT",
    "ALTER TABLE mcp_call_log ADD COLUMN response_preview TEXT",
    "ALTER TABLE mcp_call_log ADD COLUMN counterfactual_preview TEXT",
    "ALTER TABLE mcp_reuse_cache ADD COLUMN index_fingerprint TEXT NOT NULL DEFAULT ''",
    "ALTER TABLE agent_session_log ADD COLUMN cache_read_tokens INTEGER",
    "ALTER TABLE agent_session_log ADD COLUMN cache_write_tokens INTEGER",
    "ALTER TABLE model_price_daily ADD COLUMN cache_write_per_mtok REAL",
    "ALTER TABLE mcp_call_log ADD COLUMN replica_origin TEXT",
    "ALTER TABLE agent_usage_event ADD COLUMN replica_origin TEXT",
    "ALTER TABLE mcp_call_log ADD COLUMN transport TEXT NOT NULL DEFAULT 'mcp'",
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

/// Fingerprint of the schema and migrations below, stored in
/// `PRAGMA user_version` once they have run. FNV-1a, so it is stable across
/// builds; any change to the DDL changes it and the schema runs again.
fn schema_fingerprint() -> i32 {
    let mut hash: u32 = 0x811c_9dc5;
    let parts = [SCHEMA, PRICING_SCHEMA, MIGRATION_DROP_TOKEN_USAGE]
        .into_iter()
        .chain(MIGRATION_ADD_COLUMNS.iter().copied());
    for part in parts {
        for byte in part.bytes().chain(std::iter::once(0)) {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
    }
    // Keep it positive and non-zero: 0 is a fresh database.
    ((hash & 0x7fff_ffff) | 1) as i32
}

/// Identity of the database file a cached pool was opened on, so a file that
/// was deleted or replaced at the same path gets a new pool.
#[derive(Clone, Copy, PartialEq, Eq)]
struct FileId {
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
}

fn file_id(path: &std::path::Path) -> Option<FileId> {
    let meta = std::fs::metadata(path).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(FileId {
            dev: meta.dev(),
            ino: meta.ino(),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        Some(FileId {})
    }
}

type PoolCache = std::collections::HashMap<PathBuf, (SqlitePool, FileId)>;

fn pool_cache() -> &'static tokio::sync::Mutex<PoolCache> {
    static CACHE: std::sync::OnceLock<tokio::sync::Mutex<PoolCache>> = std::sync::OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// The shared pool for the usage database at [`usage_db_path`]. One pool is
/// kept per path for the life of the process, so callers get a clone instead of
/// opening new connections each time. A closed pool, or a file that was removed
/// or replaced, is opened again.
pub async fn open_pool() -> Result<SqlitePool, AxError> {
    let path = usage_db_path();
    let mut cache = pool_cache().lock().await;
    if let Some((pool, id)) = cache.get(&path) {
        if !pool.is_closed() && file_id(&path) == Some(*id) {
            return Ok(pool.clone());
        }
        let (stale, _) = cache.remove(&path).expect("cached pool");
        stale.close().await;
    }
    let pool = open_pool_at(&path).await?;
    if let Some(id) = file_id(&path) {
        cache.insert(path, (pool.clone(), id));
    }
    Ok(pool)
}

/// How long a statement waits for another writer, and a caller waits for a
/// free connection, before failing. The usage database serves interactive
/// tool calls, so a contended file fails fast instead of stalling a call.
/// `AX_USAGE_DB_BUSY_TIMEOUT_SECS` overrides it (whole seconds, 1 to 600).
fn usage_db_wait() -> std::time::Duration {
    let secs = std::env::var("AX_USAGE_DB_BUSY_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map_or(10, |secs| secs.clamp(1, 600));
    std::time::Duration::from_secs(secs)
}

pub(crate) async fn open_pool_at(path: &std::path::Path) -> Result<SqlitePool, AxError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    }

    let wait = usage_db_wait();
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(wait)
        .disable_statement_logging();

    let pool = SqlitePoolOptions::new()
        .max_connections(3)
        .acquire_timeout(wait)
        .after_connect(move |conn, _meta| {
            Box::pin(async move {
                sqlx::query(&format!("PRAGMA busy_timeout = {}", wait.as_millis()))
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect_with(options)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;

    // The schema has already run on this file: skip the DDL so every open
    // is one read instead of dozens of statements.
    let fingerprint = schema_fingerprint();
    let current: (i32,) = sqlx::query_as("PRAGMA user_version")
        .fetch_one(&pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(format!("usage schema: {e}"))))?;
    if current.0 == fingerprint {
        return Ok(pool);
    }

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

    for stmt in PRICING_SCHEMA
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        sqlx::query(stmt)
            .execute(&pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(format!("pricing schema: {e}"))))?;
    }

    for stmt in MIGRATION_ADD_COLUMNS {
        // "duplicate column name" is expected on already-migrated databases. Any
        // other failure (busy, read-only, disk full) leaves the column missing, so
        // it is returned and the schema version stays unstamped for a retry.
        if let Err(error) = sqlx::query(stmt).execute(&pool).await {
            if !error.to_string().contains("duplicate column name") {
                return Err(AxError::Database(DatabaseError::new(format!(
                    "usage migration `{stmt}`: {error}"
                ))));
            }
        }
    }

    sqlx::query(&format!("PRAGMA user_version = {fingerprint}"))
        .execute(&pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(format!("usage schema: {e}"))))?;

    Ok(pool)
}

/// Tests that point `AX_USAGE_DB` at a temp file must hold this for the whole test.
/// The variable is process-global, so separate per-module locks still race.
#[cfg(test)]
pub(crate) static USAGE_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
mod tests {
    use super::*;

    async fn index_exists(pool: &SqlitePool, name: &str) -> bool {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT name FROM sqlite_master WHERE type = 'index' AND name = ?")
                .bind(name)
                .fetch_optional(pool)
                .await
                .unwrap();
        row.is_some()
    }

    #[tokio::test]
    async fn a_database_at_the_current_schema_is_opened_without_rerunning_the_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.db");
        let pool = open_pool_at(&path).await.unwrap();
        assert!(index_exists(&pool, "idx_mcp_call_log_tool").await);
        // A marker the schema would recreate: if the second open reran the
        // DDL, the index would be back.
        sqlx::query("DROP INDEX idx_mcp_call_log_tool")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        let pool = open_pool_at(&path).await.unwrap();
        assert!(!index_exists(&pool, "idx_mcp_call_log_tool").await);
        pool.close().await;
    }

    #[tokio::test]
    async fn a_busy_database_fails_within_seconds_instead_of_minutes() {
        let dir = tempfile::tempdir().unwrap();
        let pool = open_pool_at(&dir.path().join("usage.db")).await.unwrap();
        let busy: (i64,) = sqlx::query_as("PRAGMA busy_timeout")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(busy.0 > 0 && busy.0 <= 10_000, "busy_timeout {} ms", busy.0);
        assert!(pool.options().get_acquire_timeout() <= std::time::Duration::from_secs(10));
        pool.close().await;
    }

    #[tokio::test]
    async fn open_pool_hands_out_one_shared_pool_per_path() {
        let _guard = USAGE_ENV_LOCK.lock().await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.db");
        std::env::set_var("AX_USAGE_DB", &path);
        let first = open_pool().await.unwrap();
        let second = open_pool().await.unwrap();
        // Closing one handle closes the other only when they are the same pool.
        first.close().await;
        let shared = second.is_closed();
        // A closed pool is replaced by a working one.
        let third = open_pool().await.unwrap();
        let works = sqlx::query("SELECT 1").execute(&third).await.is_ok();
        std::env::remove_var("AX_USAGE_DB");
        assert!(shared, "each open_pool call built its own pool");
        assert!(works);
    }

    #[tokio::test]
    async fn open_pool_reopens_a_removed_database_file() {
        let _guard = USAGE_ENV_LOCK.lock().await;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.db");
        std::env::set_var("AX_USAGE_DB", &path);
        let first = open_pool().await.unwrap();
        sqlx::query("INSERT INTO mcp_call_log(tool,response_chars,response_tokens_est,ok,created_at) VALUES ('old',1,1,1,1)")
            .execute(&first)
            .await
            .unwrap();
        drop(first);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        let second = open_pool().await.unwrap();
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mcp_call_log")
            .fetch_one(&second)
            .await
            .unwrap();
        std::env::remove_var("AX_USAGE_DB");
        assert_eq!(rows, 0);
        assert!(path.exists());
    }

    #[test]
    fn a_shared_pool_keeps_working_in_a_later_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.db");
        let run = |path: PathBuf| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    let _guard = USAGE_ENV_LOCK.lock().await;
                    std::env::set_var("AX_USAGE_DB", &path);
                    let pool = open_pool().await.unwrap();
                    sqlx::query("INSERT INTO mcp_call_log(tool,response_chars,response_tokens_est,ok,created_at) VALUES ('t',1,1,1,1)")
                        .execute(&pool)
                        .await
                        .unwrap();
                    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mcp_call_log")
                        .fetch_one(&pool)
                        .await
                        .unwrap();
                    std::env::remove_var("AX_USAGE_DB");
                    n
                })
        };
        assert_eq!(run(path.clone()), 1);
        assert_eq!(run(path), 2);
    }

    #[tokio::test]
    async fn a_database_at_an_older_schema_is_migrated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.db");
        let pool = open_pool_at(&path).await.unwrap();
        sqlx::query("DROP INDEX idx_mcp_call_log_tool")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("PRAGMA user_version = 1")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        let pool = open_pool_at(&path).await.unwrap();
        assert!(index_exists(&pool, "idx_mcp_call_log_tool").await);
        pool.close().await;
    }

    #[tokio::test]
    async fn a_failed_additive_migration_is_an_error_and_is_retried() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.db");
        // A name the schema expects as a table, held by a view: `CREATE TABLE IF
        // NOT EXISTS` passes, but adding a column fails with an error that is not
        // "duplicate column name".
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite://{}?mode=rwc", path.display()))
            .await
            .unwrap();
        sqlx::query("CREATE VIEW agent_session_log AS SELECT 1 AS id")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let opened = open_pool_at(&path).await;
        assert!(opened.is_err(), "a failed ALTER TABLE was ignored");

        // The schema version is not stamped, so a later open tries again.
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite://{}", path.display()))
            .await
            .unwrap();
        let version: (i32,) = sqlx::query_as("PRAGMA user_version")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_ne!(version.0, schema_fingerprint());
        pool.close().await;
    }

    #[tokio::test]
    async fn reopening_a_migrated_database_ignores_existing_columns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.db");
        let pool = open_pool_at(&path).await.unwrap();
        sqlx::query("PRAGMA user_version = 1")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
        // Every additive column already exists: the rerun must still succeed.
        let pool = open_pool_at(&path).await.unwrap();
        pool.close().await;
    }
}
