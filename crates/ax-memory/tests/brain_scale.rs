//! Brain-scale latency: cue lookup and archive search over 100,000 archived memories.
//!
//! Run: `cargo test -p ax-memory --release --test brain_scale -- --ignored --nocapture`

use std::time::{Duration, Instant};

use ax_memory::{remember, search, Depth, RememberInput, SearchRequest};

const ROWS: usize = 100_000;
const RUNS: usize = 5;
const CUE_BUDGET: Duration = Duration::from_millis(5);
const ARCHIVE_BUDGET: Duration = Duration::from_millis(50);
const WORDS: [&str; 20] = [
    "retention", "ledger", "cache", "queue", "rounding", "billing", "session", "index", "upgrade",
    "policy", "rollback", "latency", "webhook", "tenant", "invoice", "schema", "token", "replica",
    "backup", "migration",
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

async fn seed(pool: &sqlx::SqlitePool) {
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

    let mut tx = pool.begin().await.unwrap();
    for i in 0..ROWS {
        let id = format!("scale-{i}");
        let title = format!("Note {i} on {}", topic(i));
        let body = format!("Archived decision about {} for row {i}.", topic(i));
        let (vector, _) = ax_memory::embed::embed_labeled(&format!("{title}\n{body}"));
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
            .bind(serde_json::to_string(&vector).unwrap())
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
    }
    tx.commit().await.unwrap();
}

async fn median_of_runs(pool: &sqlx::SqlitePool, request: &SearchRequest) -> (Duration, Vec<String>) {
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

#[tokio::test(flavor = "multi_thread")]
#[ignore = "seeds 100,000 rows; run in release with --ignored"]
async fn cue_and_archive_search_stay_inside_budget_at_100k_rows() {
    let dir = tempfile::tempdir().unwrap();
    let db = ax_db::Database::open(&dir.path().join("ax.db")).await.unwrap();
    let pool = db.pool();
    let seeding = Instant::now();
    seed(pool).await;
    let archived: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE tier = 't3'")
        .fetch_one(pool)
        .await
        .unwrap();
    assert!(archived >= ROWS as i64);
    println!("seeded {archived} archived rows in {:?}", seeding.elapsed());

    let cue = SearchRequest {
        cue: Some("cue-77777".into()),
        depth: Depth::Archive,
        limit: 20,
        ..SearchRequest::default()
    };
    let (cue_median, cue_ids) = median_of_runs(pool, &cue).await;
    assert_eq!(cue_ids, vec!["scale-77777".to_string()]);

    let archive = SearchRequest {
        query: "retention ledger rollback".into(),
        depth: Depth::Archive,
        limit: 20,
        ..SearchRequest::default()
    };
    let (archive_median, archive_ids) = median_of_runs(pool, &archive).await;
    assert_eq!(archive_ids.len(), 20);

    println!("cue lookup median of {RUNS}: {cue_median:?} (budget {CUE_BUDGET:?})");
    println!("archive top-20 median of {RUNS}: {archive_median:?} (budget {ARCHIVE_BUDGET:?})");
    assert!(cue_median < CUE_BUDGET, "cue lookup {cue_median:?}");
    assert!(archive_median < ARCHIVE_BUDGET, "archive top-20 {archive_median:?}");
}
