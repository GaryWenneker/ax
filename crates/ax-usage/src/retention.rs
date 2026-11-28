//! Opt-in retention for history ax otherwise keeps forever.
//!
//! By default nothing here deletes anything: activity events, durable
//! conversation entries and the dated MCP verbose log files are all kept.
//! A project turns retention on per kind in `.ax/ship.toml`:
//!
//! ```toml
//! [retention]
//! activity_days = 180           # ax_activity_event rows older than 180 days
//! activity_max_bytes = 50000000 # newest rows kept up to ~50 MB of text
//! durable_days = 365            # ax_durable_entry rows of this project
//! durable_max_bytes = 0         # 0 or missing = no limit
//! verbose_log_days = 30         # .ax/mcp-verbose-YYYY-MM-DD.log files
//! verbose_log_max_bytes = 0
//! ```
//!
//! A missing, zero or unreadable value means "keep everything". Cleanup runs
//! where the existing cache TTL cleanup runs: when a row of that kind is
//! written (activity, durable) and when the verbose log rotates to a new day.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use chrono::NaiveDate;
use serde::Deserialize;
use sqlx::SqlitePool;

const DAY_SECS: i64 = 24 * 60 * 60;
/// Database cleanup runs at most this often per project and kind in one
/// process, so a busy tool loop does not sum a project's rows on every call.
const DB_PRUNE_INTERVAL_SECS: i64 = 5 * 60;

/// A retention limit for one kind of history. `None` means no limit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetentionLimit {
    /// Delete entries older than this many days.
    pub days: Option<u32>,
    /// Keep the newest entries up to this many bytes; delete older ones.
    pub max_bytes: Option<u64>,
}

impl RetentionLimit {
    pub fn is_unlimited(&self) -> bool {
        self.days.is_none() && self.max_bytes.is_none()
    }

    fn new(days: Option<u32>, max_bytes: Option<u64>) -> Self {
        Self {
            days: days.filter(|d| *d > 0),
            max_bytes: max_bytes.filter(|b| *b > 0),
        }
    }
}

/// The `[retention]` section of a project's `.ax/ship.toml`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetentionSettings {
    pub activity: RetentionLimit,
    pub durable: RetentionLimit,
    pub verbose_log: RetentionLimit,
}

#[derive(Debug, Deserialize, Default)]
struct ShipRetentionFile {
    #[serde(default)]
    retention: RetentionSection,
}

#[derive(Debug, Deserialize, Default)]
struct RetentionSection {
    activity_days: Option<u32>,
    activity_max_bytes: Option<u64>,
    durable_days: Option<u32>,
    durable_max_bytes: Option<u64>,
    verbose_log_days: Option<u32>,
    verbose_log_max_bytes: Option<u64>,
}

/// Parse the `[retention]` section from ship.toml text. Text that does not
/// parse gives the default (keep everything), so a typo never deletes data.
pub fn parse_retention_settings(text: &str) -> RetentionSettings {
    let section = match toml::from_str::<ShipRetentionFile>(text) {
        Ok(file) => file.retention,
        Err(err) => {
            tracing::warn!("ship.toml [retention] ignored, keeping all history: {err}");
            return RetentionSettings::default();
        }
    };
    RetentionSettings {
        activity: RetentionLimit::new(section.activity_days, section.activity_max_bytes),
        durable: RetentionLimit::new(section.durable_days, section.durable_max_bytes),
        verbose_log: RetentionLimit::new(section.verbose_log_days, section.verbose_log_max_bytes),
    }
}

/// Retention settings for `project_root`; the default when ship.toml is missing.
pub fn read_retention_settings(project_root: &Path) -> RetentionSettings {
    match std::fs::read_to_string(project_root.join(".ax").join("ship.toml")) {
        Ok(text) => parse_retention_settings(&text),
        Err(_) => RetentionSettings::default(),
    }
}

