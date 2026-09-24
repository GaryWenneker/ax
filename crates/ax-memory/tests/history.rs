//! Turn history (docs/specs/turn-memory-outcomes.md): related turns in preflight (R1-R3),
//! "when did I change X" (H1-H4).

use std::path::Path;
use std::process::Command;

use ax_memory::{
    format_history, format_turn_history_block, format_when, history, history_entry,
    is_history_question, related_turns, remember, save_turn, HistoryQuery, RememberInput,
    TurnRecord, OUTCOME_MARKER,
};

const DAY_MS: i64 = 86_400_000;
/// 2026-09-21T14:13:20Z.
const NOW: i64 = 1_790_000_000_000;

async fn open_db(dir: &Path) -> ax_db::Database {
    ax_db::Database::open(&dir.join("ax.db")).await.unwrap()
}

fn record(id: &str, title: &str, outcome: &str, files: &[&str]) -> TurnRecord {
    let files: Vec<String> = files.iter().map(|f| f.to_string()).collect();
    let mut body = format!("{title}\n\nFiles: {}", files.join(", "));
    if !outcome.is_empty() {
        body.push_str(OUTCOME_MARKER);
        body.push_str(outcome);
    }
    TurnRecord {
        id: id.into(),
        title: title.into(),
        body,
        files,
    }
}

async fn save(db: &ax_db::Database, r: &TurnRecord, at: i64) {
    assert!(save_turn(db.pool(), r, at).await.unwrap());
}

fn ids(rows: &[ax_memory::MemoryRow]) -> Vec<&str> {
    rows.iter().map(|r| r.id.as_str()).collect()
}

const SETTINGS: &str = "crates/ax-web/web-ui/src/pages/Settings.tsx";

#[tokio::test]
async fn r1_turn_that_changed_an_open_file_is_related() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    save(
        &db,
        &record(
            "turn-a",
            "Fix the dropdown",
            "Fixed the dropdown, 11 languages now load.",
            &[SETTINGS],
        ),
        NOW,
    )
    .await;
    save(
        &db,
        &record("turn-b", "Tune the parser", "Done.", &["src/parser.rs"]),
        NOW,
    )
    .await;

    let rows = related_turns(db.pool(), "zzz qqq", &[SETTINGS.to_string()], 3)
        .await
        .unwrap();
    assert_eq!(ids(&rows), vec!["turn-a"]);

    let block = format_turn_history_block(&rows, 1_200);
    assert!(block.starts_with("<ax_turn_history>"), "{block}");
    assert!(block.ends_with("</ax_turn_history>"), "{block}");
    assert!(block.contains(&format_when(NOW)), "{block}");
    assert!(block.contains("Fix the dropdown"), "{block}");
    assert!(
        block.contains("Fixed the dropdown, 11 languages now load."),
        "{block}"
    );
    assert!(block.contains(SETTINGS), "{block}");
}

#[tokio::test]
async fn r1_turn_sharing_two_words_with_the_prompt_is_related() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    save(
        &db,
        &record(
            "turn-a",
            "Rework the policy cache invalidation",
            "Cache is keyed by generation.",
            &["src/cache.rs"],
        ),
        NOW,
    )
    .await;

    let hit = related_turns(db.pool(), "the policy cache is stale again", &[], 3)
        .await
        .unwrap();
    assert_eq!(ids(&hit), vec!["turn-a"]);

    let one_word = related_turns(db.pool(), "a policy question", &[], 3)
        .await
        .unwrap();
    assert!(
        one_word.is_empty(),
        "one shared word is not enough: {one_word:?}"
    );
}

