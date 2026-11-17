use std::time::Duration;

use ax_core::Ax;
use ax_db::builds::{register_build, BuildStamp};

#[tokio::test]
async fn background_watch_returns_when_a_newer_build_is_registered() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let ax = Ax::init(&root).await.unwrap();
    register_build(
        ax.db_pool(),
        "mcp",
        42,
        &BuildStamp {
            version: "future".into(),
            mtime_ms: 9_000_000_000_000,
            len: 1,
        },
    )
    .await
    .unwrap();
    drop(ax);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/app.ts"),
        "export function greet(name: string) { return `hi ${name}`; }\n",
    )
    .unwrap();

    let task = Ax::spawn_background_watch(root.clone());
    tokio::time::timeout(Duration::from_secs(8), task)
        .await
        .expect("an older build must return instead of watching")
        .unwrap();

    let db = ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let indexed: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM file_contents WHERE path LIKE '%app.ts'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    db.close().await;
    assert_eq!(
        indexed, 0,
        "a newer build is registered, so sync must not index"
    );
}
