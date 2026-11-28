use ax_db::migrations::{get_current_version, run_migrations, CURRENT_SCHEMA_VERSION};
use ax_db::Database;

/// Two processes that both read the old version before either migrates: the
/// second one runs with a stale `from_version` and must skip what the first
/// already applied instead of failing on the schema_versions key.
#[tokio::test]
async fn a_second_opener_with_a_stale_version_skips_applied_migrations() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("ax.db")).await.unwrap();
    sqlx::query("DELETE FROM schema_versions WHERE version >= 40")
        .execute(db.pool())
        .await
        .unwrap();
    let stale = get_current_version(db.pool()).await.unwrap();
    assert_eq!(stale, 39);
    run_migrations(db.pool(), stale).await.unwrap();
    run_migrations(db.pool(), stale)
        .await
        .expect("the second opener must not fail on an applied migration");
    assert_eq!(
        get_current_version(db.pool()).await.unwrap(),
        CURRENT_SCHEMA_VERSION
    );
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM schema_versions WHERE version >= 40")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(i64::from(CURRENT_SCHEMA_VERSION - 39), rows);
    db.close().await;
}

/// Several opens of one file at the same moment all succeed.
#[tokio::test]
async fn concurrent_opens_of_an_old_database_all_succeed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ax.db");
    {
        let db = Database::open(&path).await.unwrap();
        sqlx::query("DELETE FROM schema_versions WHERE version >= 30")
            .execute(db.pool())
            .await
            .unwrap();
        db.close().await;
    }
    let opens: Vec<_> = (0..6)
        .map(|_| {
            let path = path.clone();
            tokio::spawn(async move { Database::open(&path).await.map(|db| db.path().to_owned()) })
        })
        .collect();
    for open in opens {
        open.await.unwrap().expect("every concurrent open succeeds");
    }
    let db = Database::open(&path).await.unwrap();
    assert_eq!(
        get_current_version(db.pool()).await.unwrap(),
        CURRENT_SCHEMA_VERSION
    );
    db.close().await;
}
