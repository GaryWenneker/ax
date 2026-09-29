//! Memory vault integration tests against a real migrated ax.db.

use ax_memory::{recall, remember, RememberInput};

async fn open_db(dir: &std::path::Path) -> ax_db::Database {
    ax_db::Database::open(&dir.join("ax.db")).await.unwrap()
}

#[tokio::test]
async fn remember_then_recall_finds_memory() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;

    remember(
        db.pool(),
        RememberInput {
            title: "Use tokio spawn_blocking for file IO".into(),
            body: "Synchronous reads in async handlers block the runtime; wrap them in spawn_blocking.".into(),
            kind: Some("convention".into()),
            tags: vec!["async".into(), "io".into()],
            files: vec!["crates/ax-context/src/explore.rs".into()],
            source: None,
        },
    )
    .await
    .unwrap();

    remember(
        db.pool(),
        RememberInput {
            title: "Pricing config lives in ~/.ax/pricing.toml".into(),
            body: "User can override model pricing there.".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let hits = recall(db.pool(), "why do we use spawn_blocking for io?", 5).await.unwrap();
    assert!(!hits.is_empty(), "expected at least one hit");
    assert_eq!(hits[0].memory.title, "Use tokio spawn_blocking for file IO");
    assert!(hits[0].score > 0.0);
}

#[tokio::test]
async fn recall_with_no_usable_tokens_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let hits = recall(db.pool(), "of in de", 5).await.unwrap();
    assert!(hits.is_empty());
}

async fn remember_fillers(db: &ax_db::Database) {
    remember(
        db.pool(),
        RememberInput {
            title: "Gauntlet vault hint".into(),
            body: "Gauntlet fixture: the vault passphrase hint is cobalt-ledger.".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    for index in 0..24 {
        remember(
            db.pool(),
            RememberInput {
                title: format!("Filler {index}"),
                body: format!("Unrelated fixture note {index}: the build cache key is filler-{index:02}-not-the-hint."),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn recall_without_a_real_match_returns_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    remember_fillers(&db).await;
    let hits = recall(db.pool(), "zebra-quartz migration", 5).await.unwrap();
    let titles: Vec<_> = hits.iter().map(|h| h.memory.title.as_str()).collect();
    assert!(hits.is_empty(), "{titles:?}");
}

#[tokio::test]
async fn recall_by_title_returns_only_that_memory() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    remember_fillers(&db).await;
    let hits = recall(db.pool(), "Gauntlet vault hint", 5).await.unwrap();
    let titles: Vec<_> = hits.iter().map(|h| h.memory.title.as_str()).collect();
    assert_eq!(titles, vec!["Gauntlet vault hint"]);
}

#[tokio::test]
async fn hybrid_recall_survives_typos_via_vector_leg() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;

    remember(
        db.pool(),
        RememberInput {
            title: "Reinstall the CLI binary after building".into(),
            body: "Run scripts/reinstall-cli.ps1 after any ax-cli change.".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // "reinstal" and "binry" won't match FTS tokens exactly; trigram embedding should.
    let hits = recall(db.pool(), "reinstal the binry", 5).await.unwrap();
    assert!(!hits.is_empty(), "vector leg should tolerate typos");
    assert!(hits[0].memory.title.contains("Reinstall"));
}

#[tokio::test]
async fn delete_removes_memory_from_recall() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;

    let row = remember(
        db.pool(),
        RememberInput {
            title: "Temporary decision about caching".into(),
            body: "We cache policy rules per database generation.".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    assert!(ax_memory::delete(db.pool(), &row.id).await.unwrap());
    let hits = recall(db.pool(), "caching decision policy", 5).await.unwrap();
    assert!(hits.is_empty(), "deleted memory must not be recalled");
}