#[tokio::test]
async fn r2_nothing_related_gives_no_block() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    save(
        &db,
        &record("turn-a", "Tune the parser", "Done.", &["src/parser.rs"]),
        NOW,
    )
    .await;
    remember(
        db.pool(),
        RememberInput {
            title: "Settings dropdown decision".into(),
            body: "The settings dropdown lists languages.".into(),
            files: vec![SETTINGS.into()],
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let rows = related_turns(
        db.pool(),
        "settings dropdown languages",
        &[SETTINGS.to_string()],
        3,
    )
    .await
    .unwrap();
    assert!(
        rows.is_empty(),
        "only turn memories are turn history: {rows:?}"
    );
    assert_eq!(format_turn_history_block(&rows, 1_200), "");
}

#[tokio::test]
async fn disabled_turns_are_not_related() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    save(
        &db,
        &record("turn-a", "Fix the dropdown", "Done.", &[SETTINGS]),
        NOW,
    )
    .await;
    ax_memory::set_enabled(db.pool(), "turn-a", false)
        .await
        .unwrap();
    let rows = related_turns(db.pool(), "", &[SETTINGS.to_string()], 3)
        .await
        .unwrap();
    assert!(rows.is_empty(), "{rows:?}");
}

#[tokio::test]
async fn r3_at_most_three_newest_first_within_1200_characters() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let long = "x".repeat(2_000);
    for i in 0..10 {
        let title = format!("Settings change number {i} {}", "y".repeat(400));
        save(
            &db,
            &record(
                &format!("turn-{i}"),
                &title,
                &long,
                &[SETTINGS, "a.rs", "b.rs", "c.rs"],
            ),
            NOW - (10 - i) * DAY_MS,
        )
        .await;
    }

    let rows = related_turns(db.pool(), "", &[SETTINGS.to_string()], 3)
        .await
        .unwrap();
    assert_eq!(ids(&rows), vec!["turn-9", "turn-8", "turn-7"]);

    let block = format_turn_history_block(&rows, 1_200);
    assert!(
        block.chars().count() <= 1_200,
        "{} chars",
        block.chars().count()
    );
    assert!(block.contains("Settings change number 9"), "{block}");
    assert!(
        !block.contains(&"x".repeat(121)),
        "outcome is cut to 120 characters"
    );
    assert!(
        !block.contains("c.rs"),
        "at most 3 files per entry: {block}"
    );
}

#[test]
fn h2_history_questions_are_recognised() {
    assert!(is_history_question(
        "wanneer heb ik de review-taal aangepast?"
    ));
    assert!(is_history_question("Wanneer hebben we de cache veranderd"));
    assert!(is_history_question("When did I change the cache?"));
    assert!(is_history_question("what did I change in Settings.tsx"));
    assert!(is_history_question("wat heb ik aangepast aan de parser"));
    assert!(!is_history_question("fix the cache"));
    assert!(!is_history_question(""));
}

#[test]
fn format_when_is_date_and_minutes() {
    let text = format_when(NOW);
    let shape: Vec<bool> = text.chars().map(|c| c.is_ascii_digit()).collect();
    assert_eq!(text.len(), 16, "{text}");
    assert_eq!(&text[4..5], "-");
    assert_eq!(&text[7..8], "-");
    assert_eq!(&text[10..11], " ");
    assert_eq!(&text[13..14], ":");
    assert!(
        shape
            .iter()
            .enumerate()
            .all(|(i, d)| *d || [4, 7, 10, 13].contains(&i)),
        "{text}"
    );
    assert!(text.starts_with("2026-09-2"), "{text}");
}

