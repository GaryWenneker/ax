//! Integration tests for incremental sync on the Ax facade.

use std::path::Path;

use ax_core::Ax;
use ax_extraction::orchestrator::IndexOptions;

fn quiet_opts() -> IndexOptions {
    IndexOptions {
        quiet: true,
        ..Default::default()
    }
}

fn write(root: &Path, rel: &str, contents: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}

async fn init_project(root: &Path) -> Ax {
    write(
        root,
        "src/app.ts",
        "export function greet(name: string) { return `hi ${name}`; }\n",
    );
    write(
        root,
        "src/util.ts",
        "export function add(a: number, b: number) { return a + b; }\n",
    );
    let mut ax = Ax::init(root).await.unwrap();
    ax.index_all(quiet_opts(), None).await.unwrap();
    ax
}

#[tokio::test]
async fn sync_without_changes_indexes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut ax = init_project(dir.path()).await;

    let result = ax.sync(quiet_opts(), None).await.unwrap();
    assert_eq!(result.files_indexed, 0, "no files changed, nothing to sync");

    ax.destroy().await.unwrap();
}

#[tokio::test]
async fn sync_reindexes_only_changed_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut ax = init_project(dir.path()).await;

    write(
        dir.path(),
        "src/app.ts",
        "export function farewell(name: string) { return `bye ${name}`; }\n",
    );
    let result = ax.sync(quiet_opts(), None).await.unwrap();
    assert_eq!(result.files_indexed, 1, "only the modified file re-indexes");

    let hits = ax
        .search_nodes("farewell", &ax_types::SearchOptions::default())
        .await
        .unwrap();
    assert!(
        hits.iter().any(|r| r.node.name == "farewell"),
        "new symbol is searchable after sync"
    );

    ax.destroy().await.unwrap();
}

#[tokio::test]
async fn sync_skips_touched_file_with_same_content() {
    let dir = tempfile::tempdir().unwrap();
    let mut ax = init_project(dir.path()).await;

    // Rewrite identical bytes: mtime changes, content hash does not.
    let path = dir.path().join("src/app.ts");
    let contents = std::fs::read_to_string(&path).unwrap();
    // Sleep so the new mtime differs even on coarse filesystem clocks.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    std::fs::write(&path, contents).unwrap();

    let result = ax.sync(quiet_opts(), None).await.unwrap();
    assert_eq!(
        result.files_indexed, 0,
        "identical content must not re-extract"
    );

    ax.destroy().await.unwrap();
}

#[tokio::test]
async fn sync_picks_up_new_and_deleted_files() {
    let dir = tempfile::tempdir().unwrap();
    let mut ax = init_project(dir.path()).await;

    write(
        dir.path(),
        "src/extra.ts",
        "export function extra() { return 42; }\n",
    );
    std::fs::remove_file(dir.path().join("src/util.ts")).unwrap();

    let result = ax.sync(quiet_opts(), None).await.unwrap();
    assert_eq!(result.files_indexed, 2, "one added + one removed");

    let hits = ax
        .search_nodes("add", &ax_types::SearchOptions::default())
        .await
        .unwrap();
    assert!(
        !hits.iter().any(|r| r.node.name == "add"),
        "symbols from deleted files are gone"
    );

    ax.destroy().await.unwrap();
}

#[tokio::test]
async fn sync_reindexes_everything_on_extractor_version_change() {
    let dir = tempfile::tempdir().unwrap();
    let mut ax = init_project(dir.path()).await;

    ax.queries()
        .set_metadata("extraction_version", "0-outdated")
        .await
        .unwrap();

    let result = ax.sync(quiet_opts(), None).await.unwrap();
    assert!(
        result.files_indexed >= 2,
        "stale extraction version forces a full reindex, got {}",
        result.files_indexed
    );

    let stored = ax
        .queries()
        .get_metadata("extraction_version")
        .await
        .unwrap();
    assert_eq!(
        stored.as_deref(),
        Some(ax_extraction::EXTRACTION_VERSION),
        "version metadata is refreshed after reindex"
    );

    ax.destroy().await.unwrap();
}
