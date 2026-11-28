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
async fn background_sync_reconciles_offline_edits_and_watches_live_edits() {
    let dir = tempfile::tempdir().unwrap();
    let mut ax = init_project(dir.path()).await;
    write(
        dir.path(),
        "src/app.ts",
        "export function offlineChange() { return 1; }\n",
    );
    let task = Ax::spawn_background_watch(dir.path().to_path_buf());
    for symbol in ["offlineChange", "liveChange"] {
        tokio::time::timeout(std::time::Duration::from_secs(20), async {
            loop {
                let hits = ax
                    .search_nodes(symbol, &ax_types::SearchOptions::default())
                    .await
                    .unwrap();
                if hits.iter().any(|r| r.node.name == symbol) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
        if symbol == "offlineChange" {
            write(
                dir.path(),
                "src/app.ts",
                "export function liveChange() { return 2; }\n",
            );
        }
    }
    task.abort();
    let _ = task.await;
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

#[tokio::test]
async fn sync_after_extractor_upgrade_drops_old_format_contract_ids() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "api/openapi.yaml",
        "openapi: 3.0.0\npaths:\n  /users:\n    get:\n      operationId: listUsers\n",
    );
    let mut ax = init_project(dir.path()).await;

    // What the previous extractor left: a stamp of the old version and a
    // contract id without the file in it.
    let old_id = "contract:openapi:GET:/users";
    ax.queries()
        .upsert_nodes(&[ax_types::Node {
            id: old_id.into(),
            kind: ax_types::NodeKind::Route,
            name: "GET /users".into(),
            qualified_name: old_id.into(),
            file_path: "api/openapi.yaml".into(),
            language: ax_types::Language::Unknown,
            start_line: 1,
            end_line: 1,
            start_column: 0,
            end_column: 0,
            docstring: None,
            signature: None,
            visibility: None,
            is_exported: Some(true),
            is_async: None,
            is_static: None,
            is_abstract: None,
            decorators: None,
            type_parameters: None,
            return_type: None,
            updated_at: 0,
        }])
        .await
        .unwrap();
    ax.queries()
        .set_metadata("extraction_version", "7")
        .await
        .unwrap();
    assert_ne!(ax_extraction::EXTRACTION_VERSION, "7");

    let result = ax.sync(quiet_opts(), None).await.unwrap();
    assert!(
        result.files_indexed >= 2,
        "an older extraction version forces a full reindex, got {}",
        result.files_indexed
    );
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM nodes WHERE substr(id, 1, 9) = 'contract:'")
            .fetch_all(ax.db_pool())
            .await
            .unwrap();
    assert_eq!(
        ids,
        vec!["contract:openapi:api/openapi.yaml:GET:/users".to_string()],
        "only the current per-file contract id remains"
    );

    ax.destroy().await.unwrap();
}

#[tokio::test]
async fn sync_resolves_only_affected_files() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "src/alpha.ts",
        r#"
import { useState } from "react";
export function alpha() {
  console.log("a");
  useState(0);
  fetch("/api");
  JSON.parse("{}");
}
"#,
    );
    write(
        dir.path(),
        "src/beta.ts",
        r#"
import { useEffect } from "react";
export function beta() {
  console.log("b");
  useEffect(() => {});
  Math.random();
  Promise.resolve(1);
}
"#,
    );
    let mut ax = Ax::init(dir.path()).await.unwrap();
    ax.index_all(quiet_opts(), None).await.unwrap();

    let beta_before = ax
        .queries()
        .get_unresolved_refs_by_files(&["src/beta.ts".into()])
        .await
        .unwrap()
        .len();
    assert!(
        beta_before > 0,
        "beta.ts should have unresolved refs after full index"
    );

    write(
        dir.path(),
        "src/alpha.ts",
        r#"
import { useState } from "react";
export function alphaRenamed() {
  console.log("a2");
  useState(1);
  fetch("/api/v2");
  JSON.parse("[]");
}
"#,
    );
    ax.sync(quiet_opts(), None).await.unwrap();

    let beta_after = ax
        .queries()
        .get_unresolved_refs_by_files(&["src/beta.ts".into()])
        .await
        .unwrap()
        .len();
    assert_eq!(
        beta_after, beta_before,
        "unchanged file's unresolved backlog must not be reprocessed on incremental sync"
    );

    ax.destroy().await.unwrap();
}

