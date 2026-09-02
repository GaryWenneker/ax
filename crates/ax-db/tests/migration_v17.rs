//! Schema v17 (`file_contents` source store) upgrade path.
//!
//! The risk this guards: an existing v16 database must gain the source store
//! without losing data and without the migration erroring out. Run:
//!   cargo test -p ax-db --test migration_v17

use std::path::{Path, PathBuf};

use ax_db::Database;
use ax_db::migrations::{CURRENT_SCHEMA_VERSION, get_current_version};

fn scratch_db(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ax-migration-v17-{name}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&p);
    p
}

fn cleanup(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut p = path.to_path_buf();
        if !suffix.is_empty() {
            p.set_file_name(format!(
                "{}{suffix}",
                path.file_name().unwrap().to_string_lossy()
            ));
        }
        let _ = std::fs::remove_file(&p);
    }
}

/// Rewind a fully-migrated database to a v16 shape: drop the v17 table and the
/// version rows that claim it exists.
async fn rewind_to_v16(db: &Database) {
    sqlx::query("DROP TABLE IF EXISTS file_contents")
        .execute(db.pool())
        .await
        .expect("drop file_contents");
    sqlx::query("DELETE FROM schema_versions WHERE version >= 17")
        .execute(db.pool())
        .await
        .expect("rewind schema_versions");
}

#[tokio::test]
async fn v16_database_upgrades_to_v17_without_losing_data() {
    let path = scratch_db("upgrade");

    // Build a v16-shaped database holding a real row.
    {
        let db = Database::open(&path).await.expect("open fresh db");
        rewind_to_v16(&db).await;
        sqlx::query(
            "INSERT INTO files (path, content_hash, language, size, modified_at, indexed_at)
             VALUES ('src/keep.rs', 'hash-abc', 'rust', 42, 1, 1)",
        )
        .execute(db.pool())
        .await
        .expect("insert pre-existing file row");

        let version = get_current_version(db.pool()).await.expect("version");
        assert_eq!(version, 16, "test fixture must start at v16");

        let has_table: Option<String> =
            sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' AND name='file_contents'")
                .fetch_optional(db.pool())
                .await
                .expect("query sqlite_master");
        assert!(has_table.is_none(), "v16 fixture must not have file_contents");
    }

    // Reopening runs apply_initial_schema + run_migrations: the real upgrade path.
    {
        let db = Database::open(&path).await.expect("reopen and migrate");

        let version = get_current_version(db.pool()).await.expect("version");
        assert_eq!(
            version, CURRENT_SCHEMA_VERSION,
            "upgrade must land on the current schema version"
        );
        assert_eq!(CURRENT_SCHEMA_VERSION, 20);

        let has_table: Option<String> =
            sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' AND name='file_contents'")
                .fetch_optional(db.pool())
                .await
                .expect("query sqlite_master");
        assert!(has_table.is_some(), "v17 must create file_contents");

        // Pre-existing data survives.
        let kept: Option<String> =
            sqlx::query_scalar("SELECT content_hash FROM files WHERE path = 'src/keep.rs'")
                .fetch_optional(db.pool())
                .await
                .expect("query files");
        assert_eq!(kept.as_deref(), Some("hash-abc"), "v16 data must survive");

        // The new table is usable and empty (backfill happens on index/sync).
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_contents")
            .fetch_one(db.pool())
            .await
            .expect("count file_contents");
        assert_eq!(rows, 0);
    }

    cleanup(&path);
}

/// Opening an already-current database repeatedly must not error.
#[tokio::test]
async fn migration_is_idempotent_across_reopens() {
    let path = scratch_db("idempotent");

    for _ in 0..3 {
        let db = Database::open(&path).await.expect("reopen");
        let version = get_current_version(db.pool()).await.expect("version");
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }

    cleanup(&path);
}

/// Mid-upgrade crash: v17's DDL landed but the version row was never recorded.
/// The next open must recover — re-running the DDL is tolerated as
/// "already exists" — and finish on v17 rather than failing forever.
#[tokio::test]
async fn interrupted_v17_upgrade_recovers_on_next_open() {
    let path = scratch_db("interrupted");

    {
        let db = Database::open(&path).await.expect("open fresh db");
        // Table exists (DDL committed) but the bookkeeping row does not.
        sqlx::query("DELETE FROM schema_versions WHERE version >= 17")
            .execute(db.pool())
            .await
            .expect("simulate crash before record_migration");
        assert_eq!(get_current_version(db.pool()).await.expect("version"), 16);
    }

    {
        let db = Database::open(&path).await.expect("recovery open must succeed");
        assert_eq!(
            get_current_version(db.pool()).await.expect("version"),
            CURRENT_SCHEMA_VERSION,
            "interrupted upgrade must complete on the next open"
        );
        let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_contents")
            .fetch_one(db.pool())
            .await
            .expect("file_contents must be queryable after recovery");
        assert_eq!(rows, 0);
    }

    cleanup(&path);
}

/// The v17 table must enforce one row per path so a re-index replaces rather
/// than duplicates stored source.
#[tokio::test]
async fn file_contents_path_is_unique() {
    let path = scratch_db("unique");
    let db = Database::open(&path).await.expect("open");

    sqlx::query(
        "INSERT INTO file_contents (path, content_hash, content, byte_len, updated_at)
         VALUES ('a.rs', 'h1', 'one', 3, 1)",
    )
    .execute(db.pool())
    .await
    .expect("first insert");

    let dup = sqlx::query(
        "INSERT INTO file_contents (path, content_hash, content, byte_len, updated_at)
         VALUES ('a.rs', 'h2', 'two', 3, 2)",
    )
    .execute(db.pool())
    .await;
    assert!(dup.is_err(), "path must be a primary key");

    drop(db);
    cleanup(&path);
}
