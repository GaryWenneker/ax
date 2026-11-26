//! Read-only checks and in-place repairs for an ax SQLite file.
//!
//! The database path is never renamed or deleted. A repair that would change a
//! readable file copies it first. A file that is not a database is copied aside
//! and left in place.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::ConnectOptions;

use crate::migrations::{get_current_version, run_migrations, CURRENT_SCHEMA_VERSION};

const LOCK_WAIT: Duration = Duration::from_millis(400);

static BACKUP_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Finding {
    pub code: String,
    pub subject: String,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub struct CheckOptions {
    pub quick: bool,
    pub embedding_model: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RepairOptions {
    pub dry_run: bool,
    pub salvage: bool,
    pub backup_dir: Option<PathBuf>,
    pub embedding_model: Option<String>,
    pub quick: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckReport {
    pub path: String,
    pub ok: bool,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepairReport {
    pub path: String,
    pub ok: bool,
    /// The caller may re-embed or rebuild vectors. False when the file was
    /// left untouched because it was locked, missing, unreadable, or the backup
    /// could not be written.
    pub proceed: bool,
    pub dry_run: bool,
    pub backup: Option<String>,
    pub actions: Vec<String>,
    pub follow_up: Vec<String>,
    pub remaining: Vec<Finding>,
}

struct OpenError {
    code: &'static str,
    detail: String,
}

/// Indexes this crate creates for the memory vault. Repair puts back any that
/// are missing. It does not invent indexes the schema never had.
const MEMORY_INDEXES: &[(&str, &str, &str)] = &[
    (
        "idx_memories_kind",
        "memories",
        "CREATE INDEX IF NOT EXISTS idx_memories_kind ON memories(kind)",
    ),
    (
        "idx_memories_updated",
        "memories",
        "CREATE INDEX IF NOT EXISTS idx_memories_updated ON memories(updated_at)",
    ),
    (
        "idx_memories_tier",
        "memories",
        "CREATE INDEX IF NOT EXISTS idx_memories_tier ON memories(tier)",
    ),
    (
        "idx_memory_cues_cue",
        "memory_cues",
        "CREATE INDEX IF NOT EXISTS idx_memory_cues_cue ON memory_cues(cue)",
    ),
];

pub async fn check_database(path: &Path, options: &CheckOptions) -> CheckReport {
    if !path.exists() {
        return report(
            path,
            vec![finding("missing", "", "database file is missing")],
        );
    }
    // Measured before opening. Opening can checkpoint, and a check must not hide a log it found.
    let wal_pending = wal_has_frames(path);
    // Read-only so a check cannot checkpoint a log it is reporting.
    // The full-text command is a write, so it runs only when the log is quiet.
    let pool = match open_pool(path, true).await {
        Ok(pool) => pool,
        Err(err) => return report(path, vec![finding(err.code, "", err.detail)]),
    };
    let mut findings = collect(&pool, options, !wal_pending).await;
    pool.close().await;
    if findings.iter().any(|item| item.code == "not_a_database") {
        findings.truncate(1);
        return report(path, findings);
    }
    if !wal_pending {
        if let Ok(writer) = open_pool(path, false).await {
            let _ = sqlx::query("PRAGMA wal_autocheckpoint = 0")
                .execute(&writer)
                .await;
            if let Err(err) = push_fts(&writer, &mut findings).await {
                findings.push(step_finding(err));
            }
            writer.close().await;
        }
    }
    if wal_pending {
        findings.push(finding(
            "wal_uncheckpointed",
            "",
            "the write-ahead log still has frames",
        ));
    }
    annotate_writers(path, &mut findings);
    report(path, findings)
}

pub async fn repair_database(path: &Path, options: &RepairOptions) -> RepairReport {
    let before = check_database(
        path,
        &CheckOptions {
            quick: options.quick,
            embedding_model: options.embedding_model.clone(),
        },
    )
    .await;
    let actions = planned_actions(&before.findings);
    let follow_up = planned_follow_up(&before.findings);

    if before.findings.iter().any(|f| f.code == "missing") {
        return finished(
            path,
            false,
            false,
            options.dry_run,
            None,
            vec![],
            vec![],
            before.findings,
        );
    }
    if before.findings.iter().any(|f| f.code == "writers_active") {
        return finished(
            path,
            false,
            false,
            options.dry_run,
            None,
            vec![],
            vec![],
            before.findings,
        );
    }
    if actions.is_empty() && follow_up.is_empty() {
        return finished(
            path,
            before.ok,
            false,
            options.dry_run,
            None,
            vec![],
            vec![],
            before.findings,
        );
    }
    if options.dry_run {
        return finished(
            path,
            false,
            false,
            true,
            None,
            actions,
            follow_up,
            before.findings,
        );
    }

    if before.findings.iter().any(|f| f.code == "not_a_database") {
        return repair_unreadable(path, options, before.findings).await;
    }
    if let Some(detail) = write_lock_blocked(path).await {
        let mut remaining = before.findings;
        let holders = ax_utils::process::file_holders(path);
        remaining.push(finding(
            "writers_active",
            "",
            ax_utils::process::with_holders(&detail, &holders),
        ));
        return finished(path, false, false, false, None, vec![], vec![], remaining);
    }

    let backup_dir = options
        .backup_dir
        .clone()
        .unwrap_or_else(|| default_backup_dir(path));
    if let Err(err) = std::fs::create_dir_all(&backup_dir) {
        let mut remaining = before.findings;
        remaining.push(finding(
            "io",
            "",
            format!("backup directory was not created: {err}"),
        ));
        return finished(path, false, false, false, None, vec![], vec![], remaining);
    }

    // A consistent snapshot, before any checkpoint or delete. A file that
    // cannot be vacuumed is copied as bytes, including its log.
    let backup = match backup_database(path, &backup_dir).await {
        Ok(backup) => backup,
        Err(detail) => {
            let mut remaining = before.findings;
            remaining.push(finding("io", "", detail));
            return finished(path, false, false, false, None, vec![], vec![], remaining);
        }
    };

    let pool = match open_pool(path, false).await {
        Ok(pool) => pool,
        Err(err) => {
            return finished(
                path,
                false,
                false,
                false,
                Some(backup),
                vec!["backup_raw".into()],
                vec![],
                vec![finding(err.code, "", err.detail)],
            );
        }
    };

    let mut done = Vec::new();
    if let Err(detail) = apply(&pool, &before.findings, &mut done).await {
        pool.close().await;
        let mut remaining = before.findings;
        remaining.push(finding("io", "", detail));
        return finished(
            path,
            false,
            false,
            false,
            Some(backup),
            done,
            follow_up,
            remaining,
        );
    }
    pool.close().await;

    let after = check_database(
        path,
        &CheckOptions {
            quick: options.quick,
            embedding_model: options.embedding_model.clone(),
        },
    )
    .await;
    if after.ok {
        clear_incident(path);
    }
    finished(
        path,
        after.ok && follow_up.is_empty(),
        true,
        false,
        Some(backup),
        done,
        follow_up,
        after.findings,
    )
}

pub fn record_incident(db_path: &Path, report: &CheckReport) -> Result<PathBuf, String> {
    let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
    let path = parent.join("integrity.json");
    let body = serde_json::json!({
        "database": db_path,
        "ok": report.ok,
        "findings": report.findings,
        "repair": "ax db repair",
    });
    let text = serde_json::to_string_pretty(&body).map_err(|err| err.to_string())?;
    std::fs::write(&path, text).map_err(|err| err.to_string())?;
    Ok(path)
}

const QUICK_CHECK_INTERVAL_SECS: i64 = 86_400;

pub fn sql_error_is_incident(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("not a database") || lower.contains("malformed") || lower.contains("corrupt")
}

pub fn note_sql_incident(db_path: &Path, message: &str) -> Result<Option<PathBuf>, String> {
    if !sql_error_is_incident(message) {
        return Ok(None);
    }
    let report = CheckReport {
        path: db_path.display().to_string(),
        ok: false,
        findings: vec![Finding {
            code: "sql_error".into(),
            subject: "sqlite".into(),
            detail: message.to_string(),
        }],
    };
    record_incident(db_path, &report).map(Some)
}

pub fn incident_path(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("integrity.json")
}

pub fn incident_value(db_path: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(incident_path(db_path)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn writers_blocked(db_path: &Path) -> bool {
    incident_value(db_path).and_then(|value| value.get("ok").and_then(|ok| ok.as_bool()))
        == Some(false)
}

pub fn incident_notice(db_path: &Path) -> Option<String> {
    if !writers_blocked(db_path) {
        return None;
    }
    Some("database incident recorded. Run `ax db repair`.\n".into())
}

pub fn clear_incident(db_path: &Path) {
    let _ = std::fs::remove_file(incident_path(db_path));
}

pub fn quick_check_due(last_unix_secs: i64, now_unix_secs: i64) -> bool {
    last_unix_secs <= 0 || now_unix_secs.saturating_sub(last_unix_secs) >= QUICK_CHECK_INTERVAL_SECS
}

fn quick_check_stamp_path(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("quick-check.json")
}

fn read_quick_check_stamp(db_path: &Path) -> i64 {
    std::fs::read_to_string(quick_check_stamp_path(db_path))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|value| value.get("at").and_then(|at| at.as_i64()))
        .unwrap_or(0)
}

fn write_quick_check_stamp(db_path: &Path, now_unix_secs: i64) {
    let _ = std::fs::write(
        quick_check_stamp_path(db_path),
        format!("{{\"at\":{now_unix_secs}}}\n"),
    );
}

pub fn daemon_unclean_path(project_root: &Path) -> PathBuf {
    project_root.join(".ax").join("daemon.unclean")
}

/// Returns whether the previous daemon exited without a clean shutdown, then
/// leaves the marker so a crash of this process is visible next time.
pub fn take_unclean_shutdown(project_root: &Path) -> bool {
    let path = daemon_unclean_path(project_root);
    let dirty = path.is_file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, b"1\n");
    dirty
}

pub fn mark_clean_shutdown(project_root: &Path) {
    let _ = std::fs::remove_file(daemon_unclean_path(project_root));
}

pub async fn run_due_quick_check(db_path: &Path) -> Result<(), String> {
    run_quick_check(db_path, false).await
}

pub async fn run_quick_check(db_path: &Path, force: bool) -> Result<(), String> {
    if !db_path.is_file() {
        return Ok(());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    if !force && !quick_check_due(read_quick_check_stamp(db_path), now) {
        return Ok(());
    }
    let report = check_database(
        db_path,
        &CheckOptions {
            quick: true,
            embedding_model: None,
        },
    )
    .await;
    if report.ok {
        clear_incident(db_path);
    } else if report
        .findings
        .iter()
        .any(|item| item.code == "writers_active")
        || report.findings.iter().all(|item| is_transient(&item.code))
    {
        // Another process is writing, or only the WAL still holds frames
        // (normal while any connection is open; PASSIVE checkpoints never
        // shrink the file). Neither is damage: recording an incident here
        // switched the watcher off until `ax db repair`. Check again next time.
        return Ok(());
    } else if report
        .findings
        .iter()
        .any(|item| is_rebuildable(&item.code))
        && !report
            .findings
            .iter()
            .any(|item| item.code == "not_a_database")
    {
        repair_rebuildable_indexes(db_path).await?;
    } else {
        record_incident(db_path, &report)?;
    }
    write_quick_check_stamp(db_path, now);
    Ok(())
}

const REBUILDABLE: &[&str] = &[
    "integrity",
    "missing_index",
    "fts",
    "orphan_cues",
    "vector_orphan_memory",
    "vector_dangling_row",
];

pub fn is_rebuildable(code: &str) -> bool {
    REBUILDABLE.contains(&code)
}

/// Findings that describe a busy or not yet checkpointed database, not damage.
pub fn is_transient(code: &str) -> bool {
    matches!(code, "wal_uncheckpointed" | "writers_active")
}

/// Rebuilds derived indexes only. Memories, edges, file contents, and unresolved
/// refs stay for `ax db repair`. This path does not copy a backup.
pub async fn repair_rebuildable_indexes(path: &Path) -> Result<Vec<String>, String> {
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let before = check_database(
        path,
        &CheckOptions {
            quick: true,
            embedding_model: None,
        },
    )
    .await;
    if before.findings.iter().any(|item| {
        item.code == "not_a_database" || item.code == "writers_active" || item.code == "missing"
    }) {
        return Ok(Vec::new());
    }
    let rebuildable: Vec<Finding> = before
        .findings
        .into_iter()
        .filter(|item| is_rebuildable(&item.code))
        .collect();
    if rebuildable.is_empty() {
        return Ok(Vec::new());
    }
    let pool = open_pool(path, false).await.map_err(|err| err.detail)?;
    let mut done = Vec::new();
    if let Err(err) = apply(&pool, &rebuildable, &mut done).await {
        pool.close().await;
        return Err(err);
    }
    pool.close().await;
    let after = check_database(
        path,
        &CheckOptions {
            quick: true,
            embedding_model: None,
        },
    )
    .await;
    if after.ok {
        clear_incident(path);
    } else {
        record_incident(path, &after)?;
    }
    Ok(done)
}

fn finding(code: &str, subject: &str, detail: impl Into<String>) -> Finding {
    Finding {
        code: code.to_string(),
        subject: subject.to_string(),
        detail: detail.into(),
    }
}

fn report(path: &Path, findings: Vec<Finding>) -> CheckReport {
    CheckReport {
        path: path.display().to_string(),
        ok: findings.is_empty(),
        findings,
    }
}

#[allow(clippy::too_many_arguments)]
fn finished(
    path: &Path,
    ok: bool,
    proceed: bool,
    dry_run: bool,
    backup: Option<PathBuf>,
    actions: Vec<String>,
    follow_up: Vec<String>,
    remaining: Vec<Finding>,
) -> RepairReport {
    RepairReport {
        path: path.display().to_string(),
        ok,
        proceed,
        dry_run,
        backup: backup.map(|p| p.display().to_string()),
        actions,
        follow_up,
        remaining,
    }
}

fn planned_actions(findings: &[Finding]) -> Vec<String> {
    let mut actions = Vec::new();
    if findings.iter().any(|f| f.code == "not_a_database") {
        actions.push("backup_raw".into());
        return actions;
    }
    if findings.iter().any(|f| f.code == "wal_uncheckpointed") {
        actions.push("checkpoint_wal".into());
    }
    if findings.iter().any(|f| f.code == "schema_behind") {
        actions.push("run_migrations".into());
    }
    if findings.iter().any(|f| f.code == "integrity") {
        actions.push("reindex".into());
    }
    for finding in findings {
        match finding.code.as_str() {
            "missing_index" => actions.push(format!("create_index:{}", finding.subject)),
            "fts" => actions.push(format!("rebuild_fts:{}", finding.subject)),
            "orphan_cues" => actions.push("delete_orphan_cues".into()),
            "orphan_links" => actions.push("delete_orphan_links".into()),
            "orphan_edges" => actions.push("delete_orphan_edges".into()),
            "orphan_file_contents" => actions.push("delete_orphan_file_contents".into()),
            "orphan_unresolved" => actions.push("delete_orphan_unresolved".into()),
            "vector_orphan_memory" | "vector_dangling_row" => {
                let action = "delete_vector_orphans".to_string();
                if !actions.iter().any(|existing| existing == &action) {
                    actions.push(action);
                }
            }
            _ => {}
        }
    }
    actions
}

fn planned_follow_up(findings: &[Finding]) -> Vec<String> {
    let mut follow = Vec::new();
    if findings
        .iter()
        .any(|f| f.code == "foreign_embedding_model" || f.code == "embedding_dimension")
    {
        follow.push("reembed".into());
    }
    if findings.iter().any(|f| {
        f.code == "foreign_embedding_model"
            || f.code == "embedding_dimension"
            || f.code == "vector_orphan_memory"
            || f.code == "vector_dangling_row"
    }) {
        follow.push("rebuild_vectors".into());
    }
    let mut reindex: Vec<String> = findings
        .iter()
        .filter(|item| item.code == "graph_file")
        .map(|item| format!("reindex:{}", item.subject))
        .collect();
    reindex.sort();
    reindex.dedup();
    follow.extend(reindex);
    follow
}

fn reindex_path_ok(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && path
            .split(['/', '\\'])
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

async fn repair_unreadable(
    path: &Path,
    options: &RepairOptions,
    findings: Vec<Finding>,
) -> RepairReport {
    let backup_dir = options
        .backup_dir
        .clone()
        .unwrap_or_else(|| default_backup_dir(path));
    if let Err(err) = std::fs::create_dir_all(&backup_dir) {
        let mut remaining = findings;
        remaining.push(finding(
            "io",
            "",
            format!("backup directory was not created: {err}"),
        ));
        return finished(path, false, false, false, None, vec![], vec![], remaining);
    }
    match copy_bytes(path, &backup_dir) {
        Ok(backup) => {
            let mut actions = vec!["backup_raw".to_string()];
            let mut remaining = findings;
            if options.salvage {
                actions.push("salvage_refused".into());
                remaining.push(finding(
                    "salvage_refused",
                    "",
                    "page recovery is not available; the original file was left in place",
                ));
            }
            finished(
                path,
                false,
                false,
                false,
                Some(backup),
                actions,
                vec![],
                remaining,
            )
        }
        Err(detail) => {
            let mut remaining = findings;
            remaining.push(finding("io", "", detail));
            finished(path, false, false, false, None, vec![], vec![], remaining)
        }
    }
}

async fn open_pool(path: &Path, read_only: bool) -> Result<SqlitePool, OpenError> {
    crate::vec_ext::install_sqlite_vec();
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .read_only(read_only)
        .busy_timeout(LOCK_WAIT)
        .disable_statement_logging();
    SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(LOCK_WAIT)
        .connect_with(options)
        .await
        .map_err(|err| classify_open(&err.to_string()))
}

fn classify_open(message: &str) -> OpenError {
    let lower = message.to_lowercase();
    let code = if lower.contains("not a database") {
        "not_a_database"
    } else if lower.contains("locked") || lower.contains("busy") {
        "writers_active"
    } else {
        "io"
    };
    OpenError {
        code,
        detail: message.to_string(),
    }
}

async fn collect(pool: &SqlitePool, options: &CheckOptions, include_fts: bool) -> Vec<Finding> {
    let mut findings = Vec::new();
    match pragma_rows(
        pool,
        if options.quick {
            "PRAGMA quick_check"
        } else {
            "PRAGMA integrity_check"
        },
    )
    .await
    {
        Err(err) if is_unreadable(&err) => return vec![finding("not_a_database", "", err)],
        Err(err) if is_locked(&err) => findings.push(finding("writers_active", "", err)),
        Ok(rows) if rows.len() == 1 && rows[0] == "ok" => {}
        Ok(rows) => findings.push(finding("integrity", "", truncate(&rows.join("; ")))),
        Err(err) => findings.push(finding("integrity", "", err)),
    }
    match count(pool, "SELECT COUNT(*) FROM pragma_foreign_key_check").await {
        Ok(0) => {}
        Ok(n) => findings.push(finding(
            "foreign_key",
            "",
            format!("{n} foreign key rows point at a missing parent"),
        )),
        Err(err) if is_unreadable(&err) => return vec![finding("not_a_database", "", err)],
        Err(err) => findings.push(step_finding(err)),
    }
    if let Err(err) = push_schema(pool, &mut findings).await {
        if is_unreadable(&err) {
            return vec![finding("not_a_database", "", err)];
        }
        findings.push(step_finding(err));
    }
    if include_fts {
        if let Err(err) = push_fts(pool, &mut findings).await {
            if is_unreadable(&err) {
                return vec![finding("not_a_database", "", err)];
            }
            findings.push(step_finding(err));
        }
    }
    if let Err(err) = push_indexes(pool, &mut findings).await {
        findings.push(step_finding(err));
    }
    if let Err(err) = push_orphans(pool, &mut findings).await {
        findings.push(step_finding(err));
    }
    if let Err(err) = push_graph_orphans(pool, &mut findings).await {
        findings.push(step_finding(err));
    }
    if let Err(err) = push_embedding_dimension(pool, &mut findings).await {
        findings.push(step_finding(err));
    }
    if let Err(err) = push_vectors(pool, &mut findings).await {
        findings.push(step_finding(err));
    }
    if let Err(err) = push_models(pool, options, &mut findings).await {
        findings.push(step_finding(err));
    }
    if findings.iter().any(|item| item.code == "not_a_database") {
        findings.retain(|item| item.code == "not_a_database");
        findings.truncate(1);
    }
    findings
}

fn is_unreadable(message: &str) -> bool {
    // "malformed" is also how FTS reports a broken index inside a real database.
    // Only a file SQLite refuses to open is not a database.
    message.to_lowercase().contains("not a database")
}

async fn write_lock_blocked(path: &Path) -> Option<String> {
    let pool = open_pool(path, false).await.ok()?;
    let blocked = match sqlx::query("BEGIN IMMEDIATE").execute(&pool).await {
        Err(err) if is_locked(&err.to_string()) => Some(err.to_string()),
        Ok(_) => {
            let _ = sqlx::query("ROLLBACK").execute(&pool).await;
            None
        }
        Err(_) => None,
    };
    pool.close().await;
    blocked
}

fn annotate_writers(path: &Path, findings: &mut [Finding]) {
    if !findings.iter().any(|item| item.code == "writers_active") {
        return;
    }
    let holders = ax_utils::process::file_holders(path);
    for item in findings
        .iter_mut()
        .filter(|item| item.code == "writers_active")
    {
        item.detail = ax_utils::process::with_holders(&item.detail, &holders);
    }
}

fn is_locked(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("locked") || lower.contains("busy")
}

fn step_finding(err: String) -> Finding {
    if is_locked(&err) {
        finding("writers_active", "", err)
    } else if is_unreadable(&err) {
        finding("not_a_database", "", err)
    } else {
        finding("io", "", err)
    }
}

async fn pragma_rows(pool: &SqlitePool, sql: &str) -> Result<Vec<String>, String> {
    sqlx::query_scalar::<_, String>(sql)
        .fetch_all(pool)
        .await
        .map_err(|err| err.to_string())
}

async fn count(pool: &SqlitePool, sql: &str) -> Result<i64, String> {
    sqlx::query_scalar::<_, i64>(sql)
        .fetch_one(pool)
        .await
        .map_err(|err| err.to_string())
}

async fn table_exists(pool: &SqlitePool, name: &str) -> Result<bool, String> {
    let hit: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM sqlite_master WHERE name = ?1 AND type = 'table'")
            .bind(name)
            .fetch_optional(pool)
            .await
            .map_err(|err| err.to_string())?;
    Ok(hit.is_some())
}

async fn push_schema(pool: &SqlitePool, findings: &mut Vec<Finding>) -> Result<(), String> {
    if !table_exists(pool, "nodes").await? || !table_exists(pool, "schema_versions").await? {
        return Ok(());
    }
    let version = get_current_version(pool)
        .await
        .map_err(|err| err.to_string())?;
    if version < CURRENT_SCHEMA_VERSION {
        findings.push(finding(
            "schema_behind",
            &version.to_string(),
            format!("schema version {version} is behind {CURRENT_SCHEMA_VERSION}"),
        ));
    }
    Ok(())
}

fn wal_has_frames(path: &Path) -> bool {
    let wal = PathBuf::from(format!("{}-wal", path.display()));
    // A checkpointed log is absent or a 32-byte header. Frames make it larger.
    std::fs::metadata(wal)
        .map(|meta| meta.len() > 32)
        .unwrap_or(false)
}

async fn push_fts(pool: &SqlitePool, findings: &mut Vec<Finding>) -> Result<(), String> {
    let names = fts_tables(pool).await?;
    for name in names {
        // rank 1 compares the index with the content table. Without it, a deleted
        // index row is invisible to the check.
        let sql = format!("INSERT INTO \"{name}\"(\"{name}\", rank) VALUES ('integrity-check', 1)");
        if let Err(err) = sqlx::query(&sql).execute(pool).await {
            let detail = err.to_string();
            if is_locked(&detail) {
                findings.push(finding("writers_active", &name, detail));
            } else if detail.to_lowercase().contains("readonly")
                || detail.to_lowercase().contains("read-only")
            {
                // A read-only connection cannot run the full-text command. The caller retries it writable.
            } else if is_unreadable(&detail) {
                return Err(detail);
            } else {
                findings.push(finding("fts", &name, detail));
            }
        }
    }
    Ok(())
}

async fn fts_tables(pool: &SqlitePool) -> Result<Vec<String>, String> {
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND sql LIKE '%fts5%'",
    )
    .fetch_all(pool)
    .await
    .map_err(|err| err.to_string())?;
    Ok(names.into_iter().filter(|name| safe_ident(name)).collect())
}

async fn push_indexes(pool: &SqlitePool, findings: &mut Vec<Finding>) -> Result<(), String> {
    for (index, table, _) in MEMORY_INDEXES {
        if !table_exists(pool, table).await? {
            continue;
        }
        let hit: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM sqlite_master WHERE name = ?1 AND type = 'index'")
                .bind(index)
                .fetch_optional(pool)
                .await
                .map_err(|err| err.to_string())?;
        if hit.is_none() {
            findings.push(finding(
                "missing_index",
                index,
                format!("{index} is missing"),
            ));
        }
    }
    Ok(())
}

async fn push_orphans(pool: &SqlitePool, findings: &mut Vec<Finding>) -> Result<(), String> {
    if table_exists(pool, "memory_cues").await? && table_exists(pool, "memories").await? {
        let n = count(
            pool,
            "SELECT COUNT(*) FROM memory_cues WHERE memory_id NOT IN (SELECT id FROM memories)",
        )
        .await?;
        if n > 0 {
            findings.push(finding(
                "orphan_cues",
                &n.to_string(),
                format!("{n} cues point at a missing memory"),
            ));
        }
    }
    if table_exists(pool, "memory_links").await? && table_exists(pool, "memories").await? {
        let n = count(
            pool,
            "SELECT COUNT(*) FROM memory_links
             WHERE from_id NOT IN (SELECT id FROM memories)
                OR to_id NOT IN (SELECT id FROM memories)",
        )
        .await?;
        if n > 0 {
            findings.push(finding(
                "orphan_links",
                &n.to_string(),
                format!("{n} links point at a missing memory"),
            ));
        }
    }
    Ok(())
}

/// Byte length of one hash-v1 embedding: 256 little-endian f32 values.
const EMBED_BYTES: i64 = 256 * 4;

async fn push_graph_orphans(pool: &SqlitePool, findings: &mut Vec<Finding>) -> Result<(), String> {
    if table_exists(pool, "edges").await? && table_exists(pool, "nodes").await? {
        let n = count(
            pool,
            "SELECT COUNT(*) FROM edges
             WHERE source NOT IN (SELECT id FROM nodes)
                OR target NOT IN (SELECT id FROM nodes)",
        )
        .await?;
        if n > 0 {
            findings.push(finding(
                "orphan_edges",
                &n.to_string(),
                format!("{n} edges point at a missing node"),
            ));
            let paths = sqlx::query_scalar::<_, String>(
                "SELECT DISTINCT n.file_path
                 FROM edges e
                 JOIN nodes n ON n.id = e.source OR n.id = e.target
                 WHERE n.file_path != ''
                   AND (e.source NOT IN (SELECT id FROM nodes)
                        OR e.target NOT IN (SELECT id FROM nodes))
                 ORDER BY n.file_path",
            )
            .fetch_all(pool)
            .await
            .map_err(|err| err.to_string())?;
            for path in paths {
                if reindex_path_ok(&path) {
                    findings.push(finding(
                        "graph_file",
                        &path,
                        "graph rows in this file point at a missing node",
                    ));
                }
            }
        }
    }
    if table_exists(pool, "file_contents").await? && table_exists(pool, "files").await? {
        let n = count(
            pool,
            "SELECT COUNT(*) FROM file_contents WHERE path NOT IN (SELECT path FROM files)",
        )
        .await?;
        if n > 0 {
            findings.push(finding(
                "orphan_file_contents",
                &n.to_string(),
                format!("{n} file contents have no file row"),
            ));
        }
    }
    if table_exists(pool, "unresolved_refs").await? && table_exists(pool, "nodes").await? {
        let n = count(
            pool,
            "SELECT COUNT(*) FROM unresolved_refs
             WHERE from_node_id NOT IN (SELECT id FROM nodes)",
        )
        .await?;
        if n > 0 {
            findings.push(finding(
                "orphan_unresolved",
                &n.to_string(),
                format!("{n} unresolved refs point at a missing node"),
            ));
        }
    }
    Ok(())
}

async fn push_embedding_dimension(
    pool: &SqlitePool,
    findings: &mut Vec<Finding>,
) -> Result<(), String> {
    if !table_exists(pool, "memories").await? {
        return Ok(());
    }
    let column = count(
        pool,
        "SELECT COUNT(*) FROM pragma_table_info('memories') WHERE name = 'embedding'",
    )
    .await?;
    if column == 0 {
        return Ok(());
    }
    let n = count(
        pool,
        &format!(
            "SELECT COUNT(*) FROM memories
             WHERE embedding IS NOT NULL
               AND length(embedding) > 0
               AND length(embedding) != {EMBED_BYTES}"
        ),
    )
    .await?;
    if n > 0 {
        findings.push(finding(
            "embedding_dimension",
            &n.to_string(),
            format!("{n} memories have an embedding that is not {EMBED_BYTES} bytes"),
        ));
    }
    Ok(())
}

async fn push_vectors(pool: &SqlitePool, findings: &mut Vec<Finding>) -> Result<(), String> {
    if !table_exists(pool, "memory_vec_map").await? || !table_exists(pool, "memories").await? {
        return Ok(());
    }
    let orphans = count(
        pool,
        "SELECT COUNT(*) FROM memory_vec_map WHERE memory_id NOT IN (SELECT id FROM memories)",
    )
    .await?;
    if orphans > 0 {
        findings.push(finding(
            "vector_orphan_memory",
            &orphans.to_string(),
            format!("{orphans} vector rows point at a missing memory"),
        ));
    }
    if table_exists(pool, "memory_vec").await? {
        let dangling = count(
            pool,
            "SELECT COUNT(*) FROM memory_vec_map
             WHERE NOT EXISTS (SELECT 1 FROM memory_vec WHERE memory_vec.rowid = memory_vec_map.rowid)",
        )
        .await?;
        if dangling > 0 {
            findings.push(finding(
                "vector_dangling_row",
                &dangling.to_string(),
                format!("{dangling} vector map rows point at a missing vector"),
            ));
        }
    }
    Ok(())
}

async fn push_models(
    pool: &SqlitePool,
    options: &CheckOptions,
    findings: &mut Vec<Finding>,
) -> Result<(), String> {
    let Some(expected) = &options.embedding_model else {
        return Ok(());
    };
    if !table_exists(pool, "memories").await? {
        return Ok(());
    }
    let rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT embedding_model, COUNT(*) FROM memories GROUP BY embedding_model")
            .fetch_all(pool)
            .await
            .map_err(|err| err.to_string())?;
    for (model, n) in rows {
        if &model != expected && n > 0 {
            findings.push(finding(
                "foreign_embedding_model",
                &model,
                format!("{n} memories use {model}; the active model is {expected}"),
            ));
        }
    }
    Ok(())
}

async fn apply(
    pool: &SqlitePool,
    findings: &[Finding],
    done: &mut Vec<String>,
) -> Result<(), String> {
    if findings.iter().any(|f| f.code == "wal_uncheckpointed") {
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(pool)
            .await
            .map_err(|err| err.to_string())?;
        done.push("checkpoint_wal".into());
    }
    if findings.iter().any(|f| f.code == "schema_behind") {
        let version = get_current_version(pool)
            .await
            .map_err(|err| err.to_string())?;
        run_migrations(pool, version)
            .await
            .map_err(|err| err.to_string())?;
        done.push("run_migrations".into());
    }
    if findings.iter().any(|f| f.code == "integrity") {
        sqlx::query("REINDEX")
            .execute(pool)
            .await
            .map_err(|err| err.to_string())?;
        done.push("reindex".into());
    }
    for finding in findings {
        if finding.code == "missing_index" {
            if let Some((_, _, sql)) = MEMORY_INDEXES
                .iter()
                .find(|(name, _, _)| *name == finding.subject)
            {
                sqlx::query(sql)
                    .execute(pool)
                    .await
                    .map_err(|err| err.to_string())?;
                done.push(format!("create_index:{}", finding.subject));
            }
        }
        if finding.code == "fts" && safe_ident(&finding.subject) {
            let name = &finding.subject;
            let sql = format!("INSERT INTO \"{name}\"(\"{name}\") VALUES ('rebuild')");
            sqlx::query(&sql)
                .execute(pool)
                .await
                .map_err(|err| err.to_string())?;
            done.push(format!("rebuild_fts:{name}"));
        }
    }
    if findings.iter().any(|f| f.code == "orphan_cues") {
        sqlx::query("DELETE FROM memory_cues WHERE memory_id NOT IN (SELECT id FROM memories)")
            .execute(pool)
            .await
            .map_err(|err| err.to_string())?;
        done.push("delete_orphan_cues".into());
    }
    if findings.iter().any(|f| f.code == "orphan_links") {
        sqlx::query(
            "DELETE FROM memory_links
             WHERE from_id NOT IN (SELECT id FROM memories)
                OR to_id NOT IN (SELECT id FROM memories)",
        )
        .execute(pool)
        .await
        .map_err(|err| err.to_string())?;
        done.push("delete_orphan_links".into());
    }
    if findings.iter().any(|f| f.code == "orphan_edges") {
        sqlx::query(
            "DELETE FROM edges
             WHERE source NOT IN (SELECT id FROM nodes)
                OR target NOT IN (SELECT id FROM nodes)",
        )
        .execute(pool)
        .await
        .map_err(|err| err.to_string())?;
        done.push("delete_orphan_edges".into());
    }
    if findings.iter().any(|f| f.code == "orphan_file_contents") {
        sqlx::query("DELETE FROM file_contents WHERE path NOT IN (SELECT path FROM files)")
            .execute(pool)
            .await
            .map_err(|err| err.to_string())?;
        done.push("delete_orphan_file_contents".into());
    }
    if findings.iter().any(|f| f.code == "orphan_unresolved") {
        sqlx::query("DELETE FROM unresolved_refs WHERE from_node_id NOT IN (SELECT id FROM nodes)")
            .execute(pool)
            .await
            .map_err(|err| err.to_string())?;
        done.push("delete_orphan_unresolved".into());
    }
    if findings
        .iter()
        .any(|f| f.code == "vector_orphan_memory" || f.code == "vector_dangling_row")
    {
        sqlx::query("DELETE FROM memory_vec_map WHERE memory_id NOT IN (SELECT id FROM memories)")
            .execute(pool)
            .await
            .map_err(|err| err.to_string())?;
        if table_exists(pool, "memory_vec").await? {
            sqlx::query(
                "DELETE FROM memory_vec_map
                 WHERE NOT EXISTS (SELECT 1 FROM memory_vec WHERE memory_vec.rowid = memory_vec_map.rowid)",
            )
            .execute(pool)
            .await
            .map_err(|err| err.to_string())?;
        }
        done.push("delete_vector_orphans".into());
    }
    // Deletes land in the log. Checkpoint before the follow-up check, or that
    // check would report the log this repair just wrote.
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(pool)
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

async fn backup_database(path: &Path, backup_dir: &Path) -> Result<PathBuf, String> {
    match vacuum_into(path, backup_dir).await {
        Ok(dest) => Ok(dest),
        Err(_) => copy_bytes(path, backup_dir),
    }
}

async fn vacuum_into(path: &Path, backup_dir: &Path) -> Result<PathBuf, String> {
    let dest = backup_destination(backup_dir)?;
    let quoted = dest.display().to_string().replace('\'', "''");
    let pool = open_pool(path, true).await.map_err(|err| err.detail)?;
    let written = sqlx::query(&format!("VACUUM INTO '{quoted}'"))
        .execute(&pool)
        .await;
    pool.close().await;
    if let Err(err) = written {
        let _ = std::fs::remove_file(&dest);
        return Err(err.to_string());
    }
    stabilize_backup(&dest).await?;
    write_blake3(&dest)?;
    Ok(dest)
}

async fn stabilize_backup(dest: &Path) -> Result<(), String> {
    let pool = open_pool(dest, false).await.map_err(|err| err.detail)?;
    let mode = sqlx::query("PRAGMA journal_mode = WAL")
        .execute(&pool)
        .await;
    let checkpoint = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&pool)
        .await;
    pool.close().await;
    mode.map_err(|err| err.to_string())?;
    checkpoint.map_err(|err| err.to_string())?;
    for suffix in ["-wal", "-shm"] {
        let side = PathBuf::from(format!("{}{suffix}", dest.display()));
        if side.is_file() {
            let len = std::fs::metadata(&side).map(|meta| meta.len()).unwrap_or(0);
            if len <= 32 {
                let _ = std::fs::remove_file(&side);
            }
        }
    }
    Ok(())
}

fn backup_destination(backup_dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(backup_dir).map_err(|err| err.to_string())?;
    let seq = BACKUP_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Ok(backup_dir.join(format!("ax-{nanos}-{seq}.db")))
}

fn write_blake3(dest: &Path) -> Result<(), String> {
    let bytes = std::fs::read(dest).map_err(|err| err.to_string())?;
    let hash = blake3::hash(&bytes);
    let sidecar = PathBuf::from(format!("{}.blake3", dest.display()));
    std::fs::write(&sidecar, hash.to_hex().as_bytes()).map_err(|err| err.to_string())
}

fn copy_bytes(source: &Path, backup_dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(backup_dir).map_err(|err| err.to_string())?;
    let seq = BACKUP_SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dest = backup_dir.join(format!("ax-{nanos}-{seq}.db"));
    std::fs::copy(source, &dest).map_err(|err| err.to_string())?;
    for suffix in ["-wal", "-shm"] {
        let from = PathBuf::from(format!("{}{suffix}", source.display()));
        if from.is_file() {
            let to = PathBuf::from(format!("{}{suffix}", dest.display()));
            std::fs::copy(&from, &to).map_err(|err| err.to_string())?;
        }
    }
    let bytes = std::fs::read(&dest).map_err(|err| err.to_string())?;
    let hash = blake3::hash(&bytes);
    let sidecar = PathBuf::from(format!("{}.blake3", dest.display()));
    std::fs::write(&sidecar, hash.to_hex().as_bytes()).map_err(|err| err.to_string())?;
    Ok(dest)
}

fn default_backup_dir(db: &Path) -> PathBuf {
    let parent = db.parent().unwrap_or_else(|| Path::new("."));
    if parent.file_name().and_then(|name| name.to_str()) == Some(".ax") {
        parent.join("backups").join("db")
    } else {
        let name = db
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("ax.db");
        parent.join(format!("{name}.backups"))
    }
}

fn safe_ident(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn truncate(text: &str) -> String {
    if text.len() <= 500 {
        text.to_string()
    } else {
        format!("{}…", &text[..500])
    }
}
