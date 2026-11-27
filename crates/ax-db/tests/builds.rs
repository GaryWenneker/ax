use ax_db::builds::{
    background_writers_allowed, mcp_stamps, register_build, stale_mcp_lines, writers_allowed,
    BuildStamp,
};
use ax_db::Database;

fn stamp(version: &str, mtime_ms: u64) -> BuildStamp {
    BuildStamp {
        version: version.to_string(),
        mtime_ms,
        len: 10,
    }
}

#[tokio::test]
async fn an_older_build_skips_writers_and_doctor_names_the_mcp_pid() {
    let older = stamp("9.0.2", 1);
    let newer = stamp("9.0.3", 2);
    let same = stamp("9.0.3", 2);
    assert!(!background_writers_allowed(
        &older,
        std::slice::from_ref(&newer)
    ));
    assert!(background_writers_allowed(
        &newer,
        std::slice::from_ref(&older)
    ));
    assert!(background_writers_allowed(
        &same,
        std::slice::from_ref(&newer)
    ));
    assert!(background_writers_allowed(
        &BuildStamp {
            version: "0".into(),
            mtime_ms: 0,
            len: 0
        },
        std::slice::from_ref(&newer)
    ));
    assert_eq!(
        stale_mcp_lines(
            &newer,
            &[
                (42, older.clone()),
                (99, stamp("9.0.4", 3)),
                (7, stamp("9.0.1", 1)),
            ]
        ),
        vec![
            "MCP pid 42 is an older binary. Restart that MCP server.".to_string(),
            "MCP pid 7 is an older binary. Restart that MCP server.".to_string(),
        ]
    );

    let root = std::env::temp_dir().join(format!(
        "ax-builds-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let db = Database::open(&root.join("ax.db")).await.unwrap();
    register_build(db.pool(), "mcp", 42, &older).await.unwrap();
    register_build(db.pool(), "web", 8, &older).await.unwrap();
    register_build(db.pool(), "mcp", 42, &newer).await.unwrap();
    assert!(writers_allowed(db.pool(), &newer).await.unwrap());
    let rows = mcp_stamps(db.pool()).await.unwrap();
    assert_eq!(rows, vec![(42, newer)]);
    db.close().await;
    let _ = std::fs::remove_dir_all(root);
}

fn temp_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "ax-builds-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// The pid of a process that has already exited.
fn dead_pid() -> i64 {
    let mut child = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", "exit"])
            .spawn()
            .unwrap()
    } else {
        std::process::Command::new("true").spawn().unwrap()
    };
    let pid = child.id() as i64;
    child.wait().unwrap();
    pid
}

#[tokio::test]
async fn a_row_left_by_an_exited_newer_build_does_not_block_writers() {
    let root = temp_root("dead");
    let db = Database::open(&root.join("ax.db")).await.unwrap();
    let older = stamp("9.0.2", 1);
    let newer = stamp("9.0.3", 2);
    let pid = dead_pid();
    register_build(db.pool(), "mcp", pid, &newer).await.unwrap();
    assert!(writers_allowed(db.pool(), &older).await.unwrap());
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ax_builds WHERE pid = ?")
        .bind(pid)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(left, 0, "the exited build's row is removed");
    db.close().await;
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn a_running_newer_build_still_blocks_writers() {
    let root = temp_root("live");
    let db = Database::open(&root.join("ax.db")).await.unwrap();
    let older = stamp("9.0.2", 1);
    let newer = stamp("9.0.3", 2);
    register_build(db.pool(), "mcp", std::process::id() as i64, &newer)
        .await
        .unwrap();
    assert!(!writers_allowed(db.pool(), &older).await.unwrap());
    db.close().await;
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_version_orders_builds_before_the_file_time() {
    // A newer release installed earlier than an old dev binary was built.
    let release = stamp("9.1.0", 1);
    let old_dev = stamp("9.0.3", 2);
    assert!(background_writers_allowed(
        &release,
        std::slice::from_ref(&old_dev)
    ));
    assert!(!background_writers_allowed(
        &old_dev,
        std::slice::from_ref(&release)
    ));
    assert!(background_writers_allowed(
        &stamp("9.10.0", 1),
        &[stamp("9.9.0", 5)]
    ));
    assert_eq!(
        stale_mcp_lines(&release, &[(5, old_dev.clone())]),
        vec!["MCP pid 5 is an older binary. Restart that MCP server.".to_string()]
    );
    assert!(stale_mcp_lines(&old_dev, &[(6, release)]).is_empty());
}
