use std::path::{Path, PathBuf};

use ax_db::integrity::{
    check_database, mark_clean_shutdown, note_sql_incident, quick_check_due, record_incident,
    repair_database, repair_rebuildable_indexes, run_due_quick_check, run_quick_check,
    take_unclean_shutdown, writers_blocked, CheckOptions, CheckReport, Finding, RepairOptions,
};
use ax_db::migrations::{get_current_version, CURRENT_SCHEMA_VERSION};
use ax_db::Database;

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "ax-integrity-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    root
}

fn cleanup(root: &Path) {
    let _ = std::fs::remove_dir_all(root);
}

fn db_path(root: &Path) -> PathBuf {
    root.join(".ax").join("ax.db")
}

fn codes(findings: &[Finding]) -> Vec<&str> {
    let mut codes: Vec<&str> = findings
        .iter()
        .map(|finding| finding.code.as_str())
        .collect();
    codes.sort_unstable();
    codes
}

fn check_opts(model: Option<&str>) -> CheckOptions {
    CheckOptions {
        quick: false,
        embedding_model: model.map(str::to_string),
    }
}

async fn seeded(path: &Path) -> Database {
    let db = Database::open(path).await.expect("open");
    sqlx::query(
        "INSERT INTO memories (id, title, body, created_at, updated_at) VALUES ('m1', 'Kept title', 'body', 1, 1)",
    )
    .execute(db.pool())
    .await
    .expect("insert memory");
    db
}

async fn settle(db: &Database) {
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(db.pool())
        .await
        .expect("checkpoint");
    db.close().await;
}

async fn cue_count(path: &Path, memory_id: &str) -> i64 {
    let db = Database::open(path).await.expect("reopen");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_cues WHERE memory_id = ?")
        .bind(memory_id)
        .fetch_one(db.pool())
        .await
        .expect("count cues");
    db.close().await;
    count
}

#[tokio::test]
async fn a_healthy_database_checks_clean_and_repair_writes_nothing() {
    let root = temp_root("healthy");
    let path = db_path(&root);
    settle(&seeded(&path).await).await;
    let report = check_database(&path, &check_opts(None)).await;
    assert!(report.ok, "{:?}", report.findings);
    let before = std::fs::read(&path).unwrap();
    let repair = repair_database(&path, &RepairOptions::default()).await;
    assert!(repair.ok, "{:?}", repair.remaining);
    assert!(repair.backup.is_none());
    assert!(repair.actions.is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    cleanup(&root);
}

#[tokio::test]
async fn a_missing_database_is_not_created() {
    let root = temp_root("missing");
    let path = db_path(&root);
    let report = check_database(&path, &check_opts(None)).await;
    assert_eq!(codes(&report.findings), ["missing"]);
    let repair = repair_database(&path, &RepairOptions::default()).await;
    assert!(!repair.ok);
    assert!(!path.exists());
    cleanup(&root);
}

#[tokio::test]
async fn garbage_bytes_are_copied_and_the_original_stays() {
    let root = temp_root("garbage");
    let path = db_path(&root);
    std::fs::write(&path, b"this is not sqlite").unwrap();
    let before = std::fs::read(&path).unwrap();
    let report = check_database(&path, &check_opts(None)).await;
    assert_eq!(codes(&report.findings), ["not_a_database"]);
    assert_eq!(std::fs::read(&path).unwrap(), before);

    let repair = repair_database(
        &path,
        &RepairOptions {
            salvage: true,
            ..RepairOptions::default()
        },
    )
    .await;
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!repair.proceed);
    assert!(repair.actions.iter().any(|action| action == "backup_raw"));
    assert!(repair
        .actions
        .iter()
        .any(|action| action == "salvage_refused"));
    let backup = PathBuf::from(repair.backup.expect("backup path"));
    assert_eq!(std::fs::read(&backup).unwrap(), before);
    let hash = std::fs::read_to_string(format!("{}.blake3", backup.display())).unwrap();
    assert_eq!(hash, blake3::hash(&before).to_hex().to_string());
    cleanup(&root);
}

