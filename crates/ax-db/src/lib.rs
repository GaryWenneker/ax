//! SQLite storage layer for ax.

pub mod builds;
pub mod graph_history;
pub mod hook_turns;
pub mod integrity;
pub mod migrations;
pub mod queries;
pub mod replication;
pub mod schema;
pub mod sync_strategy;
pub mod vec_ext;

use std::path::Path;
use std::time::Duration;

use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::ConnectOptions;

use ax_utils::errors::{AxError, DatabaseError};

pub const DB_FILENAME: &str = "ax.db";

/// Wait for concurrent ax processes (sync, web API, MCP) before failing with SQLITE_BUSY.
/// Override with `AX_DB_BUSY_TIMEOUT_SECS` (integer seconds, clamped 5..=600).
pub fn busy_timeout() -> Duration {
    let secs = std::env::var("AX_DB_BUSY_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(180)
        .clamp(5, 600);
    Duration::from_secs(secs)
}

/// Per-file cap for the source store (`file_contents`, schema v17).
///
/// Files above the cap store no text. A graph read then reports an explicit
/// "source not stored" marker instead of falling back to a disk read — a silent
/// fallback would make the graph-only guarantee unverifiable.
/// Override with `AX_SOURCE_STORE_MAX_BYTES` (bytes, clamped 64 KiB..=64 MiB).
pub fn source_store_cap_bytes() -> usize {
    const DEFAULT: usize = 1024 * 1024;
    const MIN: usize = 64 * 1024;
    const MAX: usize = 64 * 1024 * 1024;
    std::env::var("AX_SOURCE_STORE_MAX_BYTES")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(DEFAULT)
        .clamp(MIN, MAX)
}

/// Shared `ax.db` connect options — WAL + busy timeout for multi-process IDE/CLI use.
pub fn connect_options(path: &Path, create_if_missing: bool) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(create_if_missing)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(busy_timeout())
        .disable_statement_logging()
}

/// True when SQLite reports SQLITE_BUSY / database is locked (code 5).
pub fn is_sqlite_busy(err: &sqlx::Error) -> bool {
    let msg = err.to_string();
    // Code 17 (SQLITE_SCHEMA / "vtable constructor failed") is the one-shot
    // error SQLite returns after an FTS rebuild. The next statement on that
    // connection sees the new schema, so it is retried like a lock.
    msg.contains("database is locked")
        || msg.contains("(code: 5)")
        || msg.contains("SQLITE_BUSY")
        || msg.contains("(code: 17)")
        || msg.contains("vtable constructor failed")
}

/// Retry an async SQLite operation while the database is busy.
///
/// Every attempt can itself wait up to [`busy_timeout`] inside SQLite, so the
/// retries stop once that much time has passed in total. Twelve full waits
/// used to add up to about 36 minutes, which looked like a hang.
pub async fn with_busy_retry<F, Fut, T>(op: F) -> Result<T, sqlx::Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, sqlx::Error>>,
{
    with_busy_retry_for(busy_timeout(), op).await
}

/// [`with_busy_retry`] with an explicit total time budget.
pub async fn with_busy_retry_for<F, Fut, T>(budget: Duration, mut op: F) -> Result<T, sqlx::Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, sqlx::Error>>,
{
    const MAX_ATTEMPTS: u32 = 12;
    let started = std::time::Instant::now();
    for attempt in 0..MAX_ATTEMPTS {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e)
                if is_sqlite_busy(&e)
                    && attempt + 1 < MAX_ATTEMPTS
                    && started.elapsed() < budget =>
            {
                let backoff = Duration::from_millis(40 * (attempt as u64 + 1).min(25));
                tokio::time::sleep(backoff).await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("loop exits via return")
}

/// `journal_size_limit` for `ax.db`: the WAL file is cut back to this size
/// after a checkpoint resets it.
pub const WAL_SIZE_LIMIT_BYTES: i64 = 64 * 1024 * 1024;

/// Checkpoint the WAL and truncate it to zero bytes, without waiting.
///
/// Runs with `busy_timeout = 0` on its own connection: when readers or a
/// writer are active, SQLite falls back to a PASSIVE checkpoint instead of
/// waiting for them, so MCP readers are never stalled. A plain TRUNCATE would
/// wait up to the full busy timeout.
pub async fn checkpoint_truncate_nowait(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut conn = pool.acquire().await?;
    sqlx::query("PRAGMA busy_timeout = 0")
        .execute(&mut *conn)
        .await?;
    let checkpoint = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut *conn)
        .await;
    let restore = sqlx::query(&format!(
        "PRAGMA busy_timeout = {}",
        busy_timeout().as_millis()
    ))
    .execute(&mut *conn)
    .await;
    match checkpoint {
        Ok(_) => restore.map(|_| ()),
        // Busy only means the checkpoint stopped early (PASSIVE result).
        Err(e) if is_sqlite_busy(&e) => restore.map(|_| ()),
        Err(e) => Err(e),
    }
}

