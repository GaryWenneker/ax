use ax_db::integrity::{sql_error_is_incident, writers_blocked};
use ax_db::queries::QueryBuilder;
use ax_db::Database;

#[tokio::test]
async fn a_query_after_open_records_a_malformed_error() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ax.db");
    let db = Database::open(&path).await.unwrap();
    let rootpage: i64 =
        sqlx::query_scalar("SELECT rootpage FROM sqlite_master WHERE name = 'nodes'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(db.pool())
        .await
        .unwrap();
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(db.pool())
        .await
        .unwrap();
    db.close().await;
    assert!(
        rootpage > 1,
        "page 1 is the schema catalog and must stay readable"
    );
    let mut bytes = std::fs::read(&path).unwrap();
    let offset = ((rootpage - 1) * page_size) as usize;
    assert!(offset + 8 < bytes.len(), "nodes page is outside the file");
    bytes[offset] = 0x03;
    std::fs::write(&path, &bytes).unwrap();
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));

    let db = Database::open(&path)
        .await
        .expect("open must survive a damaged nodes page");

    let err = QueryBuilder::new(db.pool().clone())
        .get_all_nodes()
        .await
        .expect_err("a corrupt nodes table must fail the query");
    assert!(
        sql_error_is_incident(&err.to_string()),
        "the query error has to be an incident, got {err}"
    );
    assert!(writers_blocked(&path), "{err}");
    db.close().await;
}

#[tokio::test]
async fn a_missing_table_query_does_not_record_an_incident() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ax.db");
    let db = Database::open(&path).await.unwrap();
    sqlx::query("DROP TABLE nodes")
        .execute(db.pool())
        .await
        .unwrap();

    let err = QueryBuilder::new(db.pool().clone())
        .get_all_nodes()
        .await
        .expect_err("a dropped table must fail the query");
    assert!(!sql_error_is_incident(&err.to_string()), "{err}");
    assert!(!writers_blocked(&path), "{err}");
    db.close().await;
}
