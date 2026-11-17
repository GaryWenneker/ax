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
