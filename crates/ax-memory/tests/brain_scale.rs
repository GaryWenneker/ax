//! Brain-scale latency: cue lookup, preflight, recall, and archive search.
//!
//! 100k (default) and 1e6 (release proof bar) ignored benches.
//!
//! Run 100k:
//! `cargo test -p ax-memory --release --test brain_scale cue_and_archive_search_stay_inside_budget_at_100k_rows -- --ignored --nocapture`
//!
//! Run 1e6:
//! `cargo test -p ax-memory --release --test brain_scale speed_budgets_hold_at_1e6_rows -- --ignored --nocapture`

use std::collections::HashMap;
use std::time::{Duration, Instant};

use ax_memory::{
    memories_for_preflight, recall_for_prompt, remember, search, Depth, RememberInput,
    SearchRequest,
};

const ROWS_100K: usize = 100_000;
const ROWS_1M: usize = 1_000_000;
const RUNS: usize = 5;
const CUE_BUDGET: Duration = Duration::from_millis(5);
const PREFLIGHT_P95_BUDGET: Duration = Duration::from_millis(30);
const RECALL_P95_BUDGET: Duration = Duration::from_millis(80);
const ARCHIVE_BUDGET: Duration = Duration::from_millis(50);
const WORDS: [&str; 20] = [
    "retention",
    "ledger",
    "cache",
    "queue",
    "rounding",
    "billing",
    "session",
    "index",
    "upgrade",
    "policy",
    "rollback",
    "latency",
    "webhook",
    "tenant",
    "invoice",
    "schema",
    "token",
    "replica",
    "backup",
    "migration",
];
const REPLACED: [&str; 5] = ["id", "title", "body", "embedding", "tier"];

fn topic(i: usize) -> String {
    format!(
        "{} {} {}",
        WORDS[i % 20],
        WORDS[(i / 20) % 20],
        WORDS[(i / 400) % 20]
    )
}

async fn seed(pool: &sqlx::SqlitePool, rows: usize) {
    let template = remember(
        pool,
        RememberInput {
            title: "Scale template".into(),
            body: "Template row cloned for the scale test.".into(),
            kind: Some("decision".into()),
            tags: vec![],
            files: vec![],
            source: Some("test".into()),
        },
    )
    .await
    .unwrap();
    ax_memory::brain::index_archive_vector(pool, &template.id)
        .await
        .unwrap();

    let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('memories')")
        .fetch_all(pool)
        .await
        .unwrap();
    let selected: Vec<String> = columns
        .iter()
        .map(|c| match REPLACED.iter().position(|r| r == c) {
            Some(slot) => format!("?{}", slot + 1),
            None => c.clone(),
        })
        .collect();
    let insert = format!(
        "INSERT INTO memories ({}) SELECT {} FROM memories WHERE id = ?6",
        columns.join(", "),
        selected.join(", ")
    );

    // Cache embeddings by topic (8_000 unique topics) so 1e6 seeding stays practical.
    let mut embed_cache: HashMap<String, (Vec<f32>, String)> = HashMap::new();
    let mut tx = pool.begin().await.unwrap();
    for i in 0..rows {
        let id = format!("scale-{i}");
        let t = topic(i);
        let title = format!("Note {i} on {t}");
        let body = format!("Archived decision about {t} for row {i}.");
        let (vector, json) = embed_cache
            .entry(t.clone())
            .or_insert_with(|| {
                let (vector, _) = ax_memory::embed::embed_labeled(&format!("{t}\n{t}"));
                let json = serde_json::to_string(&vector).unwrap();
                (vector, json)
            })
            .clone();
        sqlx::query(&insert)
            .bind(&id)
            .bind(&title)
            .bind(&body)
            .bind(ax_memory::embed::embedding_to_blob(&vector))
            .bind("t3")
            .bind(&template.id)
            .execute(&mut *tx)
            .await
            .unwrap();
        let rowid = i as i64 + 1_000;
        sqlx::query("INSERT INTO memory_vec(rowid, embedding) VALUES (?, ?)")
            .bind(rowid)
            .bind(&json)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO memory_vec_map (memory_id, rowid) VALUES (?, ?)")
            .bind(&id)
            .bind(rowid)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO memory_cues (cue, memory_id, weight, source) VALUES (?, ?, 1.0, 'trigger')",
        )
        .bind(format!("cue-{i}"))
        .bind(&id)
        .execute(&mut *tx)
        .await
        .unwrap();
        if (i + 1) % 50_000 == 0 {
            tx.commit().await.unwrap();
            tx = pool.begin().await.unwrap();
            println!("seeded {} / {rows}", i + 1);
        }
    }
    tx.commit().await.unwrap();
}