#[tokio::test]
async fn an_orphan_cue_is_removed_and_the_memory_and_backup_stay() {
    let root = temp_root("orphan");
    let path = db_path(&root);
    let db = seeded(&path).await;
    sqlx::query(
        "INSERT INTO memory_cues (cue, memory_id, weight, source) VALUES ('lost', 'missing-id', 1, 'test')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    settle(&db).await;

    let report = check_database(&path, &check_opts(None)).await;
    assert_eq!(
        codes(&report.findings),
        ["orphan_cues"],
        "{:?}",
        report.findings
    );
    let repair = repair_database(&path, &RepairOptions::default()).await;
    assert!(repair.ok, "{:?}", repair.remaining);
    assert!(repair
        .actions
        .iter()
        .any(|action| action == "delete_orphan_cues"));
    assert_eq!(cue_count(&path, "missing-id").await, 0);

    let kept = Database::open(&path).await.unwrap();
    let title: String = sqlx::query_scalar("SELECT title FROM memories WHERE id = 'm1'")
        .fetch_one(kept.pool())
        .await
        .unwrap();
    kept.close().await;
    assert_eq!(title, "Kept title");

    let backup = PathBuf::from(repair.backup.expect("backup"));
    assert_eq!(cue_count(&backup, "missing-id").await, 1);
    let hash = std::fs::read_to_string(format!("{}.blake3", backup.display())).unwrap();
    let bytes = std::fs::read(&backup).unwrap();
    assert_eq!(hash, blake3::hash(&bytes).to_hex().to_string());

    let again = repair_database(&path, &RepairOptions::default()).await;
    assert!(again.ok);
    assert!(again.backup.is_none());
    cleanup(&root);
}

#[tokio::test]
async fn an_orphan_link_is_removed_and_the_kept_link_stays() {
    let root = temp_root("link");
    let path = db_path(&root);
    let db = seeded(&path).await;
    let mut conn = db.pool().acquire().await.unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memory_links (from_id, to_id, kind) VALUES ('m1', 'm1', 'kept')")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO memory_links (from_id, to_id, kind) VALUES ('m1', 'missing-id', 'source')",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    drop(conn);
    settle(&db).await;

    let report = check_database(&path, &check_opts(None)).await;
    assert!(
        codes(&report.findings).contains(&"orphan_links"),
        "{:?}",
        report.findings
    );
    let repair = repair_database(&path, &RepairOptions::default()).await;
    assert!(
        repair
            .actions
            .iter()
            .any(|action| action == "delete_orphan_links"),
        "{:?}",
        repair.actions
    );
    assert!(
        repair
            .remaining
            .iter()
            .all(|finding| finding.code != "orphan_links"),
        "{:?}",
        repair.remaining
    );
    let db = Database::open(&path).await.unwrap();
    let kept: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory_links WHERE from_id = 'm1' AND to_id = 'm1' AND kind = 'kept'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    let gone: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory_links WHERE to_id = 'missing-id'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    let title: String = sqlx::query_scalar("SELECT title FROM memories WHERE id = 'm1'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    db.close().await;
    assert_eq!((kept, gone, title.as_str()), (1, 0, "Kept title"));
    let backup = PathBuf::from(repair.backup.expect("backup"));
    let backed = Database::open(&backup).await.unwrap();
    let still: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory_links WHERE to_id = 'missing-id'")
            .fetch_one(backed.pool())
            .await
            .unwrap();
    backed.close().await;
    assert_eq!(still, 1);
    cleanup(&root);
}

#[tokio::test]
async fn a_vector_row_for_a_missing_memory_is_deleted() {
    let root = temp_root("vector-orphan");
    let path = db_path(&root);
    let db = seeded(&path).await;
    sqlx::query("CREATE VIRTUAL TABLE IF NOT EXISTS memory_vec USING vec0(embedding float[256])")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS memory_vec_map (memory_id TEXT PRIMARY KEY, rowid INTEGER NOT NULL)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    let embedding = format!("[{}]", ["0.000000"; 256].join(","));
    let mut conn = db.pool().acquire().await.unwrap();
    sqlx::query("INSERT INTO memory_vec(embedding) VALUES (?)")
        .bind(&embedding)
        .execute(&mut *conn)
        .await
        .unwrap();
    let orphan_row: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memory_vec_map (memory_id, rowid) VALUES ('missing-id', ?)")
        .bind(orphan_row)
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memory_vec(embedding) VALUES (?)")
        .bind(&embedding)
        .execute(&mut *conn)
        .await
        .unwrap();
    let kept_row: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    sqlx::query("INSERT INTO memory_vec_map (memory_id, rowid) VALUES ('m1', ?)")
        .bind(kept_row)
        .execute(&mut *conn)
        .await
        .unwrap();
    drop(conn);
    settle(&db).await;

    let report = check_database(&path, &check_opts(None)).await;
    assert!(
        codes(&report.findings).contains(&"vector_orphan_memory"),
        "{:?}",
        report.findings
    );
    assert!(
        !codes(&report.findings).contains(&"vector_dangling_row"),
        "{:?}",
        report.findings
    );
    let repair = repair_database(&path, &RepairOptions::default()).await;
    assert!(repair.proceed, "{:?}", repair.remaining);
    assert!(
        repair
            .actions
            .iter()
            .any(|action| action == "delete_vector_orphans"),
        "{:?}",
        repair.actions
    );
    assert!(
        repair
            .remaining
            .iter()
            .all(|finding| finding.code != "vector_orphan_memory"),
        "{:?}",
        repair.remaining
    );
    assert_eq!(
        cue_count_query(
            &path,
            "SELECT COUNT(*) FROM memory_vec_map WHERE memory_id = 'missing-id'",
        )
        .await,
        0
    );
    assert_eq!(
        cue_count_query(
            &path,
            "SELECT COUNT(*) FROM memory_vec_map WHERE memory_id = 'm1'",
        )
        .await,
        1
    );
    cleanup(&root);
}

#[tokio::test]
async fn a_broken_full_text_index_is_rebuilt() {
    let root = temp_root("fts");
    let path = db_path(&root);
    let db = seeded(&path).await;
    let rowid: i64 = sqlx::query_scalar("SELECT rowid FROM memories WHERE id = 'm1'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO memories_fts(memories_fts, rowid, id, title, body, tags) VALUES ('delete', ?, 'm1', 'Kept title', 'body', '[]')",
    )
    .bind(rowid)
    .execute(db.pool())
    .await
    .unwrap();
    settle(&db).await;

    let report = check_database(&path, &check_opts(None)).await;
    assert!(
        codes(&report.findings).contains(&"fts"),
        "{:?}",
        report.findings
    );
    let repair = repair_database(&path, &RepairOptions::default()).await;
    assert!(repair.ok, "{:?}", repair.remaining);
    assert!(repair
        .actions
        .iter()
        .any(|action| action.starts_with("rebuild_fts:")));
    cleanup(&root);
}

#[tokio::test]
async fn a_missing_cue_index_is_created() {
    let root = temp_root("index");
    let path = db_path(&root);
    let db = seeded(&path).await;
    sqlx::query("DROP INDEX idx_memory_cues_cue")
        .execute(db.pool())
        .await
        .unwrap();
    settle(&db).await;

    let report = check_database(&path, &check_opts(None)).await;
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "missing_index"
                && finding.subject == "idx_memory_cues_cue"),
        "{:?}",
        report.findings
    );
    let repair = repair_database(&path, &RepairOptions::default()).await;
    assert!(repair.ok, "{:?}", repair.remaining);
    let db = Database::open(&path).await.unwrap();
    let present: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM sqlite_master WHERE name = 'idx_memory_cues_cue' AND type = 'index'",
    )
    .fetch_optional(db.pool())
    .await
    .unwrap();
    db.close().await;
    assert!(present.is_some());
    cleanup(&root);
}