/// Database connection pool with ax schema.
pub struct Database {
    pool: SqlitePool,
    path: std::path::PathBuf,
    opened_inode: Option<String>,
}

impl Database {
    pub async fn open(path: &Path) -> Result<Self, AxError> {
        crate::vec_ext::install_sqlite_vec();

        let options = connect_options(path, true);
        let timeout = busy_timeout();
        let timeout_ms = timeout.as_millis() as i64;

        let pool = with_busy_retry(|| {
            let opts = options.clone();
            async move {
                SqlitePoolOptions::new()
                    .max_connections(3)
                    .acquire_timeout(timeout)
                    .after_connect(move |conn, _meta| {
                        Box::pin(async move {
                            // Belt-and-suspenders: connect options set busy_timeout, but
                            // re-apply on every pooled connection (Windows / sqlx quirks).
                            sqlx::query(&format!("PRAGMA busy_timeout = {timeout_ms}"))
                                .execute(&mut *conn)
                                .await?;
                            sqlx::query("PRAGMA wal_autocheckpoint = 1000")
                                .execute(&mut *conn)
                                .await?;
                            // Shrink the -wal file back to this size after a
                            // checkpoint; a full index otherwise leaves a WAL
                            // larger than the database itself.
                            sqlx::query(&format!(
                                "PRAGMA journal_size_limit = {WAL_SIZE_LIMIT_BYTES}"
                            ))
                            .execute(&mut *conn)
                            .await?;
                            Ok(())
                        })
                    })
                    .connect_with(opts)
                    .await
            }
        })
        .await
        .map_err(|e| {
            let message = e.to_string();
            let _ = crate::integrity::note_sql_incident(path, &message);
            AxError::Database(DatabaseError::new(message))
        })?;

        let db = Self {
            pool,
            path: path.to_path_buf(),
            opened_inode: stat_inode(path),
        };
        db.initialize().await?;
        Ok(db)
    }

    async fn initialize(&self) -> Result<(), AxError> {
        schema::apply_initial_schema(&self.pool).await?;
        let current = migrations::get_current_version(&self.pool).await?;
        migrations::run_migrations(&self.pool, current).await?;
        Ok(())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// CG: `isReplacedOnDisk` — POSIX inode changed at same path (#925).
    pub fn is_replaced_on_disk(&self) -> bool {
        if self.opened_inode.is_none() {
            return false;
        }
        let current = stat_inode(&self.path);
        current.is_some() && current != self.opened_inode
    }

    /// Maintenance for a scoped (incremental) run: `PRAGMA optimize` only
    /// re-analyzes tables whose statistics drifted, instead of a full
    /// `ANALYZE` of the whole database, then checkpoints the WAL.
    pub async fn run_light_maintenance(&self) -> Result<(), AxError> {
        with_busy_retry(|| async {
            sqlx::query("PRAGMA optimize").execute(&self.pool).await?;
            checkpoint_truncate_nowait(&self.pool).await
        })
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))
    }

    /// Full maintenance for a project-wide run: `ANALYZE` plus a WAL checkpoint.
    pub async fn run_maintenance(&self) -> Result<(), AxError> {
        with_busy_retry(|| async {
            sqlx::query("ANALYZE").execute(&self.pool).await?;
            checkpoint_truncate_nowait(&self.pool).await
        })
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))
    }

    pub async fn get_journal_mode(&self) -> Result<String, AxError> {
        let row: (String,) = with_busy_retry(|| async {
            sqlx::query_as("PRAGMA journal_mode")
                .fetch_one(&self.pool)
                .await
        })
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(row.0)
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }
}

fn stat_inode(path: &Path) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path)
            .ok()
            .map(|m| format!("{}:{}", m.dev(), m.ino()))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn busy_retry_stops_at_its_time_budget() {
        let started = std::time::Instant::now();
        let mut calls = 0u32;
        let res: Result<(), sqlx::Error> = with_busy_retry_for(Duration::from_millis(100), || {
            calls += 1;
            async {
                tokio::time::sleep(Duration::from_millis(60)).await;
                Err(sqlx::Error::Protocol("database is locked".into()))
            }
        })
        .await;
        assert!(res.is_err());
        assert!(calls < 12, "kept retrying past the budget: {calls} calls");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn busy_timeout_default_is_generous() {
        // Ensure unset env yields the hardened default (not the old 30s).
        std::env::remove_var("AX_DB_BUSY_TIMEOUT_SECS");
        assert_eq!(busy_timeout(), Duration::from_secs(180));
    }

    #[test]
    fn is_sqlite_busy_detects_code_5() {
        let err = sqlx::Error::Protocol(
            "error returned from database: (code: 5) database is locked".into(),
        );
        assert!(is_sqlite_busy(&err));
    }
}