/// True when a database prune for (`kind`, `project`) has not run within
/// [`DB_PRUNE_INTERVAL_SECS`]; records `now_secs` as the last run.
fn db_prune_due(kind: &str, project: &str, now_secs: i64) -> bool {
    static LAST: Mutex<Option<HashMap<(String, String), i64>>> = Mutex::new(None);
    let Ok(mut guard) = LAST.lock() else {
        return false;
    };
    let map = guard.get_or_insert_with(HashMap::new);
    let key = (kind.to_string(), project.to_string());
    if map
        .get(&key)
        .is_some_and(|last| now_secs - last < DB_PRUNE_INTERVAL_SECS)
    {
        return false;
    }
    map.insert(key, now_secs);
    true
}

/// Size of one activity row's text columns, in bytes.
const ACTIVITY_ROW_BYTES: &str = "length(CAST(summary AS BLOB)) + length(CAST(operation AS BLOB))
     + length(CAST(files_json AS BLOB)) + length(CAST(symbols_json AS BLOB))
     + length(CAST(commit_ids_json AS BLOB)) + COALESCE(length(CAST(error AS BLOB)), 0)";

/// Delete this project's activity events past `limit`. Returns rows deleted.
pub(crate) async fn prune_activity(
    pool: &SqlitePool,
    project_id: &str,
    limit: RetentionLimit,
    now_ms: i64,
) -> Result<u64, String> {
    let mut deleted = 0;
    if let Some(days) = limit.days {
        deleted +=
            sqlx::query("DELETE FROM ax_activity_event WHERE project_id = ? AND timestamp_ms < ?")
                .bind(project_id)
                .bind(now_ms - i64::from(days) * DAY_SECS * 1000)
                .execute(pool)
                .await
                .map_err(|e| e.to_string())?
                .rows_affected();
    }
    if let Some(max_bytes) = limit.max_bytes {
        let sql = format!(
            "DELETE FROM ax_activity_event WHERE event_id IN (
               SELECT event_id FROM (
                 SELECT event_id, SUM({ACTIVITY_ROW_BYTES}) OVER (
                   ORDER BY timestamp_ms DESC, event_id DESC ROWS UNBOUNDED PRECEDING
                 ) AS running
                 FROM ax_activity_event WHERE project_id = ?
               ) WHERE running > ?
             )"
        );
        deleted += sqlx::query(&sql)
            .bind(project_id)
            .bind(i64::try_from(max_bytes).unwrap_or(i64::MAX))
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?
            .rows_affected();
    }
    Ok(deleted)
}

/// Delete durable conversation entries of this project's conversations past
/// `limit`. Legacy conversations without a project scope are never touched.
pub(crate) async fn prune_durable(
    pool: &SqlitePool,
    project: &str,
    limit: RetentionLimit,
    now_secs: i64,
) -> Result<u64, String> {
    const SCOPED: &str =
        "conversation_id IN (SELECT conversation FROM ax_durable_scope WHERE project = ?)";
    let mut deleted = 0;
    if let Some(days) = limit.days {
        deleted += sqlx::query(&format!(
            "DELETE FROM ax_durable_entry WHERE {SCOPED} AND created_at < ?"
        ))
        .bind(project)
        .bind(now_secs - i64::from(days) * DAY_SECS)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected();
    }
    if let Some(max_bytes) = limit.max_bytes {
        deleted += sqlx::query(&format!(
            "DELETE FROM ax_durable_entry WHERE id IN (
               SELECT id FROM (
                 SELECT id, SUM(length(CAST(body AS BLOB))) OVER (
                   ORDER BY id DESC ROWS UNBOUNDED PRECEDING
                 ) AS running
                 FROM ax_durable_entry WHERE {SCOPED}
               ) WHERE running > ?
             )"
        ))
        .bind(project)
        .bind(i64::try_from(max_bytes).unwrap_or(i64::MAX))
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected();
    }
    Ok(deleted)
}

/// Apply the project's activity retention, at most once per interval.
pub(crate) async fn maybe_prune_activity(pool: &SqlitePool, root: &Path, project_id: &str) {
    let now_ms = chrono::Utc::now().timestamp_millis();
    if !db_prune_due("activity", project_id, now_ms / 1000) {
        return;
    }
    let limit = read_retention_settings(root).activity;
    if limit.is_unlimited() {
        return;
    }
    if let Err(err) = prune_activity(pool, project_id, limit, now_ms).await {
        tracing::warn!("activity retention failed: {err}");
    }
}

