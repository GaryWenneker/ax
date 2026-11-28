//! Hot recall prepares the same SQL whatever the hot set holds.

use ax_memory::{recall, remember, RememberInput};
use sqlx::Connection;

fn note(n: usize) -> RememberInput {
    RememberInput {
        title: format!("Release checklist step {n}"),
        body: format!("Run the release checklist before tagging, step {n}."),
        kind: Some("decision".into()),
        tags: vec![],
        files: vec![],
        source: Some("test".into()),
    }
}

#[tokio::test]
async fn hot_recall_does_not_prepare_a_new_statement_per_hot_set() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ax.db");
    let db = ax_db::Database::open(&path).await.unwrap();
    for n in 0..3 {
        remember(db.pool(), note(n)).await.unwrap();
    }
    let single = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(&path))
        .await
        .unwrap();

    assert!(!recall(&single, "release checklist", 5)
        .await
        .unwrap()
        .is_empty());
    let before = single.acquire().await.unwrap().cached_statements_size();
    for n in 3..13 {
        remember(db.pool(), note(n)).await.unwrap();
        assert!(!recall(&single, "release checklist", 5)
            .await
            .unwrap()
            .is_empty());
    }
    let after = single.acquire().await.unwrap().cached_statements_size();
    assert!(
        after <= before + 2,
        "statement cache grew from {before} to {after} over 10 recalls"
    );
}

/// Vector scoring keeps the best rows even when many more rows stream past
/// than it keeps.
#[tokio::test]
async fn find_similar_keeps_the_best_rows_across_many_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let db = ax_db::Database::open(&dir.path().join("ax.db"))
        .await
        .unwrap();
    for n in 0..150 {
        remember(
            db.pool(),
            RememberInput {
                title: format!("Unrelated topic {n} about gardening tomatoes"),
                body: format!("Water plant number {n} in the morning."),
                kind: Some("note".into()),
                tags: vec![],
                files: vec![],
                source: Some("test".into()),
            },
        )
        .await
        .unwrap();
    }
    let target = remember(db.pool(), note(999)).await.unwrap();
    let text = format!("{}\n{}", target.title, target.body);
    let hits = ax_memory::find_similar(db.pool(), &text, None, 0.0, 3)
        .await
        .unwrap();
    assert_eq!(hits.len(), 3);
    assert_eq!(hits[0].memory.id, target.id);
    assert!(hits.windows(2).all(|w| w[0].score >= w[1].score));
    let without = ax_memory::find_similar(db.pool(), &text, Some(&target.id), 0.0, 3)
        .await
        .unwrap();
    assert!(without.iter().all(|m| m.memory.id != target.id));
}
