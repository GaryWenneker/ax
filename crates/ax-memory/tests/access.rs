use ax_memory::{access_times, record_access};

#[tokio::test]
async fn recording_access_stores_times_in_order() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();

    assert!(!record_access(db.pool(), "", "recall", 10).await.unwrap());
    assert!(record_access(db.pool(), "m1", "recall", 10).await.unwrap());
    assert!(record_access(db.pool(), "m1", "inject", 20).await.unwrap());
    let times = access_times(db.pool(), "m1").await.unwrap();
    db.close().await;
    assert_eq!(times, vec![10, 20]);
}