/// Apply the project's durable-entry retention, at most once per interval.
pub(crate) async fn maybe_prune_durable(pool: &SqlitePool, root: &Path) {
    let project = crate::context_cache::project_scope(root);
    let now_secs = chrono::Utc::now().timestamp();
    if !db_prune_due("durable", &project, now_secs) {
        return;
    }
    let limit = read_retention_settings(root).durable;
    if limit.is_unlimited() {
        return;
    }
    if let Err(err) = prune_durable(pool, &project, limit, now_secs).await {
        tracing::warn!("durable retention failed: {err}");
    }
}

/// Delete dated verbose log files past `limit`. The file for `today` is never
/// deleted, and legacy undated files are left alone. Returns files deleted.
///
/// A file dated D is older than N days when D < today − N. For the byte cap
/// the newest files are kept until their total size passes the cap.
pub fn prune_verbose_logs(project_root: &Path, limit: RetentionLimit, today: NaiveDate) -> u64 {
    if limit.is_unlimited() {
        return 0;
    }
    let mut files: Vec<(NaiveDate, std::path::PathBuf)> =
        crate::mcp_verbose_log::list_dated_log_files(project_root)
            .into_iter()
            .filter(|(_, path)| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(crate::mcp_verbose_log::DATED_LOG_PREFIX))
            })
            .collect();
    // Newest first, so the byte cap keeps the most recent history.
    files.sort_by(|a, b| b.cmp(a));
    let cutoff = limit
        .days
        .and_then(|d| today.checked_sub_days(chrono::Days::new(u64::from(d))));
    let mut kept_bytes = 0u64;
    let mut deleted = 0;
    for (day, path) in files {
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if day >= today {
            kept_bytes = kept_bytes.saturating_add(size);
            continue;
        }
        let too_old = cutoff.is_some_and(|c| day < c);
        let over_cap = limit
            .max_bytes
            .is_some_and(|cap| kept_bytes.saturating_add(size) > cap);
        if too_old || over_cap {
            if std::fs::remove_file(&path).is_ok() {
                deleted += 1;
            }
        } else {
            kept_bytes = kept_bytes.saturating_add(size);
        }
    }
    deleted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_keeps_everything() {
        assert_eq!(parse_retention_settings(""), RetentionSettings::default());
        assert!(RetentionSettings::default().activity.is_unlimited());
        assert!(RetentionSettings::default().durable.is_unlimited());
        assert!(RetentionSettings::default().verbose_log.is_unlimited());
    }

    #[test]
    fn parses_each_kind_and_treats_zero_as_unset() {
        let s = parse_retention_settings(
            "[ui]\ntimezone = \"UTC\"\n[retention]\nactivity_days = 90\nactivity_max_bytes = 0\n\
             durable_max_bytes = 1000\nverbose_log_days = 30\nverbose_log_max_bytes = 5000\n",
        );
        assert_eq!(s.activity, RetentionLimit::new(Some(90), None));
        assert_eq!(s.durable, RetentionLimit::new(None, Some(1000)));
        assert_eq!(s.verbose_log, RetentionLimit::new(Some(30), Some(5000)));
    }

    #[test]
    fn invalid_values_keep_everything() {
        let s = parse_retention_settings("[retention]\nactivity_days = \"soon\"\n");
        assert_eq!(s, RetentionSettings::default());
        let s = parse_retention_settings("[retention]\nactivity_days = -3\n");
        assert_eq!(s, RetentionSettings::default());
        let s = parse_retention_settings("not toml [[[");
        assert_eq!(s, RetentionSettings::default());
    }

    #[test]
    fn db_prune_is_throttled_per_project_and_kind() {
        let project = format!("throttle-{}", std::process::id());
        assert!(db_prune_due("activity", &project, 1_000));
        assert!(!db_prune_due("activity", &project, 1_000 + 10));
        assert!(db_prune_due("durable", &project, 1_000 + 10));
        assert!(db_prune_due(
            "activity",
            &project,
            1_000 + DB_PRUNE_INTERVAL_SECS
        ));
    }

    async fn pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = crate::store::open_pool_at(&dir.path().join("usage.db"))
            .await
            .unwrap();
        (dir, pool)
    }

    async fn insert_activity(pool: &SqlitePool, project: &str, id: &str, ts: i64, summary: &str) {
        sqlx::query(
            "INSERT INTO ax_activity_event (event_id, idempotency_key, timestamp_ms, source,
               project_id, operation, status, summary)
             VALUES (?, ?, ?, 'cli', ?, 'op', 'success', ?)",
        )
        .bind(id)
        .bind(id)
        .bind(ts)
        .bind(project)
        .bind(summary)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn activity_ids(pool: &SqlitePool) -> Vec<String> {
        sqlx::query_scalar("SELECT event_id FROM ax_activity_event ORDER BY event_id")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn activity_unlimited_deletes_nothing() {
        let (_dir, pool) = pool().await;
        insert_activity(&pool, "p", "a", 0, "ancient").await;
        let n = prune_activity(&pool, "p", RetentionLimit::default(), 10 * DAY_SECS * 1000)
            .await
            .unwrap();
        assert_eq!(n, 0);
        assert_eq!(activity_ids(&pool).await, ["a"]);
    }

    #[tokio::test]
    async fn activity_days_deletes_only_old_rows_of_this_project() {
        let (_dir, pool) = pool().await;
        let now = 100 * DAY_SECS * 1000;
        insert_activity(&pool, "p", "old", now - 31 * DAY_SECS * 1000, "x").await;
        insert_activity(&pool, "p", "new", now - 29 * DAY_SECS * 1000, "x").await;
        insert_activity(&pool, "other", "other-old", now - 31 * DAY_SECS * 1000, "x").await;
        let n = prune_activity(&pool, "p", RetentionLimit::new(Some(30), None), now)
            .await
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(activity_ids(&pool).await, ["new", "other-old"]);
    }

    #[tokio::test]
    async fn activity_byte_cap_keeps_the_newest_rows() {
        let (_dir, pool) = pool().await;
        // Each row: summary 100 bytes + "op" 2 + "[]" x3 6 = 108 bytes.
        let body = "s".repeat(100);
        for (i, id) in ["e1", "e2", "e3", "e4"].iter().enumerate() {
            insert_activity(&pool, "p", id, i as i64 * 1000, &body).await;
        }
        let n = prune_activity(&pool, "p", RetentionLimit::new(None, Some(250)), 10_000)
            .await
            .unwrap();
        assert_eq!(n, 2);
        assert_eq!(activity_ids(&pool).await, ["e3", "e4"]);
    }

    async fn insert_entry(pool: &SqlitePool, conversation: &str, body: &str, created_at: i64) {
        sqlx::query(
            "INSERT INTO ax_durable_entry (conversation_id, kind, body, created_at)
             VALUES (?, 'tool', ?, ?)",
        )
        .bind(conversation)
        .bind(body)
        .bind(created_at)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn scope(pool: &SqlitePool, project: &str, conversation: &str) {
        sqlx::query(
            "INSERT INTO ax_durable_scope (project, session, conversation) VALUES (?, ?, ?)",
        )
        .bind(project)
        .bind(conversation)
        .bind(conversation)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn entry_bodies(pool: &SqlitePool) -> Vec<String> {
        sqlx::query_scalar("SELECT body FROM ax_durable_entry ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn durable_days_touches_only_this_projects_conversations() {
        let (_dir, pool) = pool().await;
        let now = 100 * DAY_SECS;
        scope(&pool, "p", "c1").await;
        scope(&pool, "other", "c2").await;
        insert_entry(&pool, "c1", "p-old", now - 10 * DAY_SECS).await;
        insert_entry(&pool, "c1", "p-new", now - DAY_SECS).await;
        insert_entry(&pool, "c2", "other-old", now - 10 * DAY_SECS).await;
        insert_entry(&pool, "legacy", "legacy-old", now - 10 * DAY_SECS).await;
        let n = prune_durable(&pool, "p", RetentionLimit::new(Some(7), None), now)
            .await
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            entry_bodies(&pool).await,
            ["p-new", "other-old", "legacy-old"]
        );
    }

    #[tokio::test]
    async fn durable_byte_cap_keeps_the_newest_entries() {
        let (_dir, pool) = pool().await;
        scope(&pool, "p", "c1").await;
        for body in ["aaaa", "bbbb", "cccc"] {
            insert_entry(&pool, "c1", body, 1).await;
        }
        let n = prune_durable(&pool, "p", RetentionLimit::new(None, Some(8)), 2)
            .await
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(entry_bodies(&pool).await, ["bbbb", "cccc"]);
    }

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn write_log(root: &Path, date: &str, bytes: usize) {
        std::fs::write(
            root.join(".ax").join(format!("mcp-verbose-{date}.log")),
            "x".repeat(bytes),
        )
        .unwrap();
    }

    fn log_days(root: &Path) -> Vec<String> {
        crate::mcp_verbose_log::list_dated_log_files(root)
            .into_iter()
            .map(|(d, _)| d.to_string())
            .collect()
    }

    #[test]
    fn verbose_logs_unlimited_deletes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".ax")).unwrap();
        write_log(tmp.path(), "2020-01-01", 10);
        assert_eq!(
            prune_verbose_logs(tmp.path(), RetentionLimit::default(), day("2026-01-01")),
            0
        );
        assert_eq!(log_days(tmp.path()), ["2020-01-01"]);
    }

    #[test]
    fn verbose_log_days_deletes_files_dated_before_today_minus_n() {
        let tmp = tempfile::tempdir().unwrap();
        let ax = tmp.path().join(".ax");
        std::fs::create_dir_all(&ax).unwrap();
        for d in ["2026-01-01", "2026-01-08", "2026-01-09", "2026-01-10"] {
            write_log(tmp.path(), d, 10);
        }
        let deleted = prune_verbose_logs(
            tmp.path(),
            RetentionLimit::new(Some(2), None),
            day("2026-01-10"),
        );
        // 2026-01-10 minus 2 days is 2026-01-08: only files before it go.
        assert_eq!(deleted, 1);
        assert_eq!(
            log_days(tmp.path()),
            ["2026-01-08", "2026-01-09", "2026-01-10"]
        );
    }

    #[test]
    fn verbose_log_byte_cap_keeps_newest_files_and_never_today() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".ax")).unwrap();
        write_log(tmp.path(), "2026-01-07", 100);
        write_log(tmp.path(), "2026-01-08", 100);
        write_log(tmp.path(), "2026-01-09", 100);
        // Today alone is over the cap; it is still kept.
        write_log(tmp.path(), "2026-01-10", 500);
        let deleted = prune_verbose_logs(
            tmp.path(),
            RetentionLimit::new(None, Some(300)),
            day("2026-01-10"),
        );
        assert_eq!(deleted, 3);
        assert_eq!(log_days(tmp.path()), ["2026-01-10"]);

        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".ax")).unwrap();
        write_log(tmp.path(), "2026-01-08", 100);
        write_log(tmp.path(), "2026-01-09", 100);
        write_log(tmp.path(), "2026-01-10", 100);
        let deleted = prune_verbose_logs(
            tmp.path(),
            RetentionLimit::new(None, Some(250)),
            day("2026-01-10"),
        );
        assert_eq!(deleted, 1);
        assert_eq!(log_days(tmp.path()), ["2026-01-09", "2026-01-10"]);
    }

    #[test]
    fn verbose_log_retention_covers_replicated_dated_files() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join(".ax/server/replicated-logs/node1");
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::write(remote.join("mcp-verbose-2020-01-01.log"), "old").unwrap();
        std::fs::write(remote.join("mcp-verbose-2026-01-10.log"), "new").unwrap();
        let deleted = prune_verbose_logs(
            tmp.path(),
            RetentionLimit::new(Some(7), None),
            day("2026-01-10"),
        );
        assert_eq!(deleted, 1);
        assert_eq!(log_days(tmp.path()), ["2026-01-10"]);
    }
}
