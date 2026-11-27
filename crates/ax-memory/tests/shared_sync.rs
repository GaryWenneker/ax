//! Shared memory export and import (`.ax/memory/shared.jsonl`).

use ax_memory::{
    export_shared, get, import_shared, remember, remember_with_triggers, update, RememberInput,
};

async fn open(tag: &str) -> (std::path::PathBuf, ax_db::Database) {
    let root = std::env::temp_dir().join(format!("ax-shared-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();
    (root, db)
}

fn shared(title: &str, body: &str) -> RememberInput {
    RememberInput {
        title: title.into(),
        body: body.into(),
        kind: Some("decision".into()),
        tags: vec!["shared".into()],
        files: vec![],
        source: Some("manual".into()),
    }
}

#[tokio::test]
async fn export_includes_shared_memories_older_than_ten_thousand_other_rows() {
    let (root, db) = open("export-cap").await;
    let pool = db.pool();
    let old = remember(pool, shared("Old shared decision", "Keep it."))
        .await
        .unwrap();
    sqlx::query("UPDATE memories SET updated_at = 1 WHERE id = ?")
        .bind(&old.id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 10050)
         INSERT INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, enabled)
         SELECT 'noise-' || i, 'graph_diff', 'diff ' || i, 'body', '[]', '[]', 1.0, 'graph-sync', 1000 + i, 1000 + i, 1 FROM n",
    )
    .execute(pool)
    .await
    .unwrap();
    let newer = remember(pool, shared("Newer shared decision", "Also keep it."))
        .await
        .unwrap();

    let result = export_shared(pool, &root, "shared", None).await.unwrap();
    assert_eq!(result.written, 2);
    let text = std::fs::read_to_string(&result.path).unwrap();
    let ids: Vec<String> = text
        .lines()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    let mut expected = vec![old.id.clone(), newer.id.clone()];
    expected.sort();
    assert_eq!(ids, expected, "every shared row, ordered by id");
}

#[tokio::test]
async fn import_never_replaces_a_newer_local_edit() {
    let (root, db) = open("import-newer").await;
    let pool = db.pool();
    let row = remember_with_triggers(
        pool,
        shared("Release rule", "Tag before push."),
        &["release-rule".to_string()],
    )
    .await
    .unwrap();
    sqlx::query("UPDATE memories SET updated_at = 1000 WHERE id = ?")
        .bind(&row.id)
        .execute(pool)
        .await
        .unwrap();
    let exported = export_shared(pool, &root, "shared", None).await.unwrap();

    // Local edit after the export: newer than the shared file.
    assert!(update(
        pool,
        &row.id,
        shared("Release rule", "Tag, sign, then push.")
    )
    .await
    .unwrap());

    let result = import_shared(pool, &exported.path).await.unwrap();
    assert_eq!((result.updated, result.skipped), (0, 1));
    let local = get(pool, &row.id).await.unwrap().unwrap();
    assert_eq!(local.body, "Tag, sign, then push.");
}

#[tokio::test]
async fn import_applies_a_newer_shared_row_and_keeps_its_timestamp() {
    let (root, db) = open("import-older").await;
    let pool = db.pool();
    let row = remember(pool, shared("Release rule", "Tag before push."))
        .await
        .unwrap();
    let file = root.join(".ax/memory/shared.jsonl");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let mut incoming = get(pool, &row.id).await.unwrap().unwrap();
    incoming.body = "Tag, sign, then push.".into();
    incoming.updated_at = row.updated_at + 60_000;
    std::fs::write(
        &file,
        format!("{}\n", serde_json::to_string(&incoming).unwrap()),
    )
    .unwrap();

    let result = import_shared(pool, &file).await.unwrap();
    assert_eq!(result.updated, 1);
    let local = get(pool, &row.id).await.unwrap().unwrap();
    assert_eq!(local.body, "Tag, sign, then push.");
    assert_eq!(local.updated_at, incoming.updated_at);

    // Importing the same file again changes nothing.
    let again = import_shared(pool, &file).await.unwrap();
    assert_eq!((again.updated, again.skipped), (0, 1));
}

#[tokio::test]
async fn a_bad_line_aborts_the_import_before_anything_is_written() {
    let (root, db) = open("import-bad").await;
    let pool = db.pool();
    let row = remember(pool, shared("Release rule", "Tag before push."))
        .await
        .unwrap();
    let mut incoming = get(pool, &row.id).await.unwrap().unwrap();
    incoming.body = "Changed by the import.".into();
    incoming.updated_at = row.updated_at + 60_000;
    let file = root.join("shared.jsonl");
    std::fs::write(
        &file,
        format!("{}\nnot json\n", serde_json::to_string(&incoming).unwrap()),
    )
    .unwrap();

    let err = import_shared(pool, &file).await.unwrap_err();
    assert!(err.to_string().contains("line 2"), "{err}");
    let local = get(pool, &row.id).await.unwrap().unwrap();
    assert_eq!(local.body, "Tag before push.");
}
