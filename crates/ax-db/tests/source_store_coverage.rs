//! What "source store coverage" counts.
//!
//! The number drives a warning that tells the user to run `ax_index`. If the
//! denominator includes files no parser claims — SVG assets, `.o` and `.rmeta`
//! build output the file watcher reported — the warning fires forever and names
//! a command that cannot clear it. On this repo that is 3039 of 3539 file rows.
//!
//! Run: cargo test -p ax-db --test source_store_coverage

use std::path::{Path, PathBuf};

use ax_db::Database;
use ax_db::queries::QueryBuilder;

fn scratch_db(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ax-source-coverage-{name}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&p);
    p
}

fn cleanup(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut p = path.to_path_buf();
        if !suffix.is_empty() {
            p.set_file_name(format!(
                "{}{suffix}",
                path.file_name().unwrap().to_string_lossy()
            ));
        }
        let _ = std::fs::remove_file(&p);
    }
}

async fn insert_file(db: &Database, path: &str, language: &str, size: i64) {
    sqlx::query(
        "INSERT INTO files (path, content_hash, language, size, modified_at, indexed_at)
         VALUES (?, 'h-', ?, ?, 1, 1)",
    )
    .bind(path)
    .bind(language)
    .bind(size)
    .execute(db.pool())
    .await
    .expect("insert file row");
}

#[tokio::test]
async fn coverage_counts_only_files_that_can_own_a_snippet() {
    let path = scratch_db("denominator");
    let db = Database::open(&path).await.expect("open db");
    let queries = QueryBuilder::new(db.pool().clone());
    let cap = ax_db::source_store_cap_bytes() as i64;

    // Two parseable files under the cap: the store is expected to hold both.
    insert_file(&db, "src/a.rs", "rust", 100).await;
    insert_file(&db, "src/b.ts", "typescript", 100).await;
    // No parser claims these, so they can never be asked for a snippet.
    insert_file(&db, "site/icon.svg", "unknown", 100).await;
    insert_file(&db, "target-dev/debug/deps/blob.o", "unknown", 100).await;
    // Parseable but over the cap: deliberately not stored, and not a gap either.
    insert_file(&db, "dist/bundle.js", "javascript", cap + 1).await;

    queries
        .upsert_file_content("src/a.rs", "h-", "fn a() {}\n")
        .await
        .expect("store a.rs");

    let (stored, expected) = queries.source_store_coverage().await.expect("coverage");
    assert_eq!(stored, 1, "one file has stored text");
    assert_eq!(
        expected, 2,
        "only the two parseable under-cap files are expected: assets, build output \
         and over-cap files can never be served as a snippet"
    );

    queries
        .upsert_file_content("src/b.ts", "h-", "export const b = 1;\n")
        .await
        .expect("store b.ts");
    let (stored, expected) = queries.source_store_coverage().await.expect("coverage");
    assert_eq!(
        (stored, expected),
        (2, 2),
        "a fully covered store must read as complete, not as a permanent gap"
    );

    // The cap is read at query time, so raising it widens what coverage expects.
    // That is the knob a user turns to store a large generated file, and the
    // warning has to follow it instead of reporting a by-design gap. Mutating the
    // environment is why this lives in one test: parallel tests in this binary
    // would race on the cap.
    //
    // SAFETY: no other thread in this test binary reads the environment.
    unsafe { std::env::set_var("AX_SOURCE_STORE_MAX_BYTES", (4 * 1024 * 1024).to_string()) };
    let raised = queries.source_store_coverage().await;
    unsafe { std::env::remove_var("AX_SOURCE_STORE_MAX_BYTES") };
    let (_, raised_expected) = raised.expect("coverage under a raised cap");
    assert_eq!(
        raised_expected, 3,
        "a 4 MiB cap brings the over-cap bundle into scope, making the store a real gap again"
    );

    drop(db);
    cleanup(&path);
}