#[tokio::test]
async fn a_foreign_key_row_is_reported_and_left_in_place() {
    let root = temp_root("fk");
    let path = db_path(&root);
    let db = seeded(&path).await;
    let mut conn = db.pool().acquire().await.unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO node_communities (node_id, community_id, computed_at) VALUES ('missing-node', 1, 1)",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    drop(conn);
    settle(&db).await;

    let report = check_database(&path, &check_opts(None)).await;
    assert!(
        codes(&report.findings).contains(&"foreign_key"),
        "{:?}",
        report.findings
    );
    let repair = repair_database(&path, &RepairOptions::default()).await;
    assert!(!repair.proceed);
    assert!(repair.backup.is_none());
    assert!(repair.actions.is_empty());
    let db = Database::open(&path).await.unwrap();
    let still: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM node_communities WHERE node_id = 'missing-node'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    db.close().await;
    assert_eq!(still, 1);
    cleanup(&root);
}

#[tokio::test]
async fn a_behind_schema_is_migrated() {
    let root = temp_root("schema");
    let path = db_path(&root);
    let db = seeded(&path).await;
    sqlx::query("DELETE FROM schema_versions WHERE version >= 29")
        .execute(db.pool())
        .await
        .unwrap();
    settle(&db).await;

    let report = check_database(&path, &check_opts(None)).await;
    assert!(
        codes(&report.findings).contains(&"schema_behind"),
        "{:?}",
        report.findings
    );
    let repair = repair_database(&path, &RepairOptions::default()).await;
    assert!(repair.ok, "{:?}", repair.remaining);
    assert!(repair
        .actions
        .iter()
        .any(|action| action == "run_migrations"));
    let db = Database::open(&path).await.unwrap();
    let version = get_current_version(db.pool()).await.unwrap();
    db.close().await;
    assert_eq!(version, CURRENT_SCHEMA_VERSION);
    cleanup(&root);
}

