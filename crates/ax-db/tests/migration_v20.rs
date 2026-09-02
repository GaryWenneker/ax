//! Schema v20 (`policy_revisions`) upgrade path.
//!
//!   cargo test -p ax-db --test migration_v20

use std::path::{Path, PathBuf};

use ax_db::Database;
use ax_db::migrations::{CURRENT_SCHEMA_VERSION, get_current_version};

fn scratch_db(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ax-migration-v20-{name}-{}-{}.db",
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

async fn rewind_to_v19(db: &Database) {
    sqlx::query("DELETE FROM schema_versions WHERE version >= 20")
        .execute(db.pool())
        .await
        .expect("rewind schema_versions");
}

#[tokio::test]
async fn v19_database_upgrades_to_v20_without_losing_rules() {
    let path = scratch_db("upgrade");

    {
        let db = Database::open(&path).await.expect("open fresh db");
        rewind_to_v19(&db).await;
        sqlx::query(
            "INSERT INTO policy_rules (
                id, level, always_apply, globs, triggers, tags, priority, body, source_path, content_hash, updated_at
             ) VALUES ('utf8-no-bom', 'CRITICAL', 1, '[]', '[]', '[\"conventions\"]', 100, 'body', 'x', 'h', 1)",
        )
        .execute(db.pool())
        .await
        .expect("insert pre-existing rule");

        let version = get_current_version(db.pool()).await.expect("version");
        assert_eq!(version, 19, "test fixture must start at v19");
    }

    {
        let db = Database::open(&path).await.expect("reopen and migrate");
        let version = get_current_version(db.pool()).await.expect("version");
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        assert_eq!(CURRENT_SCHEMA_VERSION, 20);

        let kept: Option<String> =
            sqlx::query_scalar("SELECT level FROM policy_rules WHERE id = 'utf8-no-bom'")
                .fetch_optional(db.pool())
                .await
                .expect("query rule");
        assert_eq!(kept.as_deref(), Some("CRITICAL"));

        sqlx::query(
            "INSERT INTO policy_revisions (kind, item_id, content_hash, body, source, created_at)
             VALUES ('rule', 'utf8-no-bom', 'abc', 'body', 'save', 1)",
        )
        .execute(db.pool())
        .await
        .expect("policy_revisions table must exist");
    }

    cleanup(&path);
}
