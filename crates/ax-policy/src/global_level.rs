//! Read-only view of the global level in `~/.ax/global.db`.
//!
//! A rule or skill stored there applies to every project, so a project copy is only kept
//! when it is the more extensive one (see [`lower_copy_wins`]).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Rule,
    Skill,
}

impl Kind {
    pub fn table(self) -> &'static str {
        match self {
            Kind::Rule => "global_policy_rules",
            Kind::Skill => "global_policy_skills",
        }
    }

    /// The `kind` value in `policy_revisions` and `global_policy_revisions`.
    pub fn revision_kind(self) -> &'static str {
        match self {
            Kind::Rule => "rule",
            Kind::Skill => "skill",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlobalItem {
    pub project_id: i64,
    pub name: String,
    /// Empty when the payload has no readable `body`, so any other copy wins over it.
    pub body: String,
    /// Last content change: the newest revision, else `synced_at`.
    pub changed_ms: i64,
}

#[derive(Debug, Clone, Default)]
pub struct GlobalLevel {
    rules: BTreeMap<String, Vec<GlobalItem>>,
    skills: BTreeMap<String, Vec<GlobalItem>>,
}

impl GlobalLevel {
    pub fn items(&self, kind: Kind) -> &BTreeMap<String, Vec<GlobalItem>> {
        match kind {
            Kind::Rule => &self.rules,
            Kind::Skill => &self.skills,
        }
    }

    /// The copy that wins among the global rows with this name.
    pub fn leader(&self, kind: Kind, name: &str) -> Option<&GlobalItem> {
        self.items(kind)
            .get(name)
            .and_then(|items| items.get(leader_index(items)))
    }

    /// True when a project copy with this body would lose to the global copy.
    pub fn shadows(&self, kind: Kind, name: &str, body: &str, changed_ms: i64) -> bool {
        self.leader(kind, name)
            .is_some_and(|g| !lower_copy_wins(body, changed_ms, &g.body, g.changed_ms))
    }
}

/// The body as compared: trimmed, with Windows line endings read as `\n`.
pub fn normalized(body: &str) -> String {
    body.replace("\r\n", "\n").trim().to_string()
}

pub fn body_len(body: &str) -> usize {
    normalized(body).chars().count()
}

/// Same text apart from surrounding whitespace and line endings.
pub fn same_text(a: &str, b: &str) -> bool {
    normalized(a) == normalized(b)
}

/// The more extensive body wins; on equal length the more recently changed one.
/// Identical bodies never replace the global copy.
pub fn lower_copy_wins(lower: &str, lower_ms: i64, global: &str, global_ms: i64) -> bool {
    if same_text(lower, global) {
        return false;
    }
    let (l, g) = (body_len(lower), body_len(global));
    l > g || (l == g && lower_ms > global_ms)
}

/// Index of the item every other item loses to (the first one on a full tie).
pub fn leader_index(items: &[GlobalItem]) -> usize {
    let mut best = 0;
    for (i, item) in items.iter().enumerate().skip(1) {
        let lead = &items[best];
        if lower_copy_wins(&item.body, item.changed_ms, &lead.body, lead.changed_ms) {
            best = i;
        }
    }
    best
}

async fn has_column(pool: &SqlitePool, table: &str, column: &str) -> Result<bool, sqlx::Error> {
    let found: Option<(String,)> = sqlx::query_as(&format!(
        "SELECT name FROM pragma_table_info('{table}') WHERE name = ?"
    ))
    .bind(column)
    .fetch_optional(pool)
    .await?;
    Ok(found.is_some())
}

async fn has_table(pool: &SqlitePool, table: &str) -> Result<bool, sqlx::Error> {
    let found: Option<(String,)> =
        sqlx::query_as("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_optional(pool)
            .await?;
    Ok(found.is_some())
}

/// Both policy tables have the `level` column and the revisions table exists.
pub async fn schema_is_current(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
    for kind in [Kind::Rule, Kind::Skill] {
        if !has_column(pool, kind.table(), "level").await? {
            return Ok(false);
        }
    }
    has_table(pool, "global_policy_revisions").await
}

async fn load_kind(
    pool: &SqlitePool,
    kind: Kind,
) -> Result<BTreeMap<String, Vec<GlobalItem>>, sqlx::Error> {
    let table = kind.table();
    let mut out: BTreeMap<String, Vec<GlobalItem>> = BTreeMap::new();
    if !has_table(pool, table).await? {
        return Ok(out);
    }
    // Before the `level` column existed every row was global.
    let level_filter = if has_column(pool, table, "level").await? {
        "WHERE g.level = 'global'"
    } else {
        ""
    };
    let with_revisions = has_table(pool, "global_policy_revisions").await?;
    let revision_ms = if with_revisions {
        "(SELECT MAX(r.created_at) FROM global_policy_revisions r
          WHERE r.kind = ? AND r.item_id = g.item_id AND r.project_id = g.project_id)"
    } else {
        "NULL"
    };
    let sql = format!(
        "SELECT g.project_id, g.item_id, g.payload,
                COALESCE({revision_ms}, CAST(strftime('%s', g.synced_at) AS INTEGER) * 1000, 0)
         FROM {table} g {level_filter} ORDER BY g.item_id, g.project_id"
    );
    let mut query = sqlx::query_as::<_, (i64, String, String, i64)>(&sql);
    if with_revisions {
        query = query.bind(kind.revision_kind());
    }
    let rows = query.fetch_all(pool).await?;
    for (project_id, name, payload, changed_ms) in rows {
        let body = serde_json::from_str::<Value>(&payload)
            .ok()
            .and_then(|v| v.get("body").and_then(|b| b.as_str()).map(str::to_string))
            .unwrap_or_default();
        out.entry(name.clone()).or_default().push(GlobalItem {
            project_id,
            name,
            body,
            changed_ms,
        });
    }
    Ok(out)
}

pub async fn load_from_pool(pool: &SqlitePool) -> Result<GlobalLevel, sqlx::Error> {
    Ok(GlobalLevel {
        rules: load_kind(pool, Kind::Rule).await?,
        skills: load_kind(pool, Kind::Skill).await?,
    })
}

/// `None` when global.db is absent or unreadable: callers then keep every project copy.
pub async fn load(path: &Path) -> Option<GlobalLevel> {
    if !path.is_file() {
        return None;
    }
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(ax_db::busy_timeout())
        .connect_with(ax_db::connect_options(path, false))
        .await
        .ok()?;
    let level = load_from_pool(&pool).await;
    pool.close().await;
    match level {
        Ok(level) => Some(level),
        Err(e) => {
            eprintln!("[ax policy] global.db not read ({}): {e}", path.display());
            None
        }
    }
}

#[cfg(not(test))]
pub fn default_path() -> Option<PathBuf> {
    ax_utils::paths::resolve_global_db_path(dirs::home_dir())
}

#[cfg(test)]
thread_local! {
    static TEST_GLOBAL_DB: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Tests never read the machine's global.db; they opt in per thread.
#[cfg(test)]
pub fn default_path() -> Option<PathBuf> {
    TEST_GLOBAL_DB.with(|p| p.borrow().clone())
}

#[cfg(test)]
pub(crate) fn set_test_global_db(path: Option<PathBuf>) {
    TEST_GLOBAL_DB.with(|p| *p.borrow_mut() = path);
}

pub async fn load_default() -> Option<GlobalLevel> {
    load(&default_path()?).await
}

/// Modification time of a policy file in ms since the epoch (0 when unknown).
pub fn file_changed_ms(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longer_body_wins_and_identical_never_replaces() {
        assert!(lower_copy_wins("abcd", 0, "abc", 9));
        assert!(!lower_copy_wins("ab", 9, "abc", 0));
        assert!(!lower_copy_wins(" abc\n", 9, "abc", 0));
    }

    #[test]
    fn windows_line_endings_are_not_more_extensive() {
        assert!(!lower_copy_wins("# a\r\n\r\nb\r\n", 9, "# a\n\nb\n", 0));
        assert!(!lower_copy_wins("# a\r\n\r\nb", 0, "# a\n\nb", 9));
        assert!(lower_copy_wins("# a\r\n\r\nbc", 0, "# a\n\nb", 9));
        assert_eq!(body_len("a\r\nb\r\n"), 3);
    }

    #[test]
    fn equal_length_goes_to_the_newest() {
        assert!(lower_copy_wins("abd", 10, "abc", 5));
        assert!(!lower_copy_wins("abd", 5, "abc", 10));
        assert!(!lower_copy_wins("abd", 5, "abc", 5));
    }

    #[test]
    fn leader_is_the_longest_then_newest() {
        let item = |pid, body: &str, ms| GlobalItem {
            project_id: pid,
            name: "auti".into(),
            body: body.into(),
            changed_ms: ms,
        };
        assert_eq!(
            leader_index(&[item(1, "ab", 0), item(2, "abcd", 0), item(3, "abc", 9)]),
            1
        );
        assert_eq!(leader_index(&[item(1, "abc", 1), item(2, "abd", 7)]), 1);
        assert_eq!(leader_index(&[item(1, "abc", 7), item(2, "abd", 1)]), 0);
    }
}