#[tokio::test]
async fn a_dangling_vector_map_row_is_deleted() {
    let root = temp_root("vector");
    let path = db_path(&root);
    let db = seeded(&path).await;
    sqlx::query("CREATE VIRTUAL TABLE IF NOT EXISTS memory_vec USING vec0(embedding float[256])")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("CREATE TABLE IF NOT EXISTS memory_vec_map (memory_id TEXT PRIMARY KEY, rowid INTEGER NOT NULL)")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO memory_vec_map (memory_id, rowid) VALUES ('m1', 999)")
        .execute(db.pool())
        .await
        .unwrap();
    settle(&db).await;

    let report = check_database(&path, &check_opts(None)).await;
    assert!(
        codes(&report.findings).contains(&"vector_dangling_row"),
        "{:?}",
        report.findings
    );
    let repair = repair_database(&path, &RepairOptions::default()).await;
    assert!(repair.proceed, "{:?}", repair.remaining);
    assert!(
        repair
            .remaining
            .iter()
            .all(|finding| finding.code != "vector_dangling_row"),
        "{:?}",
        repair.remaining
    );
    assert_eq!(
        cue_count_query(&path, "SELECT COUNT(*) FROM memory_vec_map").await,
        0
    );
    cleanup(&root);
}

async fn cue_count_query(path: &Path, sql: &'static str) -> i64 {
    let db = Database::open(path).await.unwrap();
    let count: i64 = sqlx::query_scalar(sql).fetch_one(db.pool()).await.unwrap();
    db.close().await;
    count
}

#[tokio::test]
async fn a_foreign_embedding_model_is_reported() {
    let root = temp_root("model");
    let path = db_path(&root);
    let db = seeded(&path).await;
    sqlx::query("UPDATE memories SET embedding_model = 'other-model' WHERE id = 'm1'")
        .execute(db.pool())
        .await
        .unwrap();
    settle(&db).await;
    let report = check_database(&path, &check_opts(Some("hash-v1"))).await;
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "foreign_embedding_model"
                && finding.subject == "other-model"),
        "{:?}",
        report.findings
    );
    cleanup(&root);
}

#[tokio::test]
async fn a_write_ahead_log_with_frames_is_checkpointed() {
    let root = temp_root("wal");
    let path = db_path(&root);
    let db = Database::open(&path).await.unwrap();
    let mut conn = db.pool().acquire().await.unwrap();
    sqlx::query("PRAGMA wal_autocheckpoint = 0")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO memories (id, title, body, created_at, updated_at) VALUES ('m1', 'Kept title', 'body', 1, 1)",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    // A clean close checkpoints the log. Copy it while the writer is still open,
    // which is what a killed process leaves behind.
    let snap = root.join(".ax").join("killed.db");
    std::fs::copy(&path, &snap).unwrap();
    for suffix in ["-wal", "-shm"] {
        let from = PathBuf::from(format!("{}{suffix}", path.display()));
        if from.is_file() {
            std::fs::copy(&from, PathBuf::from(format!("{}{suffix}", snap.display()))).unwrap();
        }
    }
    let wal = PathBuf::from(format!("{}-wal", snap.display()));
    assert!(std::fs::metadata(&wal).expect("wal").len() > 32);
    drop(conn);
    db.close().await;

    let report = check_database(&snap, &check_opts(None)).await;
    assert!(
        codes(&report.findings).contains(&"wal_uncheckpointed"),
        "{:?}",
        report.findings
    );
    let repair = repair_database(&snap, &RepairOptions::default()).await;
    assert!(repair.ok, "{:?}", repair.remaining);
    assert!(repair
        .actions
        .iter()
        .any(|action| action == "checkpoint_wal"));
    let after = std::fs::metadata(&wal).map(|meta| meta.len()).unwrap_or(0);
    assert!(after <= 32, "wal still has frames: {after}");
    cleanup(&root);
}

