//! Schema v18 (`policy_skills.skill_group`) upgrade path.
//!
//!   cargo test -p ax-db --test migration_v18

use std::path::{Path, PathBuf};

use ax_db::Database;
use ax_db::migrations::{CURRENT_SCHEMA_VERSION, get_current_version};

fn scratch_db(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ax-migration-v18-{name}-{}-{}.db",
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

async fn rewind_to_v17(db: &Database) {
    sqlx::query("DELETE FROM schema_versions WHERE version >= 18")
        .execute(db.pool())
        .await
        .expect("rewind schema_versions");
}

#[tokio::test]
async fn v17_database_upgrades_to_v18_without_losing_skills() {
    let path = scratch_db("upgrade");

    {
        let db = Database::open(&path).await.expect("open fresh db");
        rewind_to_v17(&db).await;
        sqlx::query(
            "INSERT INTO policy_skills (
                name, description, triggers, tags, priority, body, source_path, content_hash, updated_at
             ) VALUES ('startup', 'preflight', '[]', '[\"preflight\"]', 100, 'body', 'x', 'h', 1)",
        )
        .execute(db.pool())
        .await
        .expect("insert pre-existing skill");

        let version = get_current_version(db.pool()).await.expect("version");
        assert_eq!(version, 17, "test fixture must start at v17");
    }

    {
        let db = Database::open(&path).await.expect("reopen and migrate");
        let version = get_current_version(db.pool()).await.expect("version");
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        assert_eq!(CURRENT_SCHEMA_VERSION, 19);

        let kept: Option<String> =
            sqlx::query_scalar("SELECT description FROM policy_skills WHERE name = 'startup'")
                .fetch_optional(db.pool())
                .await
                .expect("query skill");
        assert_eq!(kept.as_deref(), Some("preflight"));

        sqlx::query("UPDATE policy_skills SET skill_group = 'session-protocol' WHERE name = 'startup'")
            .execute(db.pool())
            .await
            .expect("skill_group column must exist");
    }

    cleanup(&path);
}
