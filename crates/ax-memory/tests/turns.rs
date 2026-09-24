//! Per-turn memories (docs/specs/per-turn-memory.md): dedup, retention, recall-only, local-only, redaction.

use ax_memory::{
    export_shared, prune_turns, recall, recall_for_prompt, redact_secrets, remember, save_turn,
    turn_outcome, RememberInput, TurnRecord, OUTCOME_MARKER, TURN_KIND, TURN_RETENTION_DAYS,
    TURN_SOURCE,
};

const DAY_MS: i64 = 86_400_000;
const NOW: i64 = 1_790_000_000_000;

async fn open_db(dir: &std::path::Path) -> ax_db::Database {
    ax_db::Database::open(&dir.join("ax.db")).await.unwrap()
}

fn turn(id: &str, title: &str) -> TurnRecord {
    TurnRecord {
        id: id.into(),
        title: title.into(),
        body: format!("{title}\nfiles: src/cache.rs"),
        files: vec!["src/cache.rs".into()],
    }
}

async fn count(db: &ax_db::Database, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(db.pool()).await.unwrap()
}

#[tokio::test]
async fn saved_turn_is_a_local_turn_memory() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    assert!(
        save_turn(db.pool(), &turn("turn-a", "Rework the policy cache"), NOW)
            .await
            .unwrap()
    );
    let row = ax_memory::get(db.pool(), "turn-a")
        .await
        .unwrap()
        .expect("saved");
    assert_eq!(row.kind, TURN_KIND);
    assert_eq!(row.source, TURN_SOURCE);
    assert_eq!(row.title, "Rework the policy cache");
    assert_eq!(row.files, vec!["src/cache.rs".to_string()]);
    assert_eq!(row.created_at, NOW);
}

#[tokio::test]
async fn saving_the_same_turn_twice_keeps_one_memory() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    assert!(
        save_turn(db.pool(), &turn("turn-a", "Rework the policy cache"), NOW)
            .await
            .unwrap()
    );
    assert!(!save_turn(
        db.pool(),
        &turn("turn-a", "Rework the policy cache"),
        NOW + 5
    )
    .await
    .unwrap());
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM memories WHERE id = 'turn-a'").await,
        1
    );
}

#[tokio::test]
async fn prune_removes_only_old_turn_memories() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    save_turn(db.pool(), &turn("turn-old", "Old turn"), NOW - 31 * DAY_MS)
        .await
        .unwrap();
    save_turn(db.pool(), &turn("turn-new", "New turn"), NOW - 29 * DAY_MS)
        .await
        .unwrap();
    let note = remember(
        db.pool(),
        RememberInput {
            title: "Old decision".into(),
            body: "Keep me".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE memories SET created_at = ?, updated_at = ? WHERE id = ?")
        .bind(NOW - 400 * DAY_MS)
        .bind(NOW - 400 * DAY_MS)
        .bind(&note.id)
        .execute(db.pool())
        .await
        .unwrap();

    let backups = dir.path().join("backups");
    assert_eq!(prune_turns(db.pool(), NOW, 30, &backups).await.unwrap(), 1);
    assert!(ax_memory::get(db.pool(), "turn-old")
        .await
        .unwrap()
        .is_none());
    assert!(ax_memory::get(db.pool(), "turn-new")
        .await
        .unwrap()
        .is_some());
    assert!(ax_memory::get(db.pool(), &note.id).await.unwrap().is_some());
}

fn backup_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries.map(|e| e.unwrap().path()).collect();
    files.sort();
    files
}

#[test]
fn retention_is_90_days() {
    assert_eq!(TURN_RETENTION_DAYS, 90);
}

#[tokio::test]
async fn p1_p2_turn_older_than_retention_is_backed_up_then_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    save_turn(
        db.pool(),
        &turn("turn-91", "Ninety-one days"),
        NOW - 91 * DAY_MS,
    )
    .await
    .unwrap();
    save_turn(
        db.pool(),
        &turn("turn-89", "Eighty-nine days"),
        NOW - 89 * DAY_MS,
    )
    .await
    .unwrap();
    let backups = dir.path().join(".ax").join("backups");

    assert_eq!(
        prune_turns(db.pool(), NOW, TURN_RETENTION_DAYS, &backups)
            .await
            .unwrap(),
        1
    );
    assert!(ax_memory::get(db.pool(), "turn-91")
        .await
        .unwrap()
        .is_none());
    assert!(ax_memory::get(db.pool(), "turn-89")
        .await
        .unwrap()
        .is_some());

    let files = backup_files(&backups);
    assert_eq!(files.len(), 1, "{files:?}");
    let name = files[0].file_name().unwrap().to_string_lossy().to_string();
    let today = ax_memory::format_when(NOW)[..10].to_string();
    assert_eq!(name, format!("turn-memories-{today}.jsonl"));
    let text = std::fs::read_to_string(&files[0]).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "{text}");
    let row: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(row["id"], "turn-91");
    assert_eq!(row["kind"], TURN_KIND);
    assert_eq!(row["title"], "Ninety-one days");
    assert_eq!(row["body"], "Ninety-one days\nfiles: src/cache.rs");
    assert_eq!(row["files"], serde_json::json!(["src/cache.rs"]));
    assert_eq!(row["created_at"], NOW - 91 * DAY_MS);
}

