//! The Ax facade cleans global duplicates after sync and after a policy index.

use std::path::Path;

use ax_core::Ax;
use ax_extraction::orchestrator::IndexOptions;
use ax_global_db::policy::{self as gpolicy, PolicyKind};
use sqlx::SqlitePool;

fn quiet_opts() -> IndexOptions {
    IndexOptions {
        quiet: true,
        ..Default::default()
    }
}

fn longer_skill_file(root: &Path, name: &str) {
    let dir = root.join(".agents/skills").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let body = format!(
        "---\nname: {name}\ndescription: probe\n---\n\n# {name}\n\nLONG-MARKER: the project copy is more extensive than the global one.\n"
    );
    std::fs::write(dir.join("SKILL.md"), body).unwrap();
}

async fn in_project(ax: &Ax, name: &str) -> bool {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM policy_skills WHERE name = ?")
        .bind(name)
        .fetch_one(ax.db_pool())
        .await
        .unwrap();
    n > 0
}

async fn global_body(global: &Path, name: &str) -> String {
    let pool = ax_global_db::open_pool(global, false).await.unwrap();
    let payload: String =
        sqlx::query_scalar("SELECT payload FROM global_policy_skills WHERE item_id = ?")
            .bind(name)
            .fetch_one(&pool)
            .await
            .unwrap();
    pool.close().await;
    serde_json::from_str::<serde_json::Value>(&payload).unwrap()["body"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

async fn short_global(global: &SqlitePool, machine: i64, name: &str) {
    let payload = serde_json::json!({ "name": name, "body": "short" });
    gpolicy::upsert_policy_item(global, machine, PolicyKind::Skills, name, &payload)
        .await
        .unwrap();
}

#[tokio::test]
async fn sync_and_policy_index_promote_and_remove_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let global_path = dir.path().join("global.db");
    let global = ax_global_db::open_and_init(&global_path).await.unwrap();
    let machine = gpolicy::ensure_project(&global, &dir.path().join("machine"))
        .await
        .unwrap();
    short_global(&global, machine, "probe-sync").await;
    short_global(&global, machine, "probe-index").await;
    global.close().await;
    std::env::set_var(ax_global_db::AX_GLOBAL_DB_ENV, &global_path);

    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/app.ts"), "export const a = 1;\n").unwrap();
    let mut ax = Ax::init(&root).await.unwrap();
    ax.index_all(quiet_opts(), None).await.unwrap();

    longer_skill_file(&root, "probe-sync");
    std::fs::write(root.join("src/app.ts"), "export const a = 2;\n").unwrap();
    ax.sync(quiet_opts(), None).await.unwrap();
    assert!(
        !in_project(&ax, "probe-sync").await,
        "sync leaves the duplicate"
    );
    assert!(global_body(&global_path, "probe-sync")
        .await
        .contains("LONG-MARKER"));

    longer_skill_file(&root, "probe-index");
    ax.index_policy(false).await.unwrap();
    assert!(
        !in_project(&ax, "probe-index").await,
        "policy index leaves the duplicate"
    );
    assert!(global_body(&global_path, "probe-index")
        .await
        .contains("LONG-MARKER"));

    ax.destroy().await.unwrap();
}