#[tokio::test]
async fn dry_run_lists_the_cue_repair_and_writes_nothing() {
    let root = temp_root("dry");
    let path = db_path(&root);
    let db = seeded(&path).await;
    sqlx::query(
        "INSERT INTO memory_cues (cue, memory_id, weight, source) VALUES ('lost', 'missing-id', 1, 'test')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    settle(&db).await;
    let repair = repair_database(
        &path,
        &RepairOptions {
            dry_run: true,
            ..RepairOptions::default()
        },
    )
    .await;
    assert!(repair.dry_run);
    assert!(repair.backup.is_none());
    assert!(!repair.proceed);
    assert!(repair
        .actions
        .iter()
        .any(|action| action == "delete_orphan_cues"));
    assert_eq!(cue_count(&path, "missing-id").await, 1);
    assert!(!root.join(".ax").join("backups").exists());
    cleanup(&root);
}

#[tokio::test]
async fn a_locked_database_is_left_unchanged() {
    let root = temp_root("locked");
    let path = db_path(&root);
    let db = seeded(&path).await;
    sqlx::query(
        "INSERT INTO memory_cues (cue, memory_id, weight, source) VALUES ('lost', 'missing-id', 1, 'test')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    let mut guard = db.pool().acquire().await.unwrap();
    sqlx::query("BEGIN EXCLUSIVE")
        .execute(&mut *guard)
        .await
        .unwrap();

    let repair = repair_database(&path, &RepairOptions::default()).await;
    let lock = repair
        .remaining
        .iter()
        .find(|item| item.code == "writers_active")
        .expect("writers_active");
    let me = std::process::id();
    let holders = ax_utils::process::file_holders(&path);
    if holders.iter().any(|holder| holder.pid == me) {
        assert!(
            lock.detail.contains("holders:") && lock.detail.contains(&me.to_string()),
            "{}",
            lock.detail
        );
    } else {
        assert!(!lock.detail.contains("holders:"), "{}", lock.detail);
    }
    assert!(!repair.proceed);
    assert!(repair.backup.is_none());

    sqlx::query("ROLLBACK").execute(&mut *guard).await.unwrap();
    drop(guard);
    db.close().await;
    assert_eq!(cue_count(&path, "missing-id").await, 1);
    cleanup(&root);
}

#[tokio::test]
async fn a_blocked_backup_directory_leaves_the_cue() {
    let root = temp_root("nobackup");
    let path = db_path(&root);
    let db = seeded(&path).await;
    sqlx::query(
        "INSERT INTO memory_cues (cue, memory_id, weight, source) VALUES ('lost', 'missing-id', 1, 'test')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    settle(&db).await;
    let blocker = root.join("backup-is-a-file");
    std::fs::write(&blocker, b"not a directory").unwrap();
    let repair = repair_database(
        &path,
        &RepairOptions {
            backup_dir: Some(blocker),
            ..RepairOptions::default()
        },
    )
    .await;
    assert!(
        codes(&repair.remaining).contains(&"io"),
        "{:?}",
        repair.remaining
    );
    assert!(!repair.proceed);
    assert_eq!(cue_count(&path, "missing-id").await, 1);
    cleanup(&root);
}

#[tokio::test]
async fn a_plain_sqlite_file_passes() {
    let root = temp_root("plain");
    let path = root.join("notes.db");
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    let report = check_database(&path, &check_opts(None)).await;
    assert!(report.ok, "{:?}", report.findings);
    cleanup(&root);
}

#[tokio::test]
async fn an_incident_file_names_the_repair_command() {
    let root = temp_root("incident");
    let path = db_path(&root);
    std::fs::write(&path, b"x").unwrap();
    let report = CheckReport {
        path: path.display().to_string(),
        ok: false,
        findings: vec![Finding {
            code: "orphan_cues".into(),
            subject: "1".into(),
            detail: "1 cues point at a missing memory".into(),
        }],
    };
    let incident = record_incident(&path, &report).unwrap();
    let text = std::fs::read_to_string(incident).unwrap();
    assert!(text.contains("orphan_cues"));
    assert!(text.contains("ax db repair"));
    cleanup(&root);
}

#[tokio::test]
async fn a_corrupt_sql_error_records_an_incident_and_blocks_writers() {
    let root = temp_root("sql-incident");
    let path = db_path(&root);
    std::fs::write(&path, b"x").unwrap();
    let incident = note_sql_incident(&path, "database disk image is malformed")
        .unwrap()
        .expect("corrupt sql must record an incident");
    let text = std::fs::read_to_string(&incident).unwrap();
    assert!(text.contains("ax db repair"), "{text}");
    assert!(writers_blocked(&path));
    assert!(note_sql_incident(&path, "no such table: memories")
        .unwrap()
        .is_none());
    cleanup(&root);
}

#[test]
fn a_quick_check_is_due_once_a_day() {
    assert!(quick_check_due(0, 10));
    assert!(!quick_check_due(1_000, 1_000 + 86_399));
    assert!(quick_check_due(1_000, 1_000 + 86_400));
}

#[tokio::test]
async fn a_due_quick_check_records_a_broken_database() {
    let root = temp_root("daily-check");
    let path = db_path(&root);
    std::fs::write(&path, b"not a database").unwrap();
    run_due_quick_check(&path).await.unwrap();
    assert!(writers_blocked(&path));
    cleanup(&root);
}

#[tokio::test]
async fn opening_a_broken_database_records_an_incident() {
    let root = temp_root("open-broken");
    let path = db_path(&root);
    std::fs::write(&path, b"not a database").unwrap();
    let opened = Database::open(&path).await;
    assert!(opened.is_err());
    assert!(writers_blocked(&path));
    cleanup(&root);
}

#[tokio::test]
async fn a_passing_quick_check_clears_the_incident() {
    let root = temp_root("clear-incident");
    let path = db_path(&root);
    settle(&seeded(&path).await).await;
    note_sql_incident(&path, "database disk image is malformed")
        .unwrap()
        .expect("planted incident");
    run_due_quick_check(&path).await.unwrap();
    assert!(!writers_blocked(&path));
    cleanup(&root);
}

/// A killed process leaves WAL frames behind. The forced quick check after an
/// unclean shutdown must not treat that as damage and block writers.
#[tokio::test]
async fn a_wal_left_by_a_killed_process_does_not_block_writers() {
    let root = temp_root("killed-wal");
    let path = db_path(&root);
    let db = Database::open(&path).await.unwrap();
    let mut conn = db.pool().acquire().await.unwrap();
    sqlx::query("PRAGMA wal_autocheckpoint = 0")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO memories (id, title, body, created_at, updated_at) VALUES ('m1', 'Kept title', 'body', 1, 1)",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    let snap = root.join(".ax").join("killed.db");
    std::fs::copy(&path, &snap).unwrap();
    for suffix in ["-wal", "-shm"] {
        let from = PathBuf::from(format!("{}{suffix}", path.display()));
        if from.is_file() {
            std::fs::copy(&from, PathBuf::from(format!("{}{suffix}", snap.display()))).unwrap();
        }
    }
    drop(conn);
    db.close().await;
    let wal = PathBuf::from(format!("{}-wal", snap.display()));
    assert!(std::fs::metadata(&wal).expect("wal").len() > 32);

    run_quick_check(&snap, true).await.unwrap();
    assert!(!writers_blocked(&snap));
    cleanup(&root);
}

#[tokio::test]
async fn an_unclean_shutdown_checks_even_when_the_daily_stamp_is_fresh() {
    let root = temp_root("unclean");
    let path = db_path(&root);
    std::fs::write(&path, b"not a database").unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    std::fs::write(
        root.join(".ax").join("quick-check.json"),
        format!("{{\"at\":{now}}}\n"),
    )
    .unwrap();
    run_due_quick_check(&path).await.unwrap();
    assert!(!writers_blocked(&path));
    run_quick_check(&path, true).await.unwrap();
    assert!(writers_blocked(&path));
    cleanup(&root);
}

#[test]
fn a_marker_left_behind_means_the_previous_daemon_died() {
    let root = temp_root("marker");
    assert!(!take_unclean_shutdown(&root));
    assert!(take_unclean_shutdown(&root));
    mark_clean_shutdown(&root);
    assert!(!take_unclean_shutdown(&root));
    cleanup(&root);
}

#[tokio::test]
async fn a_missing_index_is_rebuilt_without_a_backup() {
    let root = temp_root("auto-index");
    let path = db_path(&root);
    let db = seeded(&path).await;
    sqlx::query("DROP INDEX idx_memory_cues_cue")
        .execute(db.pool())
        .await
        .unwrap();
    settle(&db).await;
    let actions = repair_rebuildable_indexes(&path).await.unwrap();
    assert!(
        actions
            .iter()
            .any(|action| action == "create_index:idx_memory_cues_cue"),
        "{actions:?}"
    );
    assert!(!root.join(".ax").join("backups").exists());
    let db = Database::open(&path).await.unwrap();
    let present: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM sqlite_master WHERE name = 'idx_memory_cues_cue' AND type = 'index'",
    )
    .fetch_optional(db.pool())
    .await
    .unwrap();
    let title: String = sqlx::query_scalar("SELECT title FROM memories WHERE id = 'm1'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    db.close().await;
    assert!(present.is_some());
    assert_eq!(title, "Kept title");
    cleanup(&root);
}

#[tokio::test]
async fn an_orphan_cue_is_rebuilt_and_a_dangling_edge_is_left() {
    let root = temp_root("auto-cue");
    let path = db_path(&root);
    let db = seeded(&path).await;
    let mut conn = db.pool().acquire().await.unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO memory_cues (cue, memory_id, weight, source) VALUES ('lost', 'missing-id', 1, 'test')",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
         VALUES ('n1', 'function', 'kept', 'kept', 'src/kept.rs', 'rust', 1, 2, 0, 1, 'fn kept()', 1)",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO edges (source, target, kind) VALUES ('n1', 'missing-node', 'dangling')",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    drop(conn);
    settle(&db).await;
    let actions = repair_rebuildable_indexes(&path).await.unwrap();
    assert!(
        actions.iter().any(|action| action == "delete_orphan_cues"),
        "{actions:?}"
    );
    assert!(
        actions.iter().all(|action| action != "delete_orphan_edges"),
        "{actions:?}"
    );
    assert_eq!(cue_count(&path, "missing-id").await, 0);
    let db = Database::open(&path).await.unwrap();
    let edges: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges WHERE target = 'missing-node'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    db.close().await;
    assert_eq!(edges, 1);
    assert!(writers_blocked(&path));
    cleanup(&root);
}

#[tokio::test]
async fn dangling_graph_rows_are_removed_and_the_kept_rows_stay() {
    let root = temp_root("graph-refs");
    let path = db_path(&root);
    let db = seeded(&path).await;
    let mut conn = db.pool().acquire().await.unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
         VALUES ('n1', 'function', 'kept', 'kept', 'src/kept.rs', 'rust', 1, 2, 0, 1, 'fn kept()', 1)",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO files (path, content_hash, language, size, modified_at, indexed_at)
         VALUES ('src/kept.rs', 'abc', 'rust', 1, 1, 1)",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO edges (source, target, kind) VALUES ('n1', 'n1', 'kept'), ('n1', 'missing-node', 'dangling')",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO file_contents (path, content_hash, content, byte_len, updated_at)
         VALUES ('src/kept.rs', 'abc', 'fn kept()', 9, 1), ('src/gone.rs', 'def', 'gone', 4, 1)",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO unresolved_refs (from_node_id, reference_name, reference_kind, line, col)
         VALUES ('n1', 'kept', 'call', 1, 1), ('missing-node', 'gone', 'call', 1, 1)",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    drop(conn);
    settle(&db).await;

    let report = check_database(&path, &check_opts(None)).await;
    let found = codes(&report.findings);
    for code in ["orphan_edges", "orphan_file_contents", "orphan_unresolved"] {
        assert!(found.contains(&code), "{code} missing in {found:?}");
    }

    let repair = repair_database(&path, &RepairOptions::default()).await;
    for action in [
        "delete_orphan_edges",
        "delete_orphan_file_contents",
        "delete_orphan_unresolved",
    ] {
        assert!(
            repair.actions.iter().any(|item| item == action),
            "{action} missing in {:?}",
            repair.actions
        );
    }
    let remaining = codes(&repair.remaining);
    for code in ["orphan_edges", "orphan_file_contents", "orphan_unresolved"] {
        assert!(!remaining.contains(&code), "{code} still in {remaining:?}");
    }

    let db = Database::open(&path).await.unwrap();
    let kept_edge: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM edges WHERE source = 'n1' AND target = 'n1' AND kind = 'kept'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    let gone_edge: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM edges WHERE target = 'missing-node'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    let kept_body: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM file_contents WHERE path = 'src/kept.rs'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    let gone_body: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM file_contents WHERE path = 'src/gone.rs'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    let kept_ref: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM unresolved_refs WHERE from_node_id = 'n1'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    let gone_ref: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM unresolved_refs WHERE from_node_id = 'missing-node'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    db.close().await;
    assert_eq!(
        (kept_edge, gone_edge, kept_body, gone_body, kept_ref, gone_ref),
        (1, 0, 1, 0, 1, 0)
    );

    let backup = PathBuf::from(repair.backup.expect("backup"));
    let backed = Database::open(&backup).await.unwrap();
    let still: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges WHERE target = 'missing-node'")
        .fetch_one(backed.pool())
        .await
        .unwrap();
    backed.close().await;
    assert_eq!(still, 1);
    cleanup(&root);
}

#[tokio::test]
async fn a_short_embedding_is_reported_and_the_memory_stays() {
    let root = temp_root("embed-dim");
    let path = db_path(&root);
    let db = seeded(&path).await;
    sqlx::query(
        "INSERT INTO memories (id, title, body, created_at, updated_at, embedding)
         VALUES ('m2', 'Full', 'body', 1, 1, zeroblob(1024))",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("UPDATE memories SET embedding = x'00000000' WHERE id = 'm1'")
        .execute(db.pool())
        .await
        .unwrap();
    settle(&db).await;

    let report = check_database(&path, &check_opts(Some("hash-v1"))).await;
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "embedding_dimension" && finding.subject == "1"),
        "{:?}",
        report.findings
    );

    let repair = repair_database(
        &path,
        &RepairOptions {
            embedding_model: Some("hash-v1".into()),
            ..RepairOptions::default()
        },
    )
    .await;
    assert!(
        repair.follow_up.iter().any(|item| item == "reembed"),
        "{:?}",
        repair.follow_up
    );
    assert!(
        repair
            .actions
            .iter()
            .all(|action| !action.contains("delete") || action.starts_with("checkpoint")),
        "a short embedding is not deleted: {:?}",
        repair.actions
    );
    let db = Database::open(&path).await.unwrap();
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM memories ORDER BY id")
        .fetch_all(db.pool())
        .await
        .unwrap();
    db.close().await;
    assert_eq!(ids, vec!["m1".to_string(), "m2".to_string()]);
    cleanup(&root);
}

#[tokio::test]
async fn a_dangling_edge_names_its_file_for_reindex() {
    let root = temp_root("graph-reindex");
    let path = db_path(&root);
    let db = seeded(&path).await;
    let mut conn = db.pool().acquire().await.unwrap();
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
         VALUES
         ('n1', 'function', 'kept', 'kept', 'src/kept.rs', 'rust', 1, 2, 0, 1, 'fn kept()', 1),
         ('n2', 'function', 'other', 'other', 'src/other.rs', 'rust', 1, 2, 0, 1, 'fn other()', 1),
         ('n3', 'function', 'out', 'out', '../outside.rs', 'rust', 1, 2, 0, 1, 'fn out()', 1)",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO edges (source, target, kind) VALUES
         ('n1', 'missing-node', 'dangling'),
         ('n2', 'n2', 'kept'),
         ('missing-a', 'missing-b', 'gone'),
         ('n3', 'missing-out', 'outside')",
    )
    .execute(&mut *conn)
    .await
    .unwrap();
    drop(conn);
    settle(&db).await;

    let repair = repair_database(
        &path,
        &RepairOptions {
            dry_run: true,
            ..RepairOptions::default()
        },
    )
    .await;
    assert!(
        repair.follow_up == vec!["reindex:src/kept.rs".to_string()],
        "{:?}",
        repair.follow_up
    );
    assert!(
        repair
            .actions
            .iter()
            .any(|action| action == "delete_orphan_edges"),
        "{:?}",
        repair.actions
    );

    let db = Database::open(&path).await.unwrap();
    let dangling: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM edges WHERE target = 'missing-node'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    db.close().await;
    assert_eq!(dangling, 1);
    cleanup(&root);
}

/// Damages one b-tree page in the middle of the file, leaving the header and
/// the schema page intact, so `PRAGMA quick_check` reports damage.
async fn damaged(path: &Path) -> Vec<u8> {
    let db = seeded(path).await;
    for i in 0..400 {
        sqlx::query(
            "INSERT INTO memories (id, title, body, created_at, updated_at) VALUES (?, ?, ?, 1, 1)",
        )
        .bind(format!("m-{i}"))
        .bind(format!("title {i}"))
        .bind("x".repeat(300))
        .execute(db.pool())
        .await
        .expect("insert");
    }
    settle(&db).await;
    let mut bytes = std::fs::read(path).unwrap();
    let page = u16::from_be_bytes([bytes[16], bytes[17]]) as usize;
    let page = if page == 1 { 65_536 } else { page };
    let pages = bytes.len() / page;
    let target = pages / 2;
    for b in &mut bytes[target * page..target * page + 64] {
        *b = 0xA5;
    }
    std::fs::write(path, &bytes).unwrap();
    bytes
}

#[tokio::test]
async fn the_unattended_quick_check_leaves_a_damaged_database_alone() {
    let root = temp_root("damaged-quick");
    let path = db_path(&root);
    let before = damaged(&path).await;
    let report = check_database(
        &path,
        &CheckOptions {
            quick: true,
            embedding_model: None,
        },
    )
    .await;
    assert!(
        codes(&report.findings).contains(&"integrity"),
        "the damage must be found: {:?}",
        report.findings
    );
    run_quick_check(&path, true).await.unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "the unattended check must not rewrite a damaged database"
    );
    for side in ["-wal", "-journal"] {
        let p = PathBuf::from(format!("{}{side}", path.display()));
        assert!(
            std::fs::metadata(&p).map(|m| m.len() == 0).unwrap_or(true),
            "{side} holds writes"
        );
    }
    assert!(writers_blocked(&path), "an incident names ax db repair");
    let repaired = repair_rebuildable_indexes(&path).await.unwrap();
    assert!(repaired.is_empty(), "no unattended rewrite: {repaired:?}");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    cleanup(&root);
}