fn percentile(mut times: Vec<Duration>, pct: usize) -> Duration {
    times.sort();
    let idx = ((times.len() * pct) / 100).min(times.len() - 1);
    times[idx]
}

async fn median_of_runs(
    pool: &sqlx::SqlitePool,
    request: &SearchRequest,
) -> (Duration, Vec<String>) {
    let mut times = Vec::with_capacity(RUNS);
    let mut ids = Vec::new();
    for _ in 0..RUNS {
        let start = Instant::now();
        let hits = search(pool, request).await.unwrap();
        times.push(start.elapsed());
        ids = hits.into_iter().map(|m| m.memory.id).collect();
    }
    times.sort();
    (times[RUNS / 2], ids)
}

async fn p95_of_fn<F, Fut>(mut f: F) -> Duration
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let mut times = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let start = Instant::now();
        f().await;
        times.push(start.elapsed());
    }
    percentile(times, 95)
}

async fn run_scale_budgets(rows: usize, probe_cue: usize) {
    let dir = tempfile::tempdir().unwrap();
    // Layout: <root>/.ax/ax.db so backup paths resolve if needed.
    let ax_dir = dir.path().join(".ax");
    std::fs::create_dir_all(&ax_dir).unwrap();
    let db = ax_db::Database::open(&ax_dir.join("ax.db")).await.unwrap();
    let pool = db.pool();
    let seeding = Instant::now();
    seed(pool, rows).await;
    let archived: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE tier = 't3'")
        .fetch_one(pool)
        .await
        .unwrap();
    assert!(archived >= rows as i64);
    println!("seeded {archived} archived rows in {:?}", seeding.elapsed());

    // Warm the page cache so cold SQLite I/O is not charged to p95 budgets.
    let warm = SearchRequest {
        query: "retention ledger rollback".into(),
        depth: Depth::Archive,
        limit: 20,
        ..SearchRequest::default()
    };
    let _ = search(pool, &warm).await.unwrap();
    let _ = memories_for_preflight(pool, "retention ledger rollback policy upgrade", 10).await;
    let _ = recall_for_prompt(pool, "retention ledger rollback policy upgrade", 10)
        .await
        .unwrap();

    let cue = SearchRequest {
        cue: Some(format!("cue-{probe_cue}")),
        depth: Depth::Archive,
        limit: 20,
        ..SearchRequest::default()
    };
    let (cue_median, cue_ids) = median_of_runs(pool, &cue).await;
    assert_eq!(cue_ids, vec![format!("scale-{probe_cue}")]);

    let archive = SearchRequest {
        query: "retention ledger rollback".into(),
        depth: Depth::Archive,
        limit: 20,
        ..SearchRequest::default()
    };
    let (archive_median, archive_ids) = median_of_runs(pool, &archive).await;
    assert_eq!(archive_ids.len(), 20);

    let prompt = "retention ledger rollback policy upgrade";
    let preflight_p95 = p95_of_fn(|| async {
        let _ = memories_for_preflight(pool, prompt, 10).await;
    })
    .await;
    let recall_p95 = p95_of_fn(|| async {
        let _ = recall_for_prompt(pool, prompt, 10).await.unwrap();
    })
    .await;

    println!("cue lookup median of {RUNS}: {cue_median:?} (budget {CUE_BUDGET:?})");
    println!("preflight p95 of {RUNS}: {preflight_p95:?} (budget {PREFLIGHT_P95_BUDGET:?})");
    println!("recall p95 of {RUNS}: {recall_p95:?} (budget {RECALL_P95_BUDGET:?})");
    println!("archive top-20 median of {RUNS}: {archive_median:?} (budget {ARCHIVE_BUDGET:?})");

    assert!(cue_median < CUE_BUDGET, "cue lookup {cue_median:?}");
    assert!(
        preflight_p95 < PREFLIGHT_P95_BUDGET,
        "preflight p95 {preflight_p95:?}"
    );
    assert!(recall_p95 < RECALL_P95_BUDGET, "recall p95 {recall_p95:?}");
    assert!(
        archive_median < ARCHIVE_BUDGET,
        "archive top-20 {archive_median:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "seeds 100,000 rows; run in release with --ignored"]
async fn cue_and_archive_search_stay_inside_budget_at_100k_rows() {
    run_scale_budgets(ROWS_100K, 77_777).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "seeds 1,000,000 rows; run in release with --ignored"]
async fn speed_budgets_hold_at_1e6_rows() {
    run_scale_budgets(ROWS_1M, 777_777).await;
}
