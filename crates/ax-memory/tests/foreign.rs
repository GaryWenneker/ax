use ax_db::Database;
use ax_memory::{recall_for_prompt, remember_with_triggers, RememberInput};

async fn open() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("ax.db")).await.unwrap();
    (dir, db)
}

fn note(
    title: &str,
    body: &str,
    source: &str,
    files: Vec<String>,
    tags: Vec<String>,
) -> RememberInput {
    RememberInput {
        title: title.into(),
        body: body.into(),
        kind: Some("note".into()),
        tags,
        files,
        source: Some(source.into()),
    }
}

#[tokio::test]
async fn a_blossom_note_stays_out_of_preflight_and_remains_stored() {
    let (_dir, db) = open().await;
    let pool = db.pool();
    let foreign = remember_with_triggers(
        pool,
        note(
            "Blossom deploy path",
            "The deploy script lives in that repository.",
            "blossom",
            vec![],
            vec![],
        ),
        &["deploy-path".into()],
    )
    .await
    .unwrap();
    let _project = remember_with_triggers(
        pool,
        note(
            "Local archive",
            "The archive stays on this machine.",
            "manual",
            vec![],
            vec![],
        ),
        &["deploy-path".into()],
    )
    .await
    .unwrap();
    remember_with_triggers(
        pool,
        note(
            "Mention only",
            "A note can mention Blossom without leaving this project.",
            "manual",
            vec![],
            vec![],
        ),
        &["mention-only".into()],
    )
    .await
    .unwrap();
    remember_with_triggers(
        pool,
        note(
            "Outside tree",
            "This file is not in the ax tree.",
            "mcp",
            vec!["/tmp/blossom/src/app.ts".into()],
            vec![],
        ),
        &["outside-tree".into()],
    )
    .await
    .unwrap();

    let injected = recall_for_prompt(pool, "deploy-path mention-only outside-tree", 10)
        .await
        .unwrap();
    let titles: Vec<&str> = injected
        .iter()
        .map(|hit| hit.memory.title.as_str())
        .collect();
    assert!(
        !titles.contains(&"Blossom deploy path"),
        "a blossom source must not be injected: {titles:?}"
    );
    assert!(
        !titles.contains(&"Outside tree"),
        "a blossom path must not be injected: {titles:?}"
    );
    assert!(titles.contains(&"Local archive"), "{titles:?}");
    assert!(titles.contains(&"Mention only"), "{titles:?}");

    let status: String = sqlx::query_scalar("SELECT status FROM memories WHERE id = ?")
        .bind(&foreign.id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(status, "foreign");
}

#[tokio::test]
async fn migration_33_flags_an_existing_blossom_note() {
    let (dir, db) = open().await;
    let path = dir.path().join("ax.db");
    sqlx::query(
        "INSERT INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, status)
         VALUES (?, 'note', ?, ?, '[]', '[]', 1.0, 'mcp', 1, 1, 'active')",
    )
    .bind("leaked")
    .bind("Blossom deploy path")
    .bind("Blossom deploys from the blossom repository.")
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, status)
         VALUES ('local', 'note', 'Local archive', 'The archive stays on this machine.', '[]', '[]', 1.0, 'manual', 1, 1, 'active')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("DELETE FROM schema_versions WHERE version >= 33")
        .execute(db.pool())
        .await
        .unwrap();
    db.close().await;

    let db = Database::open(&path).await.unwrap();
    let leaked: String = sqlx::query_scalar("SELECT status FROM memories WHERE id = 'leaked'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    let local: String = sqlx::query_scalar("SELECT status FROM memories WHERE id = 'local'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    db.close().await;
    assert_eq!(leaked, "foreign");
    assert_eq!(local, "active");
}
