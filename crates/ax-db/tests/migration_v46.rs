use ax_db::migrations::CURRENT_SCHEMA_VERSION;
use ax_db::Database;

async fn rank_total_columns(db: &Database) -> Vec<String> {
    sqlx::query_scalar("SELECT name FROM pragma_table_info('neural_rank_total') ORDER BY cid")
        .fetch_all(db.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn schema_46_adds_neural_rank_totals() {
    assert_eq!(CURRENT_SCHEMA_VERSION, 46);
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("ax.db"))
        .await
        .expect("temp database");
    assert_eq!(
        rank_total_columns(&db).await,
        vec![
            "project_id".to_string(),
            "preflights".to_string(),
            "duplicate_blocks".to_string(),
            "duplicate_tokens".to_string(),
            "rank_micros".to_string(),
            "updated_at".to_string(),
        ]
    );
}

#[tokio::test]
async fn a_version_45_database_gains_rank_totals_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ax.db");
    {
        let db = Database::open(&path).await.expect("temp database");
        sqlx::query("DROP TABLE neural_rank_total")
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query("DELETE FROM schema_versions WHERE version = 46")
            .execute(db.pool())
            .await
            .unwrap();
        db.pool().close().await;
    }
    let db = Database::open(&path).await.expect("reopened database");
    assert_eq!(rank_total_columns(&db).await.len(), 6);
    let version: i32 = sqlx::query_scalar("SELECT MAX(version) FROM schema_versions")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(version, 46);
}
