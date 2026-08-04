//! SQLite storage layer for ax.

pub mod migrations;
pub mod queries;
pub mod schema;

use std::path::Path;
use std::time::Duration;

use sqlx::ConnectOptions;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};

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

/// Shared `ax.db` connect options — WAL + busy timeout for multi-process Takumi/CLI use.
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
    msg.contains("database is locked")
        || msg.contains("(code: 5)")
        || msg.contains("SQLITE_BUSY")
}

/// Retry an async SQLite operation while the database is busy.
pub async fn with_busy_retry<F, Fut, T>(mut op: F) -> Result<T, sqlx::Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, sqlx::Error>>,
{
    const MAX_ATTEMPTS: u32 = 12;
    for attempt in 0..MAX_ATTEMPTS {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) if is_sqlite_busy(&e) && attempt + 1 < MAX_ATTEMPTS => {
                let backoff = Duration::from_millis(40 * (attempt as u64 + 1).min(25));
                tokio::time::sleep(backoff).await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("loop exits via return")
}

/// Database connection pool with ax schema.
pub struct Database {
    pool: SqlitePool,
    path: std::path::PathBuf,
    opened_inode: Option<String>,
}

impl Database {
    pub async fn open(path: &Path) -> Result<Self, AxError> {
        // Drop orphaned writer markers before connecting — dead PIDs must not block WAL recovery.
        if let Some(ax_dir) = path.parent() {
            ax_utils::clear_stale_lock(&ax_dir.join("ax.lock"));
        }

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
                            Ok(())
                        })
                    })
                    .connect_with(opts)
                    .await
            }
        })
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;

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

    pub async fn run_maintenance(&self) -> Result<(), AxError> {
        with_busy_retry(|| async {
            sqlx::query("ANALYZE").execute(&self.pool).await?;
            // PASSIVE avoids blocking readers/writers; TRUNCATE can starve MCP under load.
            sqlx::query("PRAGMA wal_checkpoint(PASSIVE)")
                .execute(&self.pool)
                .await?;
            Ok(())
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
