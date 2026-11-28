//! `remember` retries every write it makes when SQLite reports contention,
//! not only the INSERT. The follow-up UPDATEs (derived columns, tier stamp)
//! used to fail the call with "vtable constructor failed: memories_fts"
//! (code 17) when an FTS rebuild ran at the same time.

use ax_memory::{reindex_memory_fts, remember, RememberInput};

fn input(title: &str) -> RememberInput {
    RememberInput {
        title: title.into(),
        body: format!("{title} body text about retry handling"),
        tags: vec!["retry".into()],
        ..Default::default()
    }
}

/// Make the next `failures` UPDATEs of `column` on `memories` fail with the
/// transient error SQLite returns while the FTS table is being rebuilt.
/// `RAISE(FAIL)` keeps the counter decrement, so the fault clears by itself.
async fn inject_transient_update_fault(pool: &sqlx::SqlitePool, column: &str, failures: i64) {
    // Test-only: `column` is a literal column name from the tests below.
    let table = format!("fault_{column}");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TABLE {table} (n INTEGER NOT NULL)"
    )))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO {table} VALUES (?)"
    )))
    .bind(failures)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TRIGGER {table}_trigger BEFORE UPDATE OF {column} ON memories
         WHEN (SELECT n FROM {table}) > 0
         BEGIN
           UPDATE {table} SET n = n - 1;
           SELECT RAISE(FAIL, 'vtable constructor failed: memories_fts');
         END"
    )))
    .execute(pool)
    .await
    .unwrap();
}

async fn remaining(pool: &sqlx::SqlitePool, column: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT n FROM fault_{column}")))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn remember_retries_the_derived_column_update() {
    let dir = tempfile::tempdir().unwrap();
    let db = ax_db::Database::open(&dir.path().join("ax.db"))
        .await
        .unwrap();
    inject_transient_update_fault(db.pool(), "search_text", 2).await;

    let row = remember(db.pool(), input("Derived retry"))
        .await
        .expect("a transient code 17 on the derived-column UPDATE is retried");

    assert_eq!(remaining(db.pool(), "search_text").await, 0);
    let search_text: String = sqlx::query_scalar("SELECT search_text FROM memories WHERE id = ?")
        .bind(&row.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert!(search_text.contains("retry"), "got {search_text:?}");
}

#[tokio::test]
async fn remember_retries_the_tier_stamp_update() {
    let dir = tempfile::tempdir().unwrap();
    let db = ax_db::Database::open(&dir.path().join("ax.db"))
        .await
        .unwrap();
    inject_transient_update_fault(db.pool(), "tier", 2).await;

    let row = remember(db.pool(), input("Tier retry"))
        .await
        .expect("a transient code 17 on the tier UPDATE is retried");

    assert_eq!(remaining(db.pool(), "tier").await, 0);
    let tier: String = sqlx::query_scalar("SELECT tier FROM memories WHERE id = ?")
        .bind(&row.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(tier, row.tier);
}

/// Real contention: FTS rebuilds on one task while another task remembers.
/// Every remember must succeed and be findable afterwards.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn remember_survives_concurrent_fts_rebuilds() {
    let dir = tempfile::tempdir().unwrap();
    let db = ax_db::Database::open(&dir.path().join("ax.db"))
        .await
        .unwrap();
    let pool = db.pool().clone();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let rebuilder = {
        let pool = pool.clone();
        let stop = stop.clone();
        tokio::spawn(async move {
            let mut rebuilds = 0u32;
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                // A rebuild may itself hit a busy database; that is not what
                // this test checks, so a failed rebuild is just skipped.
                if reindex_memory_fts(&pool).await.is_ok() {
                    rebuilds += 1;
                }
                tokio::task::yield_now().await;
            }
            rebuilds
        })
    };
    let mut writers = Vec::new();
    for w in 0..3 {
        let pool = pool.clone();
        writers.push(tokio::spawn(async move {
            for i in 0..15 {
                remember(&pool, input(&format!("Contention w{w} n{i}")))
                    .await
                    .unwrap_or_else(|e| panic!("remember w{w} n{i} failed: {e}"));
            }
        }));
    }
    for writer in writers {
        writer.await.unwrap();
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let rebuilds = rebuilder.await.unwrap();
    assert!(rebuilds > 0, "the rebuild task never ran");
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE title LIKE 'Contention %'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 45);
}
