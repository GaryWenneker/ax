//! Setup shared by the ax-web integration tests.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tempfile::TempDir;

static HOME: OnceLock<TempDir> = OnceLock::new();

/// Points `HOME`, `AX_HOME_DIR` and `AX_GLOBAL_DB` at one temporary folder
/// for the whole test binary, so no test reads or writes the machine's
/// `~/.ax` or its `global.db`. `AX_GLOBAL_DB` wins over `HOME` when the
/// global database path is resolved, so it must be set too.
pub fn isolate_home() -> &'static Path {
    HOME.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        isolate_home_at(dir.path());
        dir
    })
    .path()
}

/// [`isolate_home`] for a folder the caller owns.
pub fn isolate_home_at(dir: &Path) {
    std::env::set_var("HOME", dir);
    std::env::set_var("AX_HOME_DIR", dir);
    std::env::set_var("AX_GLOBAL_DB", global_db(dir));
}

/// The global database path inside an isolated home.
pub fn global_db(home: &Path) -> PathBuf {
    home.join("global.db")
}

/// Stores to seed a test project with, opened without a [`ax_web::WebHub`].
/// A hub starts background tasks (index catch-up, memory seed, dedup) on the
/// same `ax.db`; seeding through one raced those tasks.
pub struct Seed {
    pub db: ax_db::Database,
    pub policy: ax_policy::PolicyStore,
}

impl Seed {
    pub fn graph_pool(&self) -> &sqlx::SqlitePool {
        self.db.pool()
    }

    pub async fn close(self) {
        self.policy.pool().close().await;
        self.db.pool().close().await;
    }
}

/// Opens `<root>/.ax/ax.db` for seeding. It also runs `ANALYZE` once, as
/// `ax init` does on a real project: the first `ANALYZE` creates
/// `sqlite_stat1`, a schema change that makes a concurrent statement on
/// `memories_fts` fail with SQLITE_SCHEMA ("vtable constructor failed").
/// Without it the hub's first index catch-up made that change while a test
/// was running.
pub async fn seed_stores(root: &Path) -> Seed {
    let ax_dir = root.join(".ax");
    std::fs::create_dir_all(&ax_dir).unwrap();
    let db_path = ax_dir.join("ax.db");
    let db = ax_db::Database::open(&db_path).await.unwrap();
    sqlx::query("ANALYZE").execute(db.pool()).await.unwrap();
    ax_policy::ensure_scaffold(&ax_dir).unwrap();
    let pool = ax_policy::open_rw_pool(&db_path).await.unwrap();
    let policy = ax_policy::PolicyStore::new(pool, root.to_path_buf());
    Seed { db, policy }
}
