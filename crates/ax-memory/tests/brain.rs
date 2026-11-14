//! Brain-scale memory: cues, demotion, archive ANN, episodes, and consolidation.

use ax_memory::{
    capture_activity_engrams, capture_graph_episodes, confirm_memory, consolidate,
    memories_for_preflight, prune_turns, recall, recall_for_prompt, remember,
    remember_with_triggers, save_turn, search, Depth, RememberInput, SearchRequest, TurnRecord,
    TURN_KIND,
};

const DAY_MS: i64 = 86_400_000;
const NOW: i64 = 1_763_000_000_000;

async fn open_db(dir: &std::path::Path) -> ax_db::Database {
    ax_db::Database::open(&dir.join("ax.db")).await.unwrap()
}

fn note(title: &str, body: &str, files: Vec<String>) -> RememberInput {
    RememberInput {
        title: title.into(),
        body: body.into(),
        kind: Some("decision".into()),
        tags: vec![],
        files,
        source: Some("test".into()),
    }
}

#[tokio::test]
async fn exact_cue_outranks_and_unknown_cue_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let row = remember_with_triggers(
        db.pool(),
        note(
            "Ship the binary together",
            "Copy every ax binary after a build.",
            vec![],
        ),
        &["reinstall-all".into()],
    )
    .await
    .unwrap();

    let hit = search(
        db.pool(),
        &SearchRequest {
            cue: Some("reinstall-all".into()),
            limit: 3,
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(hit.len(), 1);
    assert_eq!(hit[0].memory.id, row.id);

    let miss = search(
        db.pool(),
        &SearchRequest {
            cue: Some("not-a-registered-cue".into()),
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert!(miss.is_empty());

    let injected = recall_for_prompt(db.pool(), "please reinstall-all before handoff", 3)
        .await
        .unwrap();
    assert_eq!(injected[0].memory.id, row.id);
}

#[tokio::test]
async fn a_91_day_turn_leaves_hot_recall_and_stays_fetchable() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    save_turn(
        db.pool(),
        &TurnRecord {
            id: "turn-91".into(),
            title: "Retention policy".into(),
            body: "Retention policy\nfiles: src/cache.rs".into(),
            files: vec!["src/cache.rs".into()],
        },
        NOW - 91 * DAY_MS,
    )
    .await
    .unwrap();
    let backups = dir.path().join("backups");
    assert_eq!(prune_turns(db.pool(), NOW, 90, &backups).await.unwrap(), 1);

    let hot = recall(db.pool(), "Retention policy", 5).await.unwrap();
    assert!(hot.iter().all(|hit| hit.memory.id != "turn-91"));
    let injected = memories_for_preflight(db.pool(), "Retention policy", 3).await;
    assert!(injected.iter().all(|hit| hit.memory.id != "turn-91"));

    let by_id = search(
        db.pool(),
        &SearchRequest {
            id: Some("turn-91".into()),
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(by_id[0].memory.id, "turn-91");
    assert_eq!(by_id[0].memory.kind, TURN_KIND);

    let by_cue = search(
        db.pool(),
        &SearchRequest {
            cue: Some("retention-policy".into()),
            depth: Depth::Hot,
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(by_cue[0].memory.id, "turn-91");

    let tier: String = sqlx::query_scalar("SELECT tier FROM memories WHERE id = 'turn-91'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(tier, "t3");
}

#[tokio::test]
async fn archive_knn_matches_brute_force_and_skips_other_models() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let keep = remember(
        db.pool(),
        note(
            "Queue retention",
            "Keep kafka retention aligned with the cache.",
            vec![],
        ),
    )
    .await
    .unwrap();
    let other = remember(
        db.pool(),
        note(
            "Queue retention copy",
            "Keep kafka retention aligned with the cache.",
            vec![],
        ),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE memories SET tier = 't3' WHERE id IN (?, ?)")
        .bind(&keep.id)
        .bind(&other.id)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE memories SET embedding_model = 'other-model' WHERE id = ?")
        .bind(&other.id)
        .execute(db.pool())
        .await
        .unwrap();
    ax_memory::brain::index_archive_vector(db.pool(), &keep.id)
        .await
        .unwrap();
    ax_memory::brain::index_archive_vector(db.pool(), &other.id)
        .await
        .unwrap();

    let found = search(
        db.pool(),
        &SearchRequest {
            query: "kafka retention cache".into(),
            depth: Depth::Archive,
            limit: 5,
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert!(found.iter().any(|hit| hit.memory.id == keep.id));
    assert!(found.iter().all(|hit| hit.memory.id != other.id));
}

#[tokio::test]
async fn graph_episode_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
         VALUES ('sym-1', 'function', 'save_turn', 'turns::save_turn', 'src/turns.rs', 'rust', 1, 4, 0, 1, 'fn save_turn()', 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    assert_eq!(capture_graph_episodes(db.pool()).await.unwrap(), 0);
    sqlx::query("UPDATE nodes SET signature = 'fn save_turn(now: i64)' WHERE id = 'sym-1'")
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(capture_graph_episodes(db.pool()).await.unwrap(), 1);
    assert_eq!(capture_graph_episodes(db.pool()).await.unwrap(), 0);
    let again = search(
        db.pool(),
        &SearchRequest {
            query: "save_turn".into(),
            depth: Depth::Archive,
            limit: 5,
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].memory.kind, "graph_diff");
}

#[tokio::test]
async fn a_hyphenated_cue_in_the_prompt_comes_first() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    remember(
        db.pool(),
        note(
            "Cache eviction notes",
            "The build cache is slow. Cache eviction keeps the build cache small.",
            vec![],
        ),
    )
    .await
    .unwrap();
    let cue = remember_with_triggers(
        db.pool(),
        note(
            "Ship together",
            "Copy every ax binary after a build.",
            vec![],
        ),
        &["reinstall-all".to_string()],
    )
    .await
    .unwrap();
    let hits = recall_for_prompt(
        db.pool(),
        "reinstall-all after the build cache is slow and cache eviction",
        3,
    )
    .await
    .unwrap();
    assert_eq!(hits.first().map(|hit| &hit.memory.id), Some(&cue.id));
}

#[tokio::test]
async fn a_trigger_that_is_also_a_title_word_stays_a_trigger() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let row = remember_with_triggers(
        db.pool(),
        note("Cache rules", "Evict after a day.", vec![]),
        &["cache".to_string()],
    )
    .await
    .unwrap();
    let source: String =
        sqlx::query_scalar("SELECT source FROM memory_cues WHERE memory_id = ? AND cue = 'cache'")
            .bind(&row.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(source, "trigger");
}

#[tokio::test]
async fn archive_vector_points_at_its_own_memory() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let mut archived = Vec::new();
    for title in ["Queue retention", "Cache warmup", "Ledger rounding"] {
        let row = remember(db.pool(), note(title, &format!("{title} notes."), vec![]))
            .await
            .unwrap();
        sqlx::query("UPDATE memories SET tier = 't3' WHERE id = ?")
            .bind(&row.id)
            .execute(db.pool())
            .await
            .unwrap();
        ax_memory::brain::index_archive_vector(db.pool(), &row.id)
            .await
            .unwrap();
        archived.push((title, row.id));
    }
    for (title, id) in &archived {
        let nearest = ax_memory::brain::knn(db.pool(), &format!("{title} notes."), 1)
            .await
            .unwrap();
        assert_eq!(nearest.first(), Some(id), "{title}");
    }
}

#[tokio::test]
async fn schema_29_rebuilds_a_misdirected_archive_index() {
    let dir = tempfile::tempdir().unwrap();
    let mut ids = Vec::new();
    {
        let db = open_db(dir.path()).await;
        for title in ["Queue retention", "Cache warmup"] {
            let row = remember(db.pool(), note(title, &format!("{title} notes."), vec![]))
                .await
                .unwrap();
            sqlx::query("UPDATE memories SET tier = 't3' WHERE id = ?")
                .bind(&row.id)
                .execute(db.pool())
                .await
                .unwrap();
            ax_memory::brain::index_archive_vector(db.pool(), &row.id)
                .await
                .unwrap();
            ids.push(row.id);
        }
        sqlx::query("UPDATE memory_vec_map SET rowid = rowid + 100")
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query("DELETE FROM schema_versions WHERE version = 29")
            .execute(db.pool())
            .await
            .unwrap();
        db.close().await;
    }
    let db = open_db(dir.path()).await;
    ax_memory::reembed_stale(db.pool(), 32).await.unwrap();
    let nearest = ax_memory::brain::knn(db.pool(), "Cache warmup notes.", 1)
        .await
        .unwrap();
    assert_eq!(nearest.first(), Some(&ids[1]));
}

#[tokio::test]
async fn archive_word_match_outranks_trigram_lookalikes() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let mut lookalikes = Vec::new();
    for rule in [
        "install-version-resolution",
        "codegraph-retention-parity",
        "subagents-attention",
    ] {
        let row = remember(
            db.pool(),
            note(
                &format!("present rule rules/{rule}.mdc"),
                &format!("present rule rules/{rule}.mdc at catalog 1"),
                vec![],
            ),
        )
        .await
        .unwrap();
        lookalikes.push(row.id);
    }
    let wanted = remember(
        db.pool(),
        note(
            "Retention policy",
            "Retention policy for the cache.",
            vec![],
        ),
    )
    .await
    .unwrap();
    for id in lookalikes.iter().chain([&wanted.id]) {
        sqlx::query("UPDATE memories SET tier = 't3' WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await
            .unwrap();
        ax_memory::brain::index_archive_vector(db.pool(), id)
            .await
            .unwrap();
    }

    let found = search(
        db.pool(),
        &SearchRequest {
            query: "Retention".into(),
            depth: Depth::Archive,
            limit: 2,
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(found.first().map(|hit| &hit.memory.id), Some(&wanted.id));

    let ninety_one_days_ago = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
        - 91 * DAY_MS;
    sqlx::query("UPDATE memories SET updated_at = ?, created_at = ? WHERE id = ?")
        .bind(ninety_one_days_ago)
        .bind(ninety_one_days_ago)
        .bind(&wanted.id)
        .execute(db.pool())
        .await
        .unwrap();
    let old = search(
        db.pool(),
        &SearchRequest {
            query: "Retention".into(),
            depth: Depth::Archive,
            limit: 2,
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        old.first().map(|hit| &hit.memory.id),
        Some(&wanted.id),
        "age must not sink an archive match: {:?}",
        old.iter()
            .map(|hit| (&hit.memory.title, hit.score))
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn archive_search_does_not_return_unrelated_rows() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let unrelated = remember(
        db.pool(),
        note(
            "Policy history",
            "present rule utf8-no-bom in the project catalog.",
            vec![],
        ),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE memories SET tier = 't3' WHERE id = ?")
        .bind(&unrelated.id)
        .execute(db.pool())
        .await
        .unwrap();
    ax_memory::brain::index_archive_vector(db.pool(), &unrelated.id)
        .await
        .unwrap();

    let found = search(
        db.pool(),
        &SearchRequest {
            query: "kafka".into(),
            depth: Depth::Archive,
            limit: 5,
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert!(
        found.is_empty(),
        "{:?}",
        found
            .iter()
            .map(|hit| &hit.memory.title)
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn a_symbol_that_changes_back_and_forth_keeps_recording() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
         VALUES ('sym-1', 'function', 'save_turn', 'turns::save_turn', 'src/turns.rs', 'rust', 1, 4, 0, 1, 'fn save_turn()', 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    assert_eq!(capture_graph_episodes(db.pool()).await.unwrap(), 0);
    for signature in [
        "fn save_turn(now: i64)",
        "fn save_turn()",
        "fn save_turn(now: i64)",
        "fn save_turn()",
    ] {
        sqlx::query("UPDATE nodes SET signature = ? WHERE id = 'sym-1'")
            .bind(signature)
            .execute(db.pool())
            .await
            .unwrap();
        assert_eq!(capture_graph_episodes(db.pool()).await.unwrap(), 1);
    }
    let episodes: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE kind = 'graph_diff'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(episodes, 4);
}

#[tokio::test]
async fn candidate_stays_out_of_preflight_until_confirm() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    save_turn(
        db.pool(),
        &TurnRecord {
            id: "turn-decision".into(),
            title: "Use cues".into(),
            body: "We decided to fetch by cue because names are exact.".into(),
            files: vec![],
        },
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(consolidate(db.pool()).await.unwrap(), 1);
    let before = recall_for_prompt(db.pool(), "decided to fetch by cue", 5)
        .await
        .unwrap();
    assert!(before
        .iter()
        .all(|hit| hit.memory.source != "consolidation"));
    let candidate: String = sqlx::query_scalar(
        "SELECT id FROM memories WHERE source = 'consolidation' AND status = 'candidate'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert!(confirm_memory(db.pool(), &candidate).await.unwrap());
    let after = recall_for_prompt(db.pool(), "decided to fetch by cue because names", 5)
        .await
        .unwrap();
    assert!(after.iter().any(|hit| hit.memory.id == candidate));
}

#[tokio::test]
async fn open_file_boosts_the_memory_that_names_it() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let named = remember(
        db.pool(),
        note(
            "Cache policy",
            "The cache policy lives beside the store.",
            vec!["src/store.rs".into()],
        ),
    )
    .await
    .unwrap();
    let other = remember(
        db.pool(),
        note(
            "Cache policy notes",
            "The cache policy lives beside the store.",
            vec!["docs/other.md".into()],
        ),
    )
    .await
    .unwrap();
    let ranked = search(
        db.pool(),
        &SearchRequest {
            query: "cache policy store".into(),
            files: vec!["src/store.rs".into()],
            limit: 5,
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(ranked[0].memory.id, named.id);
    assert!(ranked.iter().any(|hit| hit.memory.id == other.id));
    let plain = search(
        db.pool(),
        &SearchRequest {
            query: "cache policy store".into(),
            limit: 5,
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    let boosted = ranked
        .iter()
        .find(|hit| hit.memory.id == named.id)
        .unwrap()
        .score;
    let unboosted = plain
        .iter()
        .find(|hit| hit.memory.id == named.id)
        .unwrap()
        .score;
    assert!(boosted > unboosted);
}

#[tokio::test]
async fn selected_activity_is_an_engram_and_history_archive_sees_it() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    sqlx::query(
        "CREATE TABLE ax_activity_event (event_id TEXT PRIMARY KEY, operation TEXT, summary TEXT, timestamp_ms INTEGER)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO ax_activity_event (event_id, operation, summary, timestamp_ms) VALUES ('evt-1', 'sync', 'indexed the crate', 10)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    assert_eq!(capture_activity_engrams(db.pool()).await.unwrap(), 1);
    assert_eq!(capture_activity_engrams(db.pool()).await.unwrap(), 0);
    let entries = ax_memory::history(
        db.pool(),
        dir.path(),
        &ax_memory::HistoryQuery {
            query: "indexed the crate".into(),
            since_ms: None,
            limit: 10,
            archive: true,
        },
    )
    .await
    .unwrap();
    assert!(entries.iter().any(|entry| entry.kind == "activity"));
}

#[tokio::test]
async fn a_common_title_word_does_not_hijack_preflight() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let row = remember(
        db.pool(),
        note(
            "The cache policy",
            "Keep the local sqlite file beside the store.",
            vec![],
        ),
    )
    .await
    .unwrap();

    let injected = recall_for_prompt(db.pool(), "the weather today", 3)
        .await
        .unwrap();
    assert!(
        injected
            .iter()
            .all(|hit| hit.memory.id != row.id || hit.score < 100.0),
        "a stop word from the title outranked recall: {injected:?}"
    );
    let by_word = search(
        db.pool(),
        &SearchRequest {
            cue: Some("cache".into()),
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(by_word[0].memory.id, row.id);
    let stop = search(
        db.pool(),
        &SearchRequest {
            cue: Some("the".into()),
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert!(stop.is_empty());
}

#[tokio::test]
async fn graph_deltas_keep_every_change_and_a_removal() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    for n in 0..51 {
        sqlx::query(
            "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
             VALUES (?, 'function', ?, ?, 'src/turns.rs', 'rust', 1, 2, 0, 1, 'fn old()', 1)",
        )
        .bind(format!("sym-{n}"))
        .bind(format!("fn_{n}"))
        .bind(format!("turns::fn_{n}"))
        .execute(db.pool())
        .await
        .unwrap();
    }
    assert_eq!(capture_graph_episodes(db.pool()).await.unwrap(), 0);
    sqlx::query("UPDATE nodes SET signature = 'fn new()'")
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(capture_graph_episodes(db.pool()).await.unwrap(), 51);
    sqlx::query("DELETE FROM nodes WHERE id = 'sym-0'")
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(capture_graph_episodes(db.pool()).await.unwrap(), 1);
    let removed = search(
        db.pool(),
        &SearchRequest {
            query: "removed function fn_0".into(),
            depth: Depth::Archive,
            limit: 60,
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert!(removed
        .iter()
        .any(|hit| hit.memory.title.contains("removed")));
}

#[tokio::test]
async fn a_foreign_embedding_is_not_inserted_into_the_vector_index() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let other = remember(
        db.pool(),
        note(
            "Foreign space",
            "This vector belongs to another model.",
            vec![],
        ),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE memories SET tier = 't3', embedding_model = 'other-model' WHERE id = ?")
        .bind(&other.id)
        .execute(db.pool())
        .await
        .unwrap();
    ax_memory::brain::index_archive_vector(db.pool(), &other.id)
        .await
        .unwrap();
    sqlx::query("UPDATE memories SET embedding_model = 'hash-v1' WHERE id = ?")
        .bind(&other.id)
        .execute(db.pool())
        .await
        .unwrap();
    ax_memory::brain::index_archive_vector(db.pool(), &other.id)
        .await
        .unwrap();
    sqlx::query("UPDATE memories SET embedding_model = 'other-model' WHERE id = ?")
        .bind(&other.id)
        .execute(db.pool())
        .await
        .unwrap();
    ax_memory::brain::index_archive_vector(db.pool(), &other.id)
        .await
        .unwrap();
    let indexed: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory_vec_map WHERE memory_id = ?")
            .bind(&other.id)
            .fetch_one(db.pool())
            .await
            .unwrap_or(0);
    assert_eq!(indexed, 0);
}

fn archive(query: &str) -> SearchRequest {
    SearchRequest {
        query: query.into(),
        depth: Depth::Archive,
        limit: 10,
        ..SearchRequest::default()
    }
}

async fn tier_of(pool: &sqlx::SqlitePool, id: &str) -> String {
    sqlx::query_scalar("SELECT tier FROM memories WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn git_and_folder_docs_are_archive_and_stay_out_of_preflight() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let git = remember(
        db.pool(),
        RememberInput {
            title: "Keep the archive commit".into(),
            body: "The archive commit stays searchable.".into(),
            kind: Some("git".into()),
            tags: vec!["git".into()],
            files: vec!["src/archive.rs".into()],
            source: Some("git".into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(tier_of(db.pool(), &git.id).await, "t3");

    let notes = dir.path().join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    std::fs::write(
        notes.join("kafka.md"),
        "# Kafka retention\n\nKafka retention is seven days.",
    )
    .unwrap();
    ax_memory::sync_folder(db.pool(), "notes", &notes)
        .await
        .unwrap();
    let doc_tier: String = sqlx::query_scalar("SELECT tier FROM memories WHERE kind = 'doc'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(doc_tier, "t3");

    let hot = recall(db.pool(), "archive commit kafka retention", 10)
        .await
        .unwrap();
    assert!(hot
        .iter()
        .all(|hit| hit.memory.kind != "git" && hit.memory.kind != "doc"));
    let injected = recall_for_prompt(db.pool(), "archive commit kafka retention", 5)
        .await
        .unwrap();
    assert!(injected
        .iter()
        .all(|hit| hit.memory.kind != "git" && hit.memory.kind != "doc"));

    let deep = search(db.pool(), &archive("archive commit")).await.unwrap();
    assert!(deep
        .iter()
        .any(|hit| hit.memory.id == git.id && hit.memory.tier == "t3"));
    let docs = search(db.pool(), &archive("kafka retention"))
        .await
        .unwrap();
    assert!(docs
        .iter()
        .any(|hit| hit.memory.kind == "doc" && hit.memory.tier == "t3"));
}

#[tokio::test]
async fn a_git_capture_lands_in_the_archive() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/note.txt"), "hello\n").unwrap();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .status()
            .unwrap();
        assert!(status.success(), "{args:?}");
    };
    git(&["init"]);
    git(&["add", "src/note.txt"]);
    git(&[
        "commit",
        "-m",
        "Keep sqlite-vec because the archive stays local",
    ]);
    let db = open_db(dir.path()).await;
    let result = ax_memory::capture_git_history(db.pool(), &repo, 10)
        .await
        .unwrap();
    assert_eq!(result.captured, 1);
    let tier: String = sqlx::query_scalar("SELECT tier FROM memories WHERE kind = 'git'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(tier, "t3");
    let injected = recall_for_prompt(db.pool(), "sqlite-vec archive", 5)
        .await
        .unwrap();
    assert!(injected.iter().all(|hit| hit.memory.kind != "git"));
}

#[tokio::test]
async fn archive_search_hops_once_over_link_file_and_symbol() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let make = |title: &str, body: &str, files: Vec<String>| RememberInput {
        title: title.into(),
        body: body.into(),
        kind: Some("graph_diff".into()),
        tags: vec![],
        files,
        source: Some("test".into()),
    };
    let anchor = remember(
        db.pool(),
        make(
            "Xylophone protocol 8841",
            "Xylophone protocol 8841 is the archive anchor.",
            vec!["src/anchor.rs".into()],
        ),
    )
    .await
    .unwrap();
    let linked = remember(
        db.pool(),
        make("Plain bricks", "Plain bricks sit in a pile.", vec![]),
    )
    .await
    .unwrap();
    let by_file = remember(
        db.pool(),
        make(
            "Other bricks",
            "Other bricks sit elsewhere.",
            vec!["src/anchor.rs".into()],
        ),
    )
    .await
    .unwrap();
    let by_symbol = remember(
        db.pool(),
        make("Quiet bricks", "Quiet bricks have no shared file.", vec![]),
    )
    .await
    .unwrap();
    let stranger = remember(
        db.pool(),
        make("Distant quarry", "Distant quarry has no link.", vec![]),
    )
    .await
    .unwrap();
    for id in [&linked.id, &by_file.id, &by_symbol.id, &stranger.id] {
        sqlx::query(
            "DELETE FROM memory_vec WHERE rowid IN (SELECT rowid FROM memory_vec_map WHERE memory_id = ?)",
        )
        .bind(id)
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query("DELETE FROM memory_vec_map WHERE memory_id = ?")
            .bind(id)
            .execute(db.pool())
            .await
            .unwrap();
        sqlx::query("UPDATE memories SET embedding = NULL WHERE id = ?")
            .bind(id)
            .execute(db.pool())
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO memory_links (from_id, to_id, kind) VALUES (?, ?, 'source')")
        .bind(&anchor.id)
        .bind(&linked.id)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        "UPDATE memory_cues SET source = 'symbol' WHERE memory_id = ? AND cue = 'xylophone'",
    )
    .bind(&anchor.id)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO memory_cues (cue, memory_id, weight, source) VALUES ('xylophone', ?, 1.0, 'symbol')",
    )
    .bind(&by_symbol.id)
    .execute(db.pool())
    .await
    .unwrap();

    let hits = search(db.pool(), &archive("xylophone protocol 8841"))
        .await
        .unwrap();
    let ids: Vec<&str> = hits.iter().map(|hit| hit.memory.id.as_str()).collect();
    assert!(ids.contains(&anchor.id.as_str()), "{ids:?}");
    assert!(ids.contains(&linked.id.as_str()), "{ids:?}");
    assert!(ids.contains(&by_file.id.as_str()), "{ids:?}");
    assert!(ids.contains(&by_symbol.id.as_str()), "{ids:?}");
    assert!(!ids.contains(&stranger.id.as_str()), "{ids:?}");

    let hot = recall(db.pool(), "xylophone protocol 8841", 10)
        .await
        .unwrap();
    assert!(hot.iter().all(|hit| hit.memory.id != linked.id));
}

#[tokio::test]
async fn an_open_symbol_on_the_cue_set_raises_the_score() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
         VALUES ('sym-1', 'function', 'Tokenizer', 'Tokenizer', 'src/tok.rs', 'rust', 1, 4, 0, 1, 'fn Tokenizer', 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    let row = remember_with_triggers(
        db.pool(),
        note(
            "Budget choice",
            "The budget choice stays in long-term memory.",
            vec![],
        ),
        &["tokenizer".into()],
    )
    .await
    .unwrap();
    let plain = search(
        db.pool(),
        &SearchRequest {
            query: "budget choice".into(),
            limit: 5,
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    let boosted = search(
        db.pool(),
        &SearchRequest {
            query: "budget choice".into(),
            files: vec!["src/tok.rs".into()],
            limit: 5,
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    let plain_score = plain
        .iter()
        .find(|hit| hit.memory.id == row.id)
        .unwrap()
        .score;
    let boosted_score = boosted
        .iter()
        .find(|hit| hit.memory.id == row.id)
        .unwrap()
        .score;
    assert!(
        (boosted_score - plain_score - 20.0).abs() < 0.001,
        "{plain_score} -> {boosted_score}"
    );
}

#[tokio::test]
async fn a_listed_file_registers_its_symbol_as_a_cue() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
         VALUES ('sym-2', 'function', 'Tokenizer', 'Tokenizer', 'src/tok.rs', 'rust', 1, 4, 0, 1, 'fn Tokenizer', 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    let row = remember(
        db.pool(),
        note(
            "Symbol file",
            "The symbol file is listed on the memory.",
            vec!["src/tok.rs".into()],
        ),
    )
    .await
    .unwrap();
    let source: Option<String> = sqlx::query_scalar(
        "SELECT source FROM memory_cues WHERE memory_id = ? AND cue = 'tokenizer'",
    )
    .bind(&row.id)
    .fetch_optional(db.pool())
    .await
    .unwrap();
    assert_eq!(source.as_deref(), Some("symbol"));
}

#[tokio::test]
async fn a_commit_that_states_a_decision_becomes_a_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let git = remember(
        db.pool(),
        RememberInput {
            title: "Keep sqlite-vec".into(),
            body: "We decided to keep sqlite-vec because it stays local.".into(),
            kind: Some("git".into()),
            tags: vec!["git".into()],
            files: vec![],
            source: Some("git".into()),
        },
    )
    .await
    .unwrap();
    let created = consolidate(db.pool()).await.unwrap();
    assert_eq!(created, 1);
    let status: String = sqlx::query_scalar(
        "SELECT status FROM memories WHERE kind = 'decision' AND source = 'consolidation'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(status, "candidate");
    let links: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory_links WHERE to_id = ? AND kind = 'source'")
            .bind(&git.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(links, 1);
    let candidate: String = sqlx::query_scalar(
        "SELECT id FROM memories WHERE kind = 'decision' AND source = 'consolidation'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    let injected = recall_for_prompt(db.pool(), "sqlite-vec stays local", 5)
        .await
        .unwrap();
    assert!(injected.iter().all(|hit| hit.memory.id != candidate));
}

#[tokio::test]
async fn a_foreign_model_is_reembedded_onto_the_active_model() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let row = remember(
        db.pool(),
        note(
            "Foreign model row",
            "This row waits for the active embedding model.",
            vec![],
        ),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE memories SET tier = 't3', embedding_model = 'other-model' WHERE id = ?")
        .bind(&row.id)
        .execute(db.pool())
        .await
        .unwrap();
    let updated = ax_memory::brain::reembed_stale(db.pool(), 10)
        .await
        .unwrap();
    assert_eq!(updated, 1);
    let model: String = sqlx::query_scalar("SELECT embedding_model FROM memories WHERE id = ?")
        .bind(&row.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert!(model == "hash-v1" || model == "minilm-256", "{model}");
    let indexed: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory_vec_map WHERE memory_id = ?")
            .bind(&row.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(indexed, 1);
}

#[tokio::test]
async fn migration_26_moves_git_and_docs_into_the_archive() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let git = remember(
        db.pool(),
        RememberInput {
            title: "Old git row".into(),
            body: "An old git row left on the long-term default.".into(),
            kind: Some("git".into()),
            tags: vec![],
            files: vec![],
            source: Some("git".into()),
        },
    )
    .await
    .unwrap();
    let doc = remember(
        db.pool(),
        RememberInput {
            title: "Old folder note".into(),
            body: "An old folder note left on the long-term default.".into(),
            kind: Some("doc".into()),
            tags: vec![],
            files: vec![],
            source: Some("folder".into()),
        },
    )
    .await
    .unwrap();
    let decision = remember(
        db.pool(),
        note("Stay long-term", "A decision stays long-term.", vec![]),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE memories SET tier = 't2' WHERE id IN (?, ?)")
        .bind(&git.id)
        .bind(&doc.id)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM schema_versions WHERE version >= 26")
        .execute(db.pool())
        .await
        .unwrap();
    ax_db::migrations::run_migrations(db.pool(), 25)
        .await
        .unwrap();
    assert_eq!(tier_of(db.pool(), &git.id).await, "t3");
    assert_eq!(tier_of(db.pool(), &doc.id).await, "t3");
    assert_eq!(tier_of(db.pool(), &decision.id).await, "t2");
}

#[tokio::test]
async fn archive_hits_resolve_through_an_index() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let row = remember(
        db.pool(),
        note("Queue retention", "Retention notes.", vec![]),
    )
    .await
    .unwrap();
    ax_memory::brain::index_archive_vector(db.pool(), &row.id)
        .await
        .unwrap();
    let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(&format!(
        "EXPLAIN QUERY PLAN {}",
        ax_memory::brain::ARCHIVE_HIT_SQL
    ))
    .bind("[1]")
    .fetch_all(db.pool())
    .await
    .unwrap();
    let steps: Vec<&str> = plan.iter().map(|(_, _, _, step)| step.as_str()).collect();
    assert!(
        steps.iter().all(|step| !step.starts_with("SCAN m")
            && !step.starts_with("SCAN map")
            && !step.contains("idx_memories_tier")),
        "every hit must resolve from the map index, not the tier: {steps:?}"
    );
}
