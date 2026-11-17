use ax_db::migrations::CURRENT_SCHEMA_VERSION;
use ax_db::Database;

#[tokio::test]
async fn schema_45_adds_neural_tables_beside_hash_embeddings() {
    assert_eq!(CURRENT_SCHEMA_VERSION, 46);
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("ax.db"))
        .await
        .expect("temp database");
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'neural_%' ORDER BY name",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    assert_eq!(
        names,
        vec![
            "neural_audit".to_string(),
            "neural_embedding".to_string(),
            "neural_feedback".to_string(),
            "neural_graph_suggestion".to_string(),
            "neural_model".to_string(),
            "neural_rank_total".to_string(),
        ]
    );
}
