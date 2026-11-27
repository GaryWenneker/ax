//! PMI query expansion learns only from durable (semantic tier) memories.

use ax_memory::expand_with_pmi;

#[tokio::test]
async fn pmi_ignores_turns_docs_and_archive_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();
    let pool = db.pool();

    let rows: &[(&str, &str, &str, &str)] = &[
        ("n1", "note", "t2", "zephyr budget planning"),
        ("n2", "note", "t2", "alpha beta gamma"),
        ("n3", "decision", "t2", "delta epsilon"),
        ("n4", "note", "t2", "omega sigma"),
        ("n5", "note", "t2", "lambda kappa"),
        ("t1", "turn", "t1", "zephyr quasar transcript"),
        ("t2", "turn", "t1", "zephyr quasar transcript again"),
        ("d1", "doc", "t3", "zephyr quasar document"),
        ("g1", "graph_diff", "t3", "zephyr quasar diff"),
    ];
    for (i, (id, kind, tier, body)) in rows.iter().enumerate() {
        sqlx::query(
            "INSERT INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, enabled, tier)
             VALUES (?, ?, ?, ?, '[]', '[]', 1.0, 'test', ?, ?, 1, ?)",
        )
        .bind(id)
        .bind(kind)
        .bind(*id)
        .bind(body)
        .bind(i as i64)
        .bind(i as i64)
        .bind(tier)
        .execute(pool)
        .await
        .unwrap();
    }

    let expanded = expand_with_pmi(pool, "zephyr", 10).await.unwrap();
    assert!(
        !expanded.iter().any(|t| t == "quasar"),
        "turn, doc and archive rows must not teach PMI: {expanded:?}"
    );
    assert!(
        expanded.iter().any(|t| t == "budget"),
        "durable rows still do: {expanded:?}"
    );

    // The same vault always expands the same way.
    for _ in 0..3 {
        assert_eq!(expand_with_pmi(pool, "zephyr", 10).await.unwrap(), expanded);
    }
}
