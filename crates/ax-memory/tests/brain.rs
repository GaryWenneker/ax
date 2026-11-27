//! Brain-scale memory: cues, demotion, archive ANN, episodes, and consolidation.

use ax_memory::{
    capture_activity_engrams, capture_graph_episodes, capture_graph_episodes_on, confirm_memory,
    consolidate, memories_for_preflight, prune_turns, recall, recall_for_prompt, remember,
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
async fn archive_keyword_search_keeps_a_foreign_model_row() {
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
    assert!(found.iter().any(|hit| hit.memory.id == other.id));
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
async fn graph_capture_rolls_back_as_one_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    for (id, name) in [("sym-1", "save_turn"), ("sym-2", "load_turn")] {
        sqlx::query(
            "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
             VALUES (?, 'function', ?, ?, 'src/turns.rs', 'rust', 1, 4, 0, 1, 'fn old()', 1)",
        )
        .bind(id)
        .bind(name)
        .bind(format!("turns::{name}"))
        .execute(db.pool())
        .await
        .unwrap();
    }
    assert_eq!(capture_graph_episodes(db.pool()).await.unwrap(), 0);
    sqlx::query("UPDATE nodes SET signature = 'fn new()'")
        .execute(db.pool())
        .await
        .unwrap();

    let mut conn = db.pool().acquire().await.unwrap();
    sqlx::query("BEGIN").execute(&mut *conn).await.unwrap();
    let written = capture_graph_episodes_on(&mut conn).await.unwrap();
    assert_eq!(written, 2);
    sqlx::query("ROLLBACK").execute(&mut *conn).await.unwrap();
    drop(conn);

    assert_eq!(capture_graph_episodes(db.pool()).await.unwrap(), 2);
    let episodes: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE kind = 'graph_diff'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(episodes, 2);
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
        sqlx::query("DELETE FROM schema_versions WHERE version >= 29")
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
    let row = remember(
        db.pool(),
        RememberInput {
            title: "Use cues".into(),
            body: "We decided to fetch by cue because names are exact.".into(),
            kind: Some("decision".into()),
            tags: vec![],
            files: vec![],
            source: Some("consolidation".into()),
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE memories SET status = 'candidate' WHERE id = ?")
        .bind(&row.id)
        .execute(db.pool())
        .await
        .unwrap();
    let before = recall_for_prompt(db.pool(), "decided to fetch by cue", 5)
        .await
        .unwrap();
    assert!(before
        .iter()
        .all(|hit| hit.memory.source != "consolidation"));
    let candidate = row.id;
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
async fn preflight_finds_durable_title_despite_noisy_turns() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let durable = remember(
        db.pool(),
        note(
            "Music DNA deterministic replay contract and regression fixture",
            "Reuse the locked Music DNA snapshot for identical song requests.",
            vec![],
        ),
    )
    .await
    .unwrap();
    for n in 0..80 {
        save_turn(
            db.pool(),
            &TurnRecord {
                id: format!("noise-turn-{n:02}"),
                title: format!("continue with fixture and contract {n}"),
                body: "and fixture regression contract deterministic notes from a turn".into(),
                files: vec![],
            },
            NOW,
        )
        .await
        .unwrap();
    }

    let injected = recall_for_prompt(
        db.pool(),
        "Music DNA deterministic replay contract and regression fixture",
        5,
    )
    .await
    .unwrap();
    assert!(
        injected.iter().any(|hit| hit.memory.id == durable.id),
        "durable title must survive noisy turns, got {injected:?}"
    );
}

#[tokio::test]
async fn preflight_abstains_when_only_a_single_query_term_overlaps() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    remember(
        db.pool(),
        note(
            "Blossom Spotify OAuth HTTPS callback",
            "Independent smoke observed 404 on /callback, so the user must pull latest main.",
            vec![],
        ),
    )
    .await
    .unwrap();
    remember(
        db.pool(),
        note(
            "Music DNA deterministic replay contract",
            "Regression fixture locks hexadecimal fingerprints for arrangement analysis.",
            vec![],
        ),
    )
    .await
    .unwrap();

    let lute = recall_for_prompt(
        db.pool(),
        "medieval lute tablature for imaginary symphony #404",
        5,
    )
    .await
    .unwrap();
    assert!(
        lute.is_empty(),
        "single-token 404 overlap must abstain, got {lute:?}"
    );

    let cake = recall_for_prompt(db.pool(), "bake a cake using only hexadecimal integers", 5)
        .await
        .unwrap();
    assert!(
        cake.is_empty(),
        "single-token hexadecimal overlap must abstain, got {cake:?}"
    );
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
async fn a_commit_that_states_a_decision_stays_a_commit() {
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
    assert_eq!(created, 0);
    let decisions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memories WHERE kind = 'decision' AND source = 'consolidation'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(decisions, 0);
    let links: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory_links WHERE to_id = ? AND kind = 'source'")
            .bind(&git.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(links, 0);
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
    assert_eq!(model, "hash-v1");
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
async fn migration_31_rejects_a_heuristic_candidate_and_confirm_restores_it() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let candidate = remember(
        db.pool(),
        RememberInput {
            title: "Candidate: keep it".into(),
            body: "We decided to keep it.".into(),
            kind: Some("decision".into()),
            tags: vec![],
            files: vec![],
            source: Some("consolidation".into()),
        },
    )
    .await
    .unwrap();
    let kept = remember(db.pool(), note("Kept decision", "A real decision.", vec![]))
        .await
        .unwrap();
    sqlx::query("UPDATE memories SET status = 'candidate' WHERE id = ?")
        .bind(&candidate.id)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO memory_links (from_id, to_id, kind) VALUES (?, ?, 'supersedes')")
        .bind(&candidate.id)
        .bind(&kept.id)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM schema_versions WHERE version >= 31")
        .execute(db.pool())
        .await
        .unwrap();
    ax_db::migrations::run_migrations(db.pool(), 30)
        .await
        .unwrap();
    let status: String = sqlx::query_scalar("SELECT status FROM memories WHERE id = ?")
        .bind(&candidate.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(status, "rejected");
    let kept_status: String = sqlx::query_scalar("SELECT status FROM memories WHERE id = ?")
        .bind(&kept.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(kept_status, "active");
    let kind: String =
        sqlx::query_scalar("SELECT kind FROM memory_links WHERE from_id = ? AND to_id = ?")
            .bind(&candidate.id)
            .bind(&kept.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(kind, "revoked_supersedes");
    assert!(confirm_memory(db.pool(), &candidate.id).await.unwrap());
    let restored: String = sqlx::query_scalar("SELECT status FROM memories WHERE id = ?")
        .bind(&candidate.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(restored, "active");
    let restored_kind: String =
        sqlx::query_scalar("SELECT kind FROM memory_links WHERE from_id = ? AND to_id = ?")
            .bind(&candidate.id)
            .bind(&kept.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(restored_kind, "supersedes");
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

#[tokio::test]
async fn reindexing_an_archive_memory_keeps_one_vector_row() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let row = remember(
        db.pool(),
        note("Queue retention", "Retention notes.", vec![]),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE memories SET tier = 't3' WHERE id = ?")
        .bind(&row.id)
        .execute(db.pool())
        .await
        .unwrap();
    for _ in 0..2 {
        ax_memory::brain::index_archive_vector(db.pool(), &row.id)
            .await
            .unwrap();
    }
    let vectors: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_vec")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(vectors, 1);
    let nearest = ax_memory::brain::knn(db.pool(), "Queue retention\nRetention notes.", 1)
        .await
        .unwrap();
    assert_eq!(nearest, vec![row.id]);
}

#[tokio::test]
async fn a_cue_from_two_sources_keeps_the_stronger_source() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
         VALUES ('sym-3', 'struct', 'Ledger', 'Ledger', 'src/queue.rs', 'rust', 1, 4, 0, 1, 'struct Ledger', 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    let row = remember(
        db.pool(),
        RememberInput {
            tags: vec!["retention".into()],
            ..note(
                "Queue retention",
                "Retention for the queue and the ledger.",
                vec!["src/queue.rs".into(), "src/ledger.rs".into()],
            )
        },
    )
    .await
    .unwrap();
    for (cue, expected) in [("retention", "tag"), ("queue", "title"), ("ledger", "file")] {
        let source: Option<String> =
            sqlx::query_scalar("SELECT source FROM memory_cues WHERE memory_id = ? AND cue = ?")
                .bind(&row.id)
                .bind(cue)
                .fetch_optional(db.pool())
                .await
                .unwrap();
        assert_eq!(source.as_deref(), Some(expected), "{cue}");
    }
}

#[tokio::test]
async fn full_text_finds_a_rare_word_the_vector_misses() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let filler: Vec<String> = (0..300).map(|i| format!("filler{i}")).collect();
    let row = remember(
        db.pool(),
        note(
            "Release checklist",
            &format!("{} zephyrine", filler.join(" ")),
            vec![],
        ),
    )
    .await
    .unwrap();
    let (query, _) = ax_memory::embed::embed_labeled("zephyrine");
    let (stored, _) = ax_memory::embed::embed_labeled(&format!(
        "Release checklist\n{} zephyrine",
        filler.join(" ")
    ));
    assert!(
        ax_memory::embed::cosine(&query, &stored) <= ax_memory::embed::MIN_VECTOR_SIMILARITY,
        "the vector must miss this row for the test to prove full text"
    );
    let hits = search(
        db.pool(),
        &SearchRequest {
            query: "zephyrine".into(),
            limit: 5,
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(
        hits.iter()
            .map(|h| h.memory.id.as_str())
            .collect::<Vec<_>>(),
        vec![row.id.as_str()]
    );
}

#[tokio::test]
async fn a_recurring_change_keeps_the_first_id_and_suffixes_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
         VALUES ('sym-4', 'function', 'flush', 'queue::flush', 'src/queue.rs', 'rust', 1, 4, 0, 1, 'fn flush()', 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    capture_graph_episodes(db.pool()).await.unwrap();
    for signature in ["fn flush(now: i64)", "fn flush()", "fn flush(now: i64)"] {
        sqlx::query("UPDATE nodes SET signature = ? WHERE id = 'sym-4'")
            .bind(signature)
            .execute(db.pool())
            .await
            .unwrap();
        capture_graph_episodes(db.pool()).await.unwrap();
    }
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM memories WHERE kind = 'graph_diff' ORDER BY rowid")
            .fetch_all(db.pool())
            .await
            .unwrap();
    assert_eq!(ids.len(), 3);
    assert!(!ids[0].ends_with("-1"), "{ids:?}");
    assert_eq!(ids[2], format!("{}-2", ids[0]));
}

#[tokio::test]
async fn a_wide_recall_returns_every_match_up_to_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    for i in 0..60 {
        remember(
            db.pool(),
            note(
                &format!("Retention note {i}"),
                &format!("Retention rule number {i}."),
                vec![],
            ),
        )
        .await
        .unwrap();
    }
    let hits = search(
        db.pool(),
        &SearchRequest {
            query: "retention".into(),
            limit: 100,
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(hits.len(), 60);
}

#[tokio::test]
async fn a_trigger_behind_non_injectable_rows_is_still_returned() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    for n in 0..8 {
        let id = format!("a-turn-{n:02}");
        sqlx::query(
            "INSERT INTO memories (id, kind, title, body, created_at, updated_at, tier, status, enabled)
             VALUES (?, 'turn', 'Turn', 'Turn', 1, 1, 't1', 'active', 1)",
        )
        .bind(&id)
        .execute(db.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO memory_cues (memory_id, cue, source, weight) VALUES (?, 'zulu-cue', 'trigger', 1.0)",
        )
        .bind(&id)
        .execute(db.pool())
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO memories (id, kind, title, body, created_at, updated_at, tier, status, enabled)
         VALUES ('z-hot', 'decision', 'Hot', 'Hot', 1, 1, 't1', 'active', 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO memory_cues (memory_id, cue, source, weight) VALUES ('z-hot', 'zulu-cue', 'trigger', 1.0)",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let hits = ax_memory::brain::prompt_cues(db.pool(), "please zulu-cue now", 1)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].memory.id, "z-hot");
}

#[tokio::test]
async fn a_keyword_hit_is_not_hidden_by_its_embedding_model() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let row = remember(
        db.pool(),
        note("Visible word", "The zephyrword stays searchable.", vec![]),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE memories SET embedding_model = 'minilm' WHERE id = ?")
        .bind(&row.id)
        .execute(db.pool())
        .await
        .unwrap();

    let hits = recall(db.pool(), "zephyrword", 5).await.unwrap();
    assert!(hits.iter().any(|hit| hit.memory.id == row.id), "{hits:?}");
}

#[tokio::test]
async fn a_similar_decision_does_not_confirm_the_candidate() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let body = "We decided to keep sqlite-vec because it stays local.";
    remember(
        db.pool(),
        RememberInput {
            title: "Keep sqlite-vec".into(),
            body: body.into(),
            kind: Some("decision".into()),
            tags: vec![],
            files: vec![],
            source: Some("test".into()),
        },
    )
    .await
    .unwrap();
    remember(
        db.pool(),
        RememberInput {
            title: "Keep sqlite-vec".into(),
            body: body.into(),
            kind: Some("git".into()),
            tags: vec!["git".into()],
            files: vec![],
            source: Some("git".into()),
        },
    )
    .await
    .unwrap();

    let created = consolidate(db.pool()).await.unwrap();
    assert_eq!(created, 0);
    let status: String = sqlx::query_scalar("SELECT status FROM memories WHERE kind = 'decision'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(status, "active");
    let links: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory_links WHERE kind = 'supersedes'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(links, 0);
}

#[tokio::test]
async fn automatic_cues_skip_change_verbs_kinds_and_primitive_types() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let row = remember_with_triggers(
        db.pool(),
        note(
            "Updated the function string parser",
            "Keep the parser.",
            vec![],
        ),
        &["updated".into()],
    )
    .await
    .unwrap();
    let cues: Vec<(String, String)> =
        sqlx::query_as("SELECT cue, source FROM memory_cues WHERE memory_id = ? ORDER BY cue")
            .bind(&row.id)
            .fetch_all(db.pool())
            .await
            .unwrap();
    assert!(cues.iter().any(|(cue, _)| cue == "parser"), "{cues:?}");
    assert!(
        cues.iter()
            .all(|(cue, _)| cue != "function" && cue != "string"),
        "{cues:?}"
    );
    assert_eq!(
        cues.iter()
            .find(|(cue, _)| cue == "updated")
            .map(|(_, source)| source.as_str()),
        Some("trigger"),
        "{cues:?}"
    );
}

#[tokio::test]
async fn a_graph_change_registers_only_the_changed_symbol() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    for (id, name) in [("sym-flush", "flush"), ("sym-enqueue", "enqueue")] {
        sqlx::query(
            "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
             VALUES (?, 'function', ?, ?, 'src/queue.rs', 'rust', 1, 4, 0, 1, ?, 1)",
        )
        .bind(id)
        .bind(name)
        .bind(format!("queue::{name}"))
        .bind(format!("fn {name}()"))
        .execute(db.pool())
        .await
        .unwrap();
    }
    capture_graph_episodes(db.pool()).await.unwrap();
    sqlx::query("UPDATE nodes SET signature = 'fn flush(now: i64)' WHERE id = 'sym-flush'")
        .execute(db.pool())
        .await
        .unwrap();
    capture_graph_episodes(db.pool()).await.unwrap();

    let memory_id: String = sqlx::query_scalar(
        "SELECT id FROM memories WHERE kind = 'graph_diff' AND title LIKE '%flush%' ORDER BY rowid DESC LIMIT 1",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    let cues: Vec<String> =
        sqlx::query_scalar("SELECT cue FROM memory_cues WHERE memory_id = ? ORDER BY cue")
            .bind(&memory_id)
            .fetch_all(db.pool())
            .await
            .unwrap();
    assert!(cues.iter().any(|cue| cue == "flush"), "{cues:?}");
    assert!(cues.iter().all(|cue| cue != "enqueue"), "{cues:?}");
}

#[tokio::test]
async fn an_overused_automatic_cue_is_not_added_again() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    for n in 0..32 {
        remember(
            db.pool(),
            note(&format!("commonword shelf {n}"), "A shelf.", vec![]),
        )
        .await
        .unwrap();
    }
    let plain = remember(
        db.pool(),
        note("commonword finale", "The last shelf.", vec![]),
    )
    .await
    .unwrap();
    let automatic: Option<String> = sqlx::query_scalar(
        "SELECT source FROM memory_cues WHERE memory_id = ? AND cue = 'commonword'",
    )
    .bind(&plain.id)
    .fetch_optional(db.pool())
    .await
    .unwrap();
    assert_eq!(automatic, None);

    let triggered = remember_with_triggers(
        db.pool(),
        note("Plain finale", "No shared title word.", vec![]),
        &["commonword".into()],
    )
    .await
    .unwrap();
    let source: Option<String> = sqlx::query_scalar(
        "SELECT source FROM memory_cues WHERE memory_id = ? AND cue = 'commonword'",
    )
    .bind(&triggered.id)
    .fetch_optional(db.pool())
    .await
    .unwrap();
    assert_eq!(source.as_deref(), Some("trigger"));
}

#[tokio::test]
async fn a_rare_title_cue_outranks_a_common_one_and_a_trigger_stays_first() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    for n in 0..8 {
        remember(
            db.pool(),
            note(&format!("commonword shelf {n}"), "A shelf.", vec![]),
        )
        .await
        .unwrap();
    }
    let rare = remember(db.pool(), note("rareword keeper", "One keeper.", vec![]))
        .await
        .unwrap();
    let trigger = remember_with_triggers(
        db.pool(),
        note("Plain note", "No shared title.", vec![]),
        &["commonword".into()],
    )
    .await
    .unwrap();
    let hits = ax_memory::brain::prompt_cues(db.pool(), "rareword commonword", 5)
        .await
        .unwrap();
    let ids: Vec<&str> = hits.iter().map(|hit| hit.memory.id.as_str()).collect();
    assert_eq!(ids.first().copied(), Some(trigger.id.as_str()), "{ids:?}");
    let rare_at = ids
        .iter()
        .position(|id| *id == rare.id.as_str())
        .unwrap_or_else(|| panic!("{ids:?}"));
    let common_at = ids
        .iter()
        .position(|id| *id != trigger.id.as_str() && *id != rare.id.as_str())
        .unwrap_or_else(|| panic!("{ids:?}"));
    assert!(rare_at < common_at, "{ids:?}");
}

#[test]
fn suggested_triggers_prefer_title_nouns_then_anchors_then_prompt_words() {
    let got = ax_memory::suggested_triggers(
        "Keep sqlite-vec because the archive stays local",
        &["crates/ax-memory/src/brain.rs".to_string()],
        "the user asked to remember the vector index",
        &["sqlite-vec".to_string()],
    );
    assert_eq!(
        got,
        vec!["archive", "local", "ax-memory", "brain", "vector", "index"]
    );
    assert!(ax_memory::suggested_triggers("Update the function", &[], "the", &[]).is_empty());
}

#[tokio::test]
async fn graph_episode_ignores_line_shifts() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    for n in 0..3 {
        sqlx::query(
            "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
             VALUES (?, 'function', ?, ?, 'src/shift.rs', 'rust', ?, ?, 0, 1, 'fn f()', 1)",
        )
        .bind(format!("shift-{n}"))
        .bind(format!("f_{n}"))
        .bind(format!("shift::f_{n}"))
        .bind(10 * n + 1)
        .bind(10 * n + 5)
        .execute(db.pool())
        .await
        .unwrap();
    }
    assert_eq!(capture_graph_episodes(db.pool()).await.unwrap(), 0);
    // An import added at the top moves every symbol down one line.
    sqlx::query("UPDATE nodes SET start_line = start_line + 1, end_line = end_line + 1")
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(
        capture_graph_episodes(db.pool()).await.unwrap(),
        0,
        "moving lines is not a change"
    );
    let episodes: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE kind = 'graph_diff'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(episodes, 0);
    db.close().await;
}

#[tokio::test]
async fn graph_episode_accepts_fingerprints_that_still_carry_line_numbers() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
         VALUES ('legacy-1', 'function', 'run', 'run', 'src/run.rs', 'rust', 3, 9, 0, 1, 'fn run(a: u8 | u16)', 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    // State written before line numbers left the fingerprint.
    sqlx::query(
        "INSERT INTO graph_symbol_state (symbol_id, fingerprint)
         VALUES ('legacy-1', 'run|function|src/run.rs|fn run(a: u8 | u16)|1|7')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    assert_eq!(capture_graph_episodes(db.pool()).await.unwrap(), 0);
    let stored: String = sqlx::query_scalar(
        "SELECT fingerprint FROM graph_symbol_state WHERE symbol_id = 'legacy-1'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(stored, "run|function|src/run.rs|fn run(a: u8 | u16)");
    db.close().await;
}