#[tokio::test]
async fn backups_append_to_the_same_day_file() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    let backups = dir.path().join("backups");
    for id in ["turn-a", "turn-b"] {
        save_turn(db.pool(), &turn(id, id), NOW - 100 * DAY_MS)
            .await
            .unwrap();
        prune_turns(db.pool(), NOW, TURN_RETENTION_DAYS, &backups)
            .await
            .unwrap();
    }
    let files = backup_files(&backups);
    assert_eq!(files.len(), 1, "{files:?}");
    let text = std::fs::read_to_string(&files[0]).unwrap();
    assert_eq!(text.lines().count(), 2, "{text}");
    assert!(text.contains("turn-a") && text.contains("turn-b"));
}

#[tokio::test]
async fn nothing_to_prune_writes_no_backup_file() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    save_turn(db.pool(), &turn("turn-new", "New"), NOW)
        .await
        .unwrap();
    let backups = dir.path().join("backups");
    assert_eq!(
        prune_turns(db.pool(), NOW, TURN_RETENTION_DAYS, &backups)
            .await
            .unwrap(),
        0
    );
    assert!(!backups.exists());
}

#[tokio::test]
async fn p3_failed_backup_deletes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    save_turn(db.pool(), &turn("turn-old", "Old"), NOW - 100 * DAY_MS)
        .await
        .unwrap();
    let not_a_dir = dir.path().join("backups");
    std::fs::write(&not_a_dir, "a file, not a directory").unwrap();

    assert!(prune_turns(db.pool(), NOW, TURN_RETENTION_DAYS, &not_a_dir)
        .await
        .is_err());
    assert!(ax_memory::get(db.pool(), "turn-old")
        .await
        .unwrap()
        .is_some());
}

#[test]
fn outcome_is_read_back_from_the_body() {
    assert_eq!(
        turn_outcome("Fix it\n\nFiles: a.rs\n\nOutcome: Fixed, 3 tests added."),
        Some("Fixed, 3 tests added.")
    );
    assert_eq!(turn_outcome("Fix it\n\nFiles: a.rs"), None);
    assert_eq!(turn_outcome(&format!("Fix it{OUTCOME_MARKER}")), None);
}

#[tokio::test]
async fn preflight_recall_skips_turn_memories_but_recall_finds_them() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    save_turn(
        db.pool(),
        &turn("turn-a", "Rework the policy cache invalidation"),
        NOW,
    )
    .await
    .unwrap();

    let for_prompt = recall_for_prompt(db.pool(), "policy cache invalidation", 3)
        .await
        .unwrap();
    assert!(
        for_prompt.is_empty(),
        "turn memories are recall-only: {for_prompt:?}"
    );

    let hits = recall(db.pool(), "policy cache invalidation", 5)
        .await
        .unwrap();
    assert!(
        hits.iter().any(|m| m.memory.id == "turn-a"),
        "ax_recall must find it"
    );
}

#[tokio::test]
async fn preflight_recall_still_returns_other_kinds_when_turns_outrank_them() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    for i in 0..6 {
        save_turn(
            db.pool(),
            &turn(&format!("turn-{i}"), "policy cache invalidation"),
            NOW,
        )
        .await
        .unwrap();
    }
    remember(
        db.pool(),
        RememberInput {
            title: "Policy cache is keyed by db generation".into(),
            body: "Invalidate the policy cache when the generation changes.".into(),
            kind: Some("decision".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let for_prompt = recall_for_prompt(db.pool(), "policy cache invalidation", 3)
        .await
        .unwrap();
    assert_eq!(for_prompt.len(), 1, "{for_prompt:?}");
    assert_eq!(for_prompt[0].memory.kind, "decision");
}

#[tokio::test]
async fn export_never_writes_turn_memories() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_db(dir.path()).await;
    save_turn(db.pool(), &turn("turn-a", "Rework the policy cache"), NOW)
        .await
        .unwrap();
    sqlx::query("UPDATE memories SET tags = '[\"shared\"]' WHERE id = 'turn-a'")
        .execute(db.pool())
        .await
        .unwrap();
    remember(
        db.pool(),
        RememberInput {
            title: "Shared decision".into(),
            body: "Team-wide".into(),
            tags: vec!["shared".into()],
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let out = dir.path().join("shared.jsonl");
    let result = export_shared(db.pool(), dir.path(), "shared", Some(&out))
        .await
        .unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    assert_eq!(result.written, 1);
    assert!(!text.contains("turn-a"), "{text}");
    assert!(text.contains("Shared decision"));
}

#[test]
fn redacts_api_keys_and_tokens() {
    assert_eq!(
        redact_secrets("use key sk-abcdefghijklmnopqrstuvwx now"),
        "use key [redacted] now"
    );
    assert_eq!(
        redact_secrets("token ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123 here"),
        "token [redacted] here"
    );
    assert_eq!(
        redact_secrets("aws AKIAIOSFODNN7EXAMPLE."),
        "aws [redacted]."
    );
}

#[test]
fn redacts_password_values() {
    assert_eq!(
        redact_secrets("password=hunter2 and more"),
        "password=[redacted] and more"
    );
    assert_eq!(
        redact_secrets("the password: hunter2"),
        "the password: [redacted]"
    );
}

#[test]
fn redacts_long_hex_and_base64_runs() {
    assert_eq!(
        redact_secrets("sha da39a3ee5e6b4b0d3255bfef95601890afd80709 end"),
        "sha [redacted] end"
    );
    assert_eq!(
        redact_secrets("b64 QWxhZGRpbjpvcGVuIHNlc2FtZVFXeGhaR1JwYmpwdmNHVnVJSE5sYzJGdFpR== end"),
        "b64 [redacted] end"
    );
}

#[test]
fn leaves_ordinary_text_alone() {
    let text = "fix crates/ax-sync/src/git_hooks.rs, commit b92f459, ask about sk-learn and the password field";
    assert_eq!(redact_secrets(text), text);
}