fn git(root: &Path, envs: &[(&str, &str)], args: &[&str]) {
    let status = Command::new("git")
        .current_dir(root)
        .envs(envs.iter().copied())
        .args([
            "-c",
            "user.email=t@example.com",
            "-c",
            "user.name=t",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

const LANG_FILE: &str = "crates/ax-core/src/review_language.rs";
/// 2026-09-20T10:00:00Z, one day before `NOW`.
const COMMIT_DATE: &str = "2026-09-20T10:00:00Z";
const COMMIT_MS: i64 = 1_789_898_400_000;

/// A repo with one commit touching `LANG_FILE` and an indexed file + symbol for it.
async fn project() -> (tempfile::TempDir, ax_db::Database) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &[], &["init", "-q"]);
    std::fs::write(root.join("README.md"), "readme\n").unwrap();
    git(root, &[], &["add", "."]);
    git(root, &[], &["commit", "-q", "-m", "initial"]);
    std::fs::create_dir_all(root.join("crates/ax-core/src")).unwrap();
    std::fs::write(
        root.join(LANG_FILE),
        "pub fn resolve_review_language() {}\n",
    )
    .unwrap();
    git(root, &[], &["add", "."]);
    let dates = [
        ("GIT_AUTHOR_DATE", COMMIT_DATE),
        ("GIT_COMMITTER_DATE", COMMIT_DATE),
    ];
    git(
        root,
        &dates,
        &["commit", "-q", "-m", "Add review comment language setting"],
    );

    let db = open_db(root).await;
    sqlx::query(
        "INSERT INTO files (path, content_hash, language, size, modified_at, indexed_at) VALUES (?, 'h', 'rust', 1, 0, 0)",
    )
    .bind(LANG_FILE)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, updated_at) \
         VALUES ('n1', 'function', 'resolve_review_language', 'review_language::resolve_review_language', ?, 'rust', 1, 1, 0, 0, 0)",
    )
    .bind(LANG_FILE)
    .execute(db.pool())
    .await
    .unwrap();
    save(
        &db,
        &record(
            "turn-lang",
            "Make the review comment language a setting",
            "Added reviews.commentLanguage with a dropdown.",
            &[LANG_FILE],
        ),
        NOW,
    )
    .await;
    save(
        &db,
        &record("turn-other", "Tune the parser", "Done.", &["src/parser.rs"]),
        NOW,
    )
    .await;
    (dir, db)
}

fn query(text: &str) -> HistoryQuery {
    HistoryQuery {
        query: text.into(),
        since_ms: None,
        limit: 20,
    }
}

fn kinds_and_ids(entries: &[ax_memory::HistoryEntry]) -> Vec<(String, String)> {
    entries
        .iter()
        .map(|e| {
            (
                e.kind.clone(),
                if e.kind == "turn" {
                    e.id.clone()
                } else {
                    e.title.clone()
                },
            )
        })
        .collect()
}

#[tokio::test]
async fn h1_path_lists_the_turn_and_the_commit_newest_first() {
    let (dir, db) = project().await;
    for q in ["review_language.rs", LANG_FILE] {
        let entries = history(db.pool(), dir.path(), &query(q)).await.unwrap();
        assert_eq!(
            kinds_and_ids(&entries),
            vec![
                ("turn".to_string(), "turn-lang".to_string()),
                (
                    "commit".to_string(),
                    "Add review comment language setting".to_string()
                ),
            ],
            "query {q}"
        );
        assert_eq!(entries[0].at_ms, NOW);
        assert_eq!(entries[1].at_ms, COMMIT_MS);
        assert_eq!(
            entries[0].outcome.as_deref(),
            Some("Added reviews.commentLanguage with a dropdown.")
        );
        assert_eq!(entries[1].files, vec![LANG_FILE.to_string()]);
    }
}

#[tokio::test]
async fn h1_symbol_name_resolves_to_its_file() {
    let (dir, db) = project().await;
    let entries = history(db.pool(), dir.path(), &query("resolve_review_language"))
        .await
        .unwrap();
    assert_eq!(kinds_and_ids(&entries).len(), 2, "{entries:?}");
    assert_eq!(entries[0].id, "turn-lang");
}

#[tokio::test]
async fn h1_free_text_uses_recall_on_turns() {
    let (dir, db) = project().await;
    let entries = history(db.pool(), dir.path(), &query("review comment language"))
        .await
        .unwrap();
    assert!(entries.iter().any(|e| e.id == "turn-lang"), "{entries:?}");
    assert!(!entries.iter().any(|e| e.id == "turn-other"), "{entries:?}");
}

