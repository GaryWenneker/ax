//! SQLite storage layer for ax.

pub mod migrations;
pub mod queries;
pub mod schema;

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous};
use sqlx::ConnectOptions;

use ax_utils::errors::{AxError, DatabaseError};

pub const DB_FILENAME: &str = "ax.db";

/// Database connection pool with ax schema.
pub struct Database {
    pool: SqlitePool,
    path: std::path::PathBuf,
    opened_inode: Option<String>,
}

impl Database {
    pub async fn open(path: &Path) -> Result<Self, AxError> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .disable_statement_logging();

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
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
        sqlx::query("ANALYZE")
            .execute(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        Ok(())
    }

    pub async fn get_journal_mode(&self) -> Result<String, AxError> {
        let row: (String,) = sqlx::query_as("PRAGMA journal_mode")
            .fetch_one(&self.pool)
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