#[tokio::test]
async fn index_names_each_maintenance_step() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "src/app.ts",
        "export function greet(name: string) { return name; }\n",
    );
    let mut ax = Ax::init(dir.path()).await.unwrap();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let capture = std::sync::Arc::clone(&seen);
    let progress = Box::new(move |event: ax_types::IndexProgress| {
        if event.phase == ax_types::IndexPhase::Optimizing {
            if let Some(label) = event.file_path {
                capture.lock().expect("progress").push(label);
            }
        }
    }) as Box<dyn FnMut(ax_types::IndexProgress) + Send>;
    ax.index_all(quiet_opts(), Some(progress)).await.unwrap();
    let labels = seen.lock().expect("progress").clone();
    assert_eq!(
        labels,
        vec![
            "markdown docs".to_string(),
            "contracts".to_string(),
            "SQLite maintenance".to_string(),
            "graph episodes".to_string(),
            "activity engrams".to_string(),
            "rename re-anchor".to_string(),
            "anchor flags".to_string(),
            "memory consolidation".to_string(),
            "memory re-embed".to_string(),
        ]
    );
    ax.destroy().await.unwrap();
}

#[tokio::test]
async fn incremental_index_keeps_project_wide_resolution_stats() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "src/app.ts",
        "export function greet() { console.log(\"hi\"); missingHelper(); }\n",
    );
    write(dir.path(), "src/util.ts", "export const ZERO = 0;\n");
    let mut ax = Ax::init(dir.path()).await.unwrap();
    ax.index_all(quiet_opts(), None).await.unwrap();
    let before = ax.get_stats().await.unwrap();
    assert!(
        before.resolution_total.unwrap_or(0) > 0,
        "full index records project-wide totals: {before:?}"
    );

    // A file with no references at all.
    write(dir.path(), "src/util.ts", "export const ONE = 1;\n");
    ax.index_files(&["src/util.ts".into()], quiet_opts(), &mut None)
        .await
        .unwrap();
    let after = ax.get_stats().await.unwrap();
    assert_eq!(after.resolution_total, before.resolution_total);
    assert_eq!(after.resolution_resolved, before.resolution_resolved);
    assert_eq!(after.resolution_unresolved, before.resolution_unresolved);

    ax.destroy().await.unwrap();
}

type Labels = std::sync::Arc<std::sync::Mutex<Vec<String>>>;
type Progress = Option<Box<dyn FnMut(ax_types::IndexProgress) + Send>>;

fn optimizing_labels() -> (Labels, Progress) {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let capture = std::sync::Arc::clone(&seen);
    let progress = Box::new(move |event: ax_types::IndexProgress| {
        if event.phase == ax_types::IndexPhase::Optimizing {
            if let Some(label) = event.file_path {
                capture.lock().expect("progress").push(label);
            }
        }
    }) as Box<dyn FnMut(ax_types::IndexProgress) + Send>;
    (seen, Some(progress))
}

async fn doc_links_to(ax: &Ax, symbol: &str) -> Vec<String> {
    let hits = ax
        .search_nodes(symbol, &ax_types::SearchOptions::default())
        .await
        .unwrap();
    let mut sources = Vec::new();
    for hit in hits.iter().filter(|h| h.node.name == symbol) {
        for edge in ax.queries().get_incoming_edges(&hit.node.id).await.unwrap() {
            if edge.source.starts_with("doc:") {
                sources.push(edge.source);
            }
        }
    }
    sources
}

async fn docs_project(root: &Path) -> Ax {
    write(
        root,
        "src/app.ts",
        "export function greetUser(name: string) { return name; }\n",
    );
    write(
        root,
        "src/util.ts",
        "export function addNumbers(a: number, b: number) { return a + b; }\n",
    );
    write(root, "README.md", "# Demo\n\nCall `greetUser` to say hi.\n");
    let mut ax = Ax::init(root).await.unwrap();
    ax.index_all(quiet_opts(), None).await.unwrap();
    ax
}

