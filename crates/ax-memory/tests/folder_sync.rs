//! Folder import into `doc` memories (S1–S6 in docs/specs/vault-folders.md).

use std::path::Path;

use ax_memory::{
    doc_memory_id, is_recall_only, remove_folder_memories, sync_folder, FolderSyncReport, DOC_KIND,
    MAX_FOLDER_FILE_BYTES,
};

async fn open_db(dir: &Path) -> ax_db::Database {
    ax_db::Database::open(&dir.join("ax.db")).await.unwrap()
}

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn report(added: usize, updated: usize, removed: usize, skipped: usize) -> FolderSyncReport {
    FolderSyncReport {
        added,
        updated,
        removed,
        skipped,
    }
}

async fn doc_ids(db: &ax_db::Database) -> Vec<String> {
    sqlx::query_scalar("SELECT id FROM memories WHERE kind = 'doc' ORDER BY id")
        .fetch_all(db.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn s1_markdown_and_text_files_become_doc_memories() {
    let tmp = tempfile::tempdir().unwrap();
    let db = open_db(tmp.path()).await;
    let root = tmp.path().join("notes");
    write(&root, "plan.md", "# Release plan\n\nShip on Friday.");
    write(&root, "sub/todo.txt", "buy milk");
    write(&root, "image.png", "not text");

    let got = sync_folder(db.pool(), "notes", &root).await.unwrap();
    assert_eq!(got, report(2, 0, 0, 0));

    let plan = ax_memory::get(db.pool(), &doc_memory_id("notes", "plan.md"))
        .await
        .unwrap()
        .expect("plan.md imported");
    assert_eq!(plan.kind, DOC_KIND);
    assert_eq!(plan.source, "folder");
    assert_eq!(plan.title, "Release plan");
    assert_eq!(plan.tags, vec!["folder:notes".to_string()]);
    assert!(plan.body.starts_with("# Release plan\n\nShip on Friday."), "{}", plan.body);
    assert!(plan.body.ends_with("Source: folders/notes/plan.md"), "{}", plan.body);

    let todo = ax_memory::get(db.pool(), &doc_memory_id("notes", "sub/todo.txt"))
        .await
        .unwrap()
        .expect("sub/todo.txt imported");
    assert_eq!(todo.title, "todo.txt");
}

#[tokio::test]
async fn s2_changes_update_in_place_and_a_second_sync_is_a_no_op() {
    let tmp = tempfile::tempdir().unwrap();
    let db = open_db(tmp.path()).await;
    let root = tmp.path().join("notes");
    write(&root, "a.md", "first");
    sync_folder(db.pool(), "notes", &root).await.unwrap();
    assert_eq!(
        sync_folder(db.pool(), "notes", &root).await.unwrap(),
        report(0, 0, 0, 0)
    );

    write(&root, "a.md", "second");
    assert_eq!(
        sync_folder(db.pool(), "notes", &root).await.unwrap(),
        report(0, 1, 0, 0)
    );
    let row = ax_memory::get(db.pool(), &doc_memory_id("notes", "a.md"))
        .await
        .unwrap()
        .unwrap();
    assert!(row.body.starts_with("second"), "{}", row.body);
    assert_eq!(doc_ids(&db).await.len(), 1);
}

#[tokio::test]
async fn s3_deleted_files_lose_their_memory_and_other_folders_are_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let db = open_db(tmp.path()).await;
    let a = tmp.path().join("a");
    let b = tmp.path().join("b");
    write(&a, "x.md", "x");
    write(&a, "y.md", "y");
    write(&b, "x.md", "x");
    sync_folder(db.pool(), "a", &a).await.unwrap();
    sync_folder(db.pool(), "b", &b).await.unwrap();

    std::fs::remove_file(a.join("y.md")).unwrap();
    assert_eq!(
        sync_folder(db.pool(), "a", &a).await.unwrap(),
        report(0, 0, 1, 0)
    );
    let mut want = vec![doc_memory_id("a", "x.md"), doc_memory_id("b", "x.md")];
    want.sort();
    assert_eq!(doc_ids(&db).await, want);
}

#[tokio::test]
async fn s4_hidden_vendor_and_oversized_files_are_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let db = open_db(tmp.path()).await;
    let root = tmp.path().join("notes");
    write(&root, "keep.md", "keep");
    write(&root, ".obsidian/workspace.md", "hidden");
    write(&root, ".secret.md", "hidden");
    write(&root, "node_modules/pkg/README.md", "vendor");
    write(&root, "target-dev/out.md", "build");
    write(&root, "big.md", &"x".repeat(MAX_FOLDER_FILE_BYTES as usize + 1));

    let got = sync_folder(db.pool(), "notes", &root).await.unwrap();
    assert_eq!(got, report(1, 0, 0, 1));
    assert_eq!(doc_ids(&db).await, vec![doc_memory_id("notes", "keep.md")]);
}

#[cfg(unix)]
#[tokio::test]
async fn s4_symlinks_are_not_followed() {
    let tmp = tempfile::tempdir().unwrap();
    let db = open_db(tmp.path()).await;
    let outside = tmp.path().join("outside");
    write(&outside, "private.md", "private");
    let root = tmp.path().join("notes");
    write(&root, "keep.md", "keep");
    std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
    std::os::unix::fs::symlink(outside.join("private.md"), root.join("file.md")).unwrap();

    sync_folder(db.pool(), "notes", &root).await.unwrap();
    assert_eq!(doc_ids(&db).await, vec![doc_memory_id("notes", "keep.md")]);
}

#[tokio::test]
async fn s5_remove_folder_memories_only_touches_that_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let db = open_db(tmp.path()).await;
    let a = tmp.path().join("a");
    let b = tmp.path().join("a_b");
    write(&a, "x.md", "x");
    write(&b, "x.md", "x");
    sync_folder(db.pool(), "a", &a).await.unwrap();
    sync_folder(db.pool(), "a_b", &b).await.unwrap();
    ax_memory::remember(
        db.pool(),
        ax_memory::RememberInput {
            title: "keep".into(),
            body: "a note".into(),
            kind: None,
            tags: vec![],
            files: vec![],
            source: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(remove_folder_memories(db.pool(), "a").await.unwrap(), 1);
    assert_eq!(doc_ids(&db).await, vec![doc_memory_id("a_b", "x.md")]);
    let notes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE kind = 'note'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(notes, 1);
}

#[tokio::test]
async fn s6_doc_memories_are_recall_only() {
    assert!(is_recall_only(DOC_KIND));
    assert!(is_recall_only(ax_memory::TURN_KIND));
    assert!(!is_recall_only("decision"));

    let tmp = tempfile::tempdir().unwrap();
    let db = open_db(tmp.path()).await;
    let root = tmp.path().join("notes");
    write(&root, "kafka.md", "# Kafka retention\n\nKafka retention is seven days.");
    sync_folder(db.pool(), "notes", &root).await.unwrap();

    let recalled = ax_memory::recall(db.pool(), "kafka retention", 5).await.unwrap();
    assert!(recalled.iter().any(|m| m.memory.kind == DOC_KIND), "recall finds doc memories");
    let injected = ax_memory::recall_for_prompt(db.pool(), "kafka retention", 5)
        .await
        .unwrap();
    assert!(injected.iter().all(|m| m.memory.kind != DOC_KIND), "preflight skips them");
}
