use std::path::PathBuf;

use ax_db::migrations::{get_current_version, CURRENT_SCHEMA_VERSION};
use ax_db::Database;

fn scratch_db(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ax-migration-v23-{name}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&path);
    path
}

fn cleanup(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

async fn rewind_to_v22(db: &Database) {
    sqlx::query("DELETE FROM schema_versions WHERE version >= 23")
        .execute(db.pool())
        .await
        .expect("rewind schema_versions");
}

#[tokio::test]
async fn v22_database_gains_agent_economics_tables() {
    let path = scratch_db("upgrade");
    {
        let db = Database::open(&path).await.expect("open fresh db");
        rewind_to_v22(&db).await;
        let version = get_current_version(db.pool()).await.expect("version");
        assert_eq!(version, 22, "test fixture must start at v22");
    }
    {
        let db = Database::open(&path).await.expect("reopen and migrate");
        let version = get_current_version(db.pool()).await.expect("version");
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
        assert_eq!(CURRENT_SCHEMA_VERSION, 23);
        sqlx::query(
            "INSERT INTO agent_sessions (id, project_id, created_at, updated_at, cache_namespace)
             VALUES ('s', 'p', 1, 1, 'p:s')",
        )
        .execute(db.pool())
        .await
        .expect("agent_sessions table must exist");
        sqlx::query(
            "INSERT INTO agent_tool_calls (id, session_id, turn_id, tool_name, input_hash, output_tokens, duration_ms, cache_hit, repeated)
             VALUES ('c', 's', 't', 'rg', 'hash', 10, 1, 0, 0)",
        )
        .execute(db.pool())
        .await
        .expect("agent_tool_calls table must exist");
    }
    cleanup(&path);
}
