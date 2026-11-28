//! Coding-memory benchmark. The default test does not write a results file.

use std::path::PathBuf;

use ax_memory::bench::{
    baseline_json, load_coding_fixture, missing_families, score_coding, score_dataset_path,
    score_dataset_text_stack, score_shape_text, QUESTION_FAMILIES,
};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/memory/coding.json")
}

fn sample_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/memory/samples")
        .join(name)
}

async fn open_db(dir: &std::path::Path) -> ax_db::Database {
    ax_db::Database::open(&dir.join("ax.db")).await.unwrap()
}

async fn score_fixture() -> ax_memory::bench::CodingReport {
    let text = std::fs::read_to_string(fixture_path()).unwrap();
    let fixture = load_coding_fixture(&text).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let report = score_coding(db.pool(), &fixture).await.unwrap();
    assert_databases_inside(db.pool(), dir.path()).await;
    report
}

/// Every database file the pool has open, attached ones included, lies under `root`.
async fn assert_databases_inside(pool: &sqlx::SqlitePool, root: &std::path::Path) {
    let root = root.canonicalize().unwrap();
    let files: Vec<String> =
        sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE file <> ''")
            .fetch_all(pool)
            .await
            .unwrap();
    assert!(!files.is_empty(), "the bench pool has no database file");
    for file in files {
        let path = std::path::Path::new(&file).canonicalize().unwrap();
        assert!(
            path.starts_with(&root),
            "the bench database {file} is outside {}",
            root.display()
        );
    }
}

#[tokio::test]
async fn coding_fixture_covers_every_family_and_is_deterministic() {
    let once = score_fixture().await;
    let twice = score_fixture().await;
    assert_eq!(once, twice);
    assert_eq!(once.questions.len(), 19);
    let families = once.questions.iter().map(|score| score.family.as_str());
    assert_eq!(missing_families(families), Vec::<&str>::new());
    assert!(QUESTION_FAMILIES.len() == 16);
    assert!(
        !once.inserted.iter().any(|key| key == "backup-only"),
        "a backup-only memory is ground truth and is not written"
    );
    let named = once
        .questions
        .iter()
        .find(|score| score.id == "named-trigger")
        .unwrap();
    assert_eq!(named.ax.first().map(String::as_str), Some("hash-vectors"));
}

#[test]
fn shape_samples_are_not_reported_as_the_public_sets() {
    let longmem = score_shape_text(
        "longmemeval",
        &std::fs::read_to_string(sample_path("longmemeval-shape.json")).unwrap(),
    )
    .unwrap();
    let locomo = score_shape_text(
        "locomo",
        &std::fs::read_to_string(sample_path("locomo-shape.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(longmem.source.as_deref(), Some("shape-sample"));
    assert_eq!(locomo.source.as_deref(), Some("shape-sample"));
    assert!((longmem.summary.unwrap().recall_at_5 - 1.0).abs() < 1e-9);
    assert!((locomo.summary.unwrap().recall_at_5 - 1.0).abs() < 1e-9);
    let skipped = score_dataset_path("longmemeval", None).unwrap();
    assert_eq!(skipped.status, "skipped");
}

#[tokio::test]
#[ignore]
async fn write_coding_baseline() {
    let out = std::env::var("AX_MEMORY_BENCH_OUT")
        .expect("set AX_MEMORY_BENCH_OUT to the baseline file to write");
    let report = score_fixture().await;
    let longmem_sample = score_shape_text(
        "longmemeval",
        &std::fs::read_to_string(sample_path("longmemeval-shape.json")).unwrap(),
    )
    .unwrap();
    let locomo_sample = score_shape_text(
        "locomo",
        &std::fs::read_to_string(sample_path("locomo-shape.json")).unwrap(),
    )
    .unwrap();
    let longmem = score_dataset_text_stack(
        "longmemeval",
        std::env::var_os("AX_LONGMEMEVAL_PATH")
            .as_ref()
            .map(std::path::Path::new),
    )
    .unwrap();
    let locomo = score_dataset_text_stack(
        "locomo",
        std::env::var_os("AX_LOCOMO_PATH")
            .as_ref()
            .map(std::path::Path::new),
    )
    .unwrap();
    let body = baseline_json(
        &report,
        &std::env::var("AX_MEMORY_BENCH_COMMIT").unwrap_or_else(|_| "unknown".into()),
        std::env::var("AX_MEMORY_BENCH_DIRTY").ok().as_deref() == Some("true"),
        &longmem,
        &locomo,
        &longmem_sample,
        &locomo_sample,
    )
    .unwrap();
    let path = PathBuf::from(out);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, body).unwrap();
}
