//! One shared connection pool for `~/.ax/global.db`.
//!
//! Opening the database and running its schema statements is costly and
//! competes for the SQLite write lock with the CLI and MCP processes, so the
//! Command Center opens it once and hands out clones of that pool. A pool is
//! opened again only when its file disappears or the pool was closed. Pools
//! are kept per path because `AX_GLOBAL_DB` can point elsewhere.
//!
//! Callers must not close the returned pool: other requests share it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;
use tokio::sync::Mutex;

struct Entry {
    pool: SqlitePool,
    opens: usize,
}

static SHARED: Mutex<Option<HashMap<PathBuf, Entry>>> = Mutex::const_new(None);

/// The shared pool for the configured global database.
pub(crate) async fn global_pool() -> Result<SqlitePool, String> {
    let path = ax_global_db::global_db_path().map_err(|e| e.to_string())?;
    pool_at(&path).await
}

/// The shared pool for the global database at `path`.
pub(crate) async fn pool_at(path: &Path) -> Result<SqlitePool, String> {
    let mut guard = SHARED.lock().await;
    let pools = guard.get_or_insert_with(HashMap::new);
    let opens = match pools.get(path) {
        Some(e) if !e.pool.is_closed() && path.is_file() => return Ok(e.pool.clone()),
        Some(e) => e.opens,
        None => 0,
    };
    let pool = ax_global_db::open_and_init(path)
        .await
        .map_err(|e| e.to_string())?;
    pools.insert(
        path.to_path_buf(),
        Entry {
            pool: pool.clone(),
            opens: opens + 1,
        },
    );
    Ok(pool)
}

/// How many times the pool for `path` was opened (and its schema run).
#[cfg(test)]
pub(crate) async fn opens(path: &Path) -> usize {
    SHARED
        .lock()
        .await
        .as_ref()
        .and_then(|m| m.get(path))
        .map_or(0, |e| e.opens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn opens_the_database_once_per_path() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.db");
        let b = dir.path().join("b.db");
        for _ in 0..5 {
            let pool = pool_at(&a).await.unwrap();
            sqlx::query("SELECT 1").execute(&pool).await.unwrap();
        }
        assert_eq!(opens(&a).await, 1, "same path must reuse the pool");

        pool_at(&b).await.unwrap();
        assert_eq!(opens(&b).await, 1);
        assert_eq!(opens(&a).await, 1, "another path leaves the first alone");

        // A closed pool is replaced instead of handed out.
        pool_at(&b).await.unwrap().close().await;
        let fresh = pool_at(&b).await.unwrap();
        assert!(!fresh.is_closed());
        assert_eq!(opens(&b).await, 2);
    }
}