#[tokio::test]
async fn h1_free_text_needs_a_shared_word_beyond_recall() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    save(
        &db,
        &record(
            "turn-drop",
            "Rework dropdown",
            "Reworked it.",
            &["src/ui.rs"],
        ),
        NOW,
    )
    .await;
    // Character trigrams make the plural a vector match without a shared whole word.
    let recalled = ax_memory::recall(db.pool(), "dropdowns", 40).await.unwrap();
    assert!(
        recalled.iter().any(|m| m.memory.id == "turn-drop"),
        "{recalled:?}"
    );

    let entries = history(db.pool(), dir.path(), &query("dropdowns"))
        .await
        .unwrap();
    assert!(entries.is_empty(), "{entries:?}");
    let entries = history(db.pool(), dir.path(), &query("dropdown"))
        .await
        .unwrap();
    let found: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(found, vec!["turn-drop"]);
}

#[tokio::test]
async fn h3_since_drops_older_entries() {
    let (dir, db) = project().await;
    let mut q = query(LANG_FILE);
    q.since_ms = Some(NOW - DAY_MS / 2);
    let entries = history(db.pool(), dir.path(), &q).await.unwrap();
    assert_eq!(
        kinds_and_ids(&entries),
        vec![("turn".to_string(), "turn-lang".to_string())]
    );
}

#[tokio::test]
async fn h3_since_drops_older_turns() {
    let (dir, db) = project().await;
    save(
        &db,
        &record("turn-older", "Older edit", "Done.", &[LANG_FILE]),
        NOW - 3 * DAY_MS,
    )
    .await;
    let mut q = query(LANG_FILE);
    q.since_ms = Some(NOW - 2 * DAY_MS);
    let entries = history(db.pool(), dir.path(), &q).await.unwrap();
    assert!(!entries.iter().any(|e| e.id == "turn-older"), "{entries:?}");
    assert!(entries.iter().any(|e| e.id == "turn-lang"), "{entries:?}");
}

#[tokio::test]
async fn history_entry_is_only_for_turn_memories() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let note = remember(
        db.pool(),
        RememberInput {
            title: "A decision".into(),
            body: "Not a turn".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(history_entry(db.pool(), &note.id).await.unwrap(), None);
}

#[tokio::test]
async fn h4_listing_cuts_the_outcome_and_the_id_gives_it_in_full() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let outcome: String = (0..5_000)
        .map(|i| char::from(b'a' + (i % 26) as u8))
        .collect();
    save(
        &db,
        &record("turn-long", "Long turn", &outcome, &["src/a.rs"]),
        NOW,
    )
    .await;

    let entries = history(db.pool(), dir.path(), &query("src/a.rs"))
        .await
        .unwrap();
    assert_eq!(entries.len(), 1, "{entries:?}");
    let listing = format_history(&entries, 600);
    assert!(listing.contains(&outcome[..600]), "{listing}");
    assert!(!listing.contains(&outcome[..601]), "listing is cut at 600");
    assert!(listing.contains("turn-long"), "{listing}");
    assert!(listing.contains(&format_when(NOW)), "{listing}");

    let full = history_entry(db.pool(), "turn-long")
        .await
        .unwrap()
        .expect("found");
    assert_eq!(full.outcome.as_deref(), Some(outcome.as_str()));
    assert!(format_history(&[full], usize::MAX).contains(&outcome));
    assert_eq!(history_entry(db.pool(), "missing").await.unwrap(), None);
}

#[tokio::test]
async fn history_outside_git_still_lists_turns() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    save(
        &db,
        &record("turn-a", "Edit a", "Done.", &["src/a.rs"]),
        NOW,
    )
    .await;
    let entries = history(db.pool(), dir.path(), &query("src/a.rs"))
        .await
        .unwrap();
    assert_eq!(
        kinds_and_ids(&entries),
        vec![("turn".to_string(), "turn-a".to_string())]
    );
}
