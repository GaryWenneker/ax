//! What the source store is allowed to hold.
//!
//! `index_files` takes paths from callers that did not run the scan filter — the
//! daemon's file watcher is the loud example: while cargo builds, it reports
//! every `.o` and `.rmeta` under the build directory. Those files have no
//! language, so they are never parsed and can never own a graph node, which
//! means a stored copy of their text can never be served as a snippet. Storing
//! it anyway put 90 MB of object-file text into this repo's ax.db.
//!
//! Run: cargo test -p ax-extraction --test source_store_write_path

use ax_db::Database;
use ax_db::queries::QueryBuilder;
use ax_extraction::orchestrator::{ExtractionOrchestrator, IndexOptions};

struct Fixture {
    root: std::path::PathBuf,
    db: std::path::PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "ax-source-store-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&base).expect("create fixture dir");
        Self {
            db: base.join("ax.db"),
            root: base,
        }
    }

    fn write(&self, rel: &str, body: &str) {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, body).expect("write fixture file");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn index_files_stores_source_only_for_files_it_can_parse() {
    let fx = Fixture::new("write-path");
    fx.write("src/keep.rs", "pub fn keep() -> u32 {\n    7\n}\n");
    // Stand-ins for build output: a linker object and a rustc metadata blob.
    // Neither has a language, so neither is ever parsed.
    fx.write("target-dev/debug/deps/blob.o", "\u{1}ELF not really code\n");
    fx.write("target-dev/debug/deps/libthing.rmeta", "rust metadata blob\n");

    let db = Database::open(&fx.db).await.expect("open db");
    let queries = QueryBuilder::new(db.pool().clone());
    let orchestrator = ExtractionOrchestrator::new(fx.root.clone());
    let opts = IndexOptions::default();

    let paths = vec![
        "src/keep.rs".to_string(),
        "target-dev/debug/deps/blob.o".to_string(),
        "target-dev/debug/deps/libthing.rmeta".to_string(),
    ];
    orchestrator
        .index_files(&queries, &paths, &opts, None)
        .await
        .expect("index_files");

    let kept = queries
        .get_file_content("src/keep.rs")
        .await
        .expect("query stored source");
    assert!(
        kept.is_some(),
        "a parseable source file must be stored, or snippets fall back to disk"
    );

    for artifact in ["target-dev/debug/deps/blob.o", "target-dev/debug/deps/libthing.rmeta"] {
        let stored = queries
            .get_file_content(artifact)
            .await
            .expect("query stored source");
        assert!(
            stored.is_none(),
            "{artifact} has no language, so no snippet can ever be served from it — \
             storing its text only inflates ax.db (got {} bytes)",
            stored.map(|c| c.content.len()).unwrap_or(0)
        );
    }
}

#[tokio::test]
async fn sync_prunes_stored_source_that_no_indexed_file_claims() {
    let fx = Fixture::new("prune");
    fx.write("src/keep.rs", "pub fn keep() -> u32 {\n    7\n}\n");

    let db = Database::open(&fx.db).await.expect("open db");
    let queries = QueryBuilder::new(db.pool().clone());
    let orchestrator = ExtractionOrchestrator::new(fx.root.clone());
    let opts = IndexOptions::default();

    orchestrator
        .index_all(&queries, &opts, None)
        .await
        .expect("index_all");

    // Simulate what a pre-fix binary left behind: a stored row for a path the
    // graph knows nothing about.
    queries
        .upsert_file_content(
            "target-dev/debug/deps/stale.o",
            "deadbeef",
            "object file text",
        )
        .await
        .expect("seed orphan row");
    assert!(
        queries
            .get_file_content("target-dev/debug/deps/stale.o")
            .await
            .expect("query")
            .is_some(),
        "fixture must actually seed the orphan row"
    );

    let result = orchestrator
        .sync_changed(&queries, &opts, None)
        .await
        .expect("sync_changed");

    assert!(
        queries
            .get_file_content("target-dev/debug/deps/stale.o")
            .await
            .expect("query")
            .is_none(),
        "sync must drop stored source no indexed file claims, or a v17 database \
         keeps carrying whatever an older binary wrote"
    );
    assert_eq!(
        result.source_pruned, 1,
        "the prune has to be reported, not silent"
    );
    assert!(
        queries
            .get_file_content("src/keep.rs")
            .await
            .expect("query")
            .is_some(),
        "pruning must not touch source the graph still needs"
    );
}