#[tokio::test]
async fn scoped_index_of_a_code_file_skips_project_wide_passes() {
    let dir = tempfile::tempdir().unwrap();
    let mut ax = docs_project(dir.path()).await;
    write(
        dir.path(),
        "src/util.ts",
        "export function addNumbers(a: number, b: number) { return b + a; }\n",
    );
    let (seen, mut progress) = optimizing_labels();
    ax.index_files(&["src/util.ts".into()], quiet_opts(), &mut progress)
        .await
        .unwrap();
    let labels = seen.lock().expect("progress").clone();
    assert!(
        !labels
            .iter()
            .any(|l| l == "markdown docs" || l == "contracts"),
        "a code-only change must not rescan every doc and contract: {labels:?}"
    );
    assert!(
        labels.iter().any(|l| l == "SQLite optimize"),
        "a scoped run uses the light maintenance step: {labels:?}"
    );
    ax.destroy().await.unwrap();
}

#[tokio::test]
async fn doc_links_survive_a_scoped_reindex_of_the_code_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut ax = docs_project(dir.path()).await;
    assert_eq!(doc_links_to(&ax, "greetUser").await, vec!["doc:README.md"]);

    write(
        dir.path(),
        "src/app.ts",
        "export function greetUser(name: string) { return `hi ${name}`; }\n",
    );
    ax.index_files(&["src/app.ts".into()], quiet_opts(), &mut None)
        .await
        .unwrap();
    assert_eq!(
        doc_links_to(&ax, "greetUser").await,
        vec!["doc:README.md"],
        "re-extracting the code file must keep the doc's link to it"
    );
    ax.destroy().await.unwrap();
}

#[tokio::test]
async fn scoped_index_of_a_doc_file_relinks_docs() {
    let dir = tempfile::tempdir().unwrap();
    let mut ax = docs_project(dir.path()).await;
    write(
        dir.path(),
        "README.md",
        "# Demo\n\nCall `addNumbers` to add.\n",
    );
    let (seen, mut progress) = optimizing_labels();
    ax.index_files(&["README.md".into()], quiet_opts(), &mut progress)
        .await
        .unwrap();
    assert!(seen
        .lock()
        .expect("progress")
        .iter()
        .any(|l| l == "markdown docs"));
    assert_eq!(doc_links_to(&ax, "addNumbers").await, vec!["doc:README.md"]);
    assert!(doc_links_to(&ax, "greetUser").await.is_empty());
    ax.destroy().await.unwrap();
}

async fn file_change_titles(ax: &Ax) -> Vec<String> {
    sqlx::query_scalar("SELECT title FROM memories WHERE kind = 'file_change' ORDER BY title")
        .fetch_all(ax.db_pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn excluded_paths_write_no_file_change_memory() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join(".git/info")).unwrap();
    std::fs::write(root.join(".git/info/exclude"), ".claude/worktrees/\n").unwrap();
    write(root, ".gitignore", "/target-dev/\n");
    write(root, "web/.gitignore", "test-results/\n");
    let mut ax = init_project(root).await;
    let before = file_change_titles(&ax).await;

    let junk = [
        "target-dev/release/build/x/out.rs",
        "target/debug/build/y/lib.rs",
        "web/test-results/.playwright-artifacts-0/trace.ts",
        "site/node_modules/pkg/index.js",
        ".claude/worktrees/wt/src/app.ts",
    ];
    for rel in junk {
        write(root, rel, "export function junk() { return 0; }\n");
    }
    write(
        root,
        "src/app.ts",
        "export function edited() { return 3; }\n",
    );
    let mut paths: Vec<String> = junk.iter().map(|rel| rel.to_string()).collect();
    paths.push("src/app.ts".into());
    ax.index_files(&paths, quiet_opts(), &mut None)
        .await
        .unwrap();

    let after = file_change_titles(&ax).await;
    let new: Vec<&String> = after.iter().filter(|t| !before.contains(t)).collect();
    assert_eq!(
        new,
        vec!["changed src/app.ts"],
        "only the source edit is captured"
    );

    ax.destroy().await.unwrap();
}
