//! Keep one copy of every rule and skill.
//!
//! A row at the global level of `~/.ax/global.db` applies to every project, so a project
//! copy with the same name is removed. When the project copy is the more extensive one it
//! is promoted into the global row first, as a new version.

use std::path::Path;

use ax_global_db::policy::{self as gpolicy, PolicyKind};
use ax_policy::global_level::{self, GlobalItem, Kind};
use serde::Serialize;
use sqlx::SqlitePool;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum Action {
    /// The project copy lost to (or equalled) the global copy.
    RemovedFromProject,
    /// The project copy won; it is now the global copy at this version.
    Promoted { version: i64 },
    /// A second global-level row with the same name, under another project id.
    RemovedGlobalDuplicate { project_id: i64 },
    /// A `ax global sync` mirror of a name that exists at the global level.
    RemovedMirror { project_id: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DedupAction {
    pub kind: &'static str,
    pub name: String,
    #[serde(flatten)]
    pub action: Action,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DedupReport {
    pub dry_run: bool,
    /// Why nothing was checked (no global.db).
    pub skipped: Option<String>,
    /// Why the run stopped early. Actions listed before it were applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub actions: Vec<DedupAction>,
}

impl DedupReport {
    /// One line per action, for CLI output and logs.
    pub fn lines(&self) -> Vec<String> {
        if let Some(reason) = &self.skipped {
            return vec![format!("skipped: {reason}")];
        }
        let mut lines: Vec<String> = self
            .actions
            .iter()
            .map(|a| {
                let what = match &a.action {
                    Action::RemovedFromProject => "removed from project".to_string(),
                    Action::Promoted { version } => format!("promoted to global (v{version})"),
                    Action::RemovedGlobalDuplicate { project_id } => {
                        format!("removed duplicate global row (project {project_id})")
                    }
                    Action::RemovedMirror { project_id } => {
                        format!("removed mirror (project {project_id})")
                    }
                };
                format!("{} {}: {what} — {}", a.kind, a.name, a.reason)
            })
            .collect();
        if let Some(error) = &self.error {
            lines.push(format!("error: {error}"));
        }
        lines
    }

    /// Actions at info, a stopped run at warn; a skipped run (no global.db) stays quiet.
    pub fn log(&self) {
        if self.skipped.is_some() {
            return;
        }
        for line in self.lines() {
            if self.error.is_some() && line.starts_with("error: ") {
                tracing::warn!("policy dedup: {line}");
            } else {
                tracing::info!("policy dedup: {line}");
            }
        }
    }
}

/// Clean one project (when given) and global.db itself.
///
/// Never deletes a row before its winner is stored, and records every removed body as a
/// revision. Files on disk are never touched.
pub async fn run(
    global_path: &Path,
    project: Option<(&SqlitePool, &Path)>,
    dry_run: bool,
) -> DedupReport {
    let mut report = DedupReport {
        dry_run,
        ..DedupReport::default()
    };
    if !global_path.is_file() {
        report.skipped = Some(format!("no global.db at {}", global_path.display()));
        return report;
    }
    let opened = if dry_run {
        ax_global_db::open_pool(global_path, false).await
    } else {
        ax_global_db::open_and_init(global_path).await
    };
    let global = match opened {
        Ok(pool) => pool,
        Err(e) => {
            report.error = Some(format!("global.db not opened: {e}"));
            return report;
        }
    };
    let current = if dry_run {
        global_level::schema_is_current(&global).await
    } else {
        Ok(true)
    };
    report.error = match current {
        Ok(false) => Some("global.db has an older schema; a real run upgrades it first".into()),
        Err(e) => Some(format!("stopped: {e}")),
        Ok(true) => clean(&global, project, dry_run, &mut report.actions)
            .await
            .err()
            .map(|e| format!("stopped: {e}")),
    };
    global.close().await;
    report
}

/// [`run`] against `~/.ax/global.db` (or `AX_GLOBAL_DB`).
pub async fn run_default(project: Option<(&SqlitePool, &Path)>, dry_run: bool) -> DedupReport {
    match ax_global_db::global_db_path() {
        Ok(path) => run(&path, project, dry_run).await,
        Err(e) => DedupReport {
            dry_run,
            skipped: None,
            error: Some(e.to_string()),
            actions: Vec::new(),
        },
    }
}

type Error = Box<dyn std::error::Error + Send + Sync>;

const KINDS: [Kind; 2] = [Kind::Rule, Kind::Skill];

fn gkind(kind: Kind) -> PolicyKind {
    match kind {
        Kind::Rule => PolicyKind::Rules,
        Kind::Skill => PolicyKind::Skills,
    }
}

async fn clean(
    global: &SqlitePool,
    project: Option<(&SqlitePool, &Path)>,
    dry_run: bool,
    actions: &mut Vec<DedupAction>,
) -> Result<(), Error> {
    let level = global_level::load_from_pool(global).await?;
    for kind in KINDS {
        for (name, items) in level.items(kind) {
            collapse_global(global, kind, name, items, dry_run, actions).await?;
        }
        remove_mirrors(global, kind, dry_run, actions).await?;
        if let Some((pool, root)) = project {
            clean_project(global, &level, kind, pool, root, dry_run, actions).await?;
        }
    }
    Ok(())
}

async fn collapse_global(
    global: &SqlitePool,
    kind: Kind,
    name: &str,
    items: &[GlobalItem],
    dry_run: bool,
    actions: &mut Vec<DedupAction>,
) -> Result<(), Error> {
    if items.len() < 2 {
        return Ok(());
    }
    let lead = global_level::leader_index(items);
    for (i, item) in items.iter().enumerate() {
        if i == lead {
            continue;
        }
        if !dry_run {
            gpolicy::delete_policy_item_from(
                global,
                item.project_id,
                gkind(kind),
                name,
                "dedup-removed",
            )
            .await?;
        }
        actions.push(DedupAction {
            kind: kind.revision_kind(),
            name: name.to_string(),
            action: Action::RemovedGlobalDuplicate {
                project_id: item.project_id,
            },
            reason: format!(
                "the copy under project {} is more extensive",
                items[lead].project_id
            ),
        });
    }
    Ok(())
}

async fn remove_mirrors(
    global: &SqlitePool,
    kind: Kind,
    dry_run: bool,
    actions: &mut Vec<DedupAction>,
) -> Result<(), Error> {
    for (project_id, name) in gpolicy::shadowed_mirrors(global, gkind(kind), !dry_run).await? {
        actions.push(DedupAction {
            kind: kind.revision_kind(),
            name,
            action: Action::RemovedMirror { project_id },
            reason: "a global copy exists".into(),
        });
    }
    Ok(())
}

struct ProjectRow {
    name: String,
    body: String,
    source_path: String,
    updated_at: i64,
}

fn project_table(kind: Kind) -> (&'static str, &'static str) {
    match kind {
        Kind::Rule => ("policy_rules", "id"),
        Kind::Skill => ("policy_skills", "name"),
    }
}

async fn project_rows(pool: &SqlitePool, kind: Kind) -> Result<Vec<ProjectRow>, Error> {
    let (table, id_col) = project_table(kind);
    let rows: Vec<(String, String, String, i64)> = sqlx::query_as(&format!(
        "SELECT {id_col}, body, source_path, updated_at FROM {table} ORDER BY {id_col}"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(name, body, source_path, updated_at)| ProjectRow {
            name,
            body,
            source_path,
            updated_at,
        })
        .collect())
}

/// The policy file's modification time when it still exists, else the row's `updated_at`.
fn row_changed_ms(root: &Path, row: &ProjectRow) -> i64 {
    let path = Path::new(&row.source_path);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    if path.is_file() {
        global_level::file_changed_ms(&path)
    } else {
        row.updated_at
    }
}

async fn clean_project(
    global: &SqlitePool,
    level: &global_level::GlobalLevel,
    kind: Kind,
    pool: &SqlitePool,
    root: &Path,
    dry_run: bool,
    actions: &mut Vec<DedupAction>,
) -> Result<(), Error> {
    for row in project_rows(pool, kind).await? {
        let Some(lead) = level.leader(kind, &row.name) else {
            continue;
        };
        let changed = row_changed_ms(root, &row);
        let action =
            if global_level::lower_copy_wins(&row.body, changed, &lead.body, lead.changed_ms) {
                let version = promote(global, lead, kind, pool, &row.name, dry_run).await?;
                Action::Promoted { version }
            } else {
                Action::RemovedFromProject
            };
        let reason = match &action {
            Action::Promoted { .. } => "the project copy is more extensive".to_string(),
            _ if global_level::same_text(&row.body, &lead.body) => {
                "identical to the global copy".to_string()
            }
            _ => "the global copy is at least as extensive".to_string(),
        };
        if !dry_run {
            remove_project_row(pool, kind, &row).await?;
        }
        actions.push(DedupAction {
            kind: kind.revision_kind(),
            name: row.name.clone(),
            action,
            reason,
        });
    }
    Ok(())
}

async fn promote(
    global: &SqlitePool,
    lead: &GlobalItem,
    kind: Kind,
    pool: &SqlitePool,
    name: &str,
    dry_run: bool,
) -> Result<i64, Error> {
    if dry_run {
        return Ok(gpolicy::next_version(global, gkind(kind), name).await?);
    }
    let (table, id_col) = project_table(kind);
    let row = sqlx::query(&format!("SELECT * FROM {table} WHERE {id_col} = ?"))
        .bind(name)
        .fetch_one(pool)
        .await?;
    let payload = ax_global_db::sync::row_to_policy_json(&row, id_col);
    match gpolicy::upsert_policy_item_from(
        global,
        lead.project_id,
        gkind(kind),
        name,
        &payload,
        "dedup-promote",
    )
    .await?
    {
        Some(version) => Ok(version),
        None => Ok(gpolicy::next_version(global, gkind(kind), name).await? - 1),
    }
}

async fn remove_project_row(pool: &SqlitePool, kind: Kind, row: &ProjectRow) -> Result<(), Error> {
    ax_policy::revisions::record_if_changed(
        pool,
        kind.revision_kind(),
        &row.name,
        &row.body,
        ax_policy::revisions::SOURCE_DEDUP,
    )
    .await?;
    let (table, id_col) = project_table(kind);
    sqlx::query(&format!("DELETE FROM {table} WHERE {id_col} = ?"))
        .bind(&row.name)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    struct Fixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
        project: SqlitePool,
        global_path: PathBuf,
        global: SqlitePool,
        machine: i64,
    }

    async fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        std::fs::create_dir_all(root.join(".ax")).unwrap();
        let db = ax_db::Database::open(&root.join(".ax").join("ax.db"))
            .await
            .unwrap();
        let project = db.pool().clone();
        let global_path = dir.path().join("global.db");
        let global = ax_global_db::open_and_init(&global_path).await.unwrap();
        let machine = gpolicy::ensure_project(&global, &dir.path().join("machine"))
            .await
            .unwrap();
        Fixture {
            _dir: dir,
            root,
            project,
            global_path,
            global,
            machine,
        }
    }

    fn gkind(kind: &str) -> PolicyKind {
        if kind == "rule" {
            PolicyKind::Rules
        } else {
            PolicyKind::Skills
        }
    }

    async fn project_row(f: &Fixture, kind: &str, name: &str, body: &str, updated_at: i64) {
        let sql = if kind == "rule" {
            "INSERT INTO policy_rules (id, level, body, source_path, content_hash, updated_at)
             VALUES (?, 'INFO', ?, 'rules/x.mdc', 'h', ?)"
        } else {
            "INSERT INTO policy_skills (name, description, body, source_path, content_hash, updated_at)
             VALUES (?, 'd', ?, 'skills/x/SKILL.md', 'h', ?)"
        };
        sqlx::query(sql)
            .bind(name)
            .bind(body)
            .bind(updated_at)
            .execute(&f.project)
            .await
            .unwrap();
    }

    async fn global_row(f: &Fixture, pid: i64, kind: &str, name: &str, body: &str) {
        let key = if kind == "rule" { "id" } else { "name" };
        gpolicy::upsert_policy_item(
            &f.global,
            pid,
            gkind(kind),
            name,
            &json!({ key: name, "body": body }),
        )
        .await
        .unwrap();
    }

    async fn project_body(f: &Fixture, kind: &str, name: &str) -> Option<String> {
        let sql = if kind == "rule" {
            "SELECT body FROM policy_rules WHERE id = ?"
        } else {
            "SELECT body FROM policy_skills WHERE name = ?"
        };
        sqlx::query_scalar(sql)
            .bind(name)
            .fetch_optional(&f.project)
            .await
            .unwrap()
    }

    async fn global_bodies(f: &Fixture, kind: &str, name: &str) -> Vec<(i64, String)> {
        let table = if kind == "rule" {
            "global_policy_rules"
        } else {
            "global_policy_skills"
        };
        let rows: Vec<(i64, String)> = sqlx::query_as(&format!(
            "SELECT project_id, payload FROM {table} WHERE item_id = ? ORDER BY project_id"
        ))
        .bind(name)
        .fetch_all(&f.global)
        .await
        .unwrap();
        rows.into_iter()
            .map(|(pid, p)| {
                let v: serde_json::Value = serde_json::from_str(&p).unwrap();
                (pid, v["body"].as_str().unwrap_or_default().to_string())
            })
            .collect()
    }

    async fn project_revision_sources(
        f: &Fixture,
        kind: &str,
        name: &str,
    ) -> Vec<(String, String)> {
        sqlx::query_as(
            "SELECT source, body FROM policy_revisions WHERE kind = ? AND item_id = ? ORDER BY id",
        )
        .bind(kind)
        .bind(name)
        .fetch_all(&f.project)
        .await
        .unwrap()
    }

    async fn run_on(f: &Fixture, dry_run: bool) -> DedupReport {
        run(&f.global_path, Some((&f.project, &f.root)), dry_run).await
    }

    fn body(n: usize) -> String {
        "x".repeat(n)
    }

    #[tokio::test]
    async fn identical_project_copy_is_removed() {
        for kind in ["skill", "rule"] {
            let f = fixture().await;
            global_row(&f, f.machine, kind, "noti", "same body").await;
            project_row(&f, kind, "noti", "same body", 1).await;

            let report = run_on(&f, false).await;

            assert_eq!(report.skipped, None);
            assert_eq!(report.actions.len(), 1, "{kind}: {report:?}");
            assert_eq!(report.actions[0].action, Action::RemovedFromProject);
            assert_eq!(project_body(&f, kind, "noti").await, None, "{kind}");
            assert_eq!(
                project_revision_sources(&f, kind, "noti").await,
                vec![("dedup".to_string(), "same body".to_string())],
                "{kind}"
            );
            assert_eq!(
                global_bodies(&f, kind, "noti").await,
                vec![(f.machine, "same body".to_string())]
            );
            assert_eq!(
                gpolicy::list_revisions(&f.global, gkind(kind), "noti")
                    .await
                    .unwrap()
                    .len(),
                1
            );
        }
    }

    #[tokio::test]
    async fn dry_run_on_a_pre_versioning_global_db_says_so_and_the_real_run_upgrades() {
        let f = fixture().await;
        let old = f.root.parent().unwrap().join("old-global.db");
        let pool = ax_global_db::open_pool(&old, true).await.unwrap();
        sqlx::query(
            "CREATE TABLE global_policy_skills (project_id INTEGER NOT NULL, item_id TEXT NOT NULL,
             payload TEXT NOT NULL, synced_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY (project_id, item_id))",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO global_policy_skills (project_id, item_id, payload) VALUES (1, 'noti', ?)",
        )
        .bind(json!({ "name": "noti", "body": "short" }).to_string())
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
        project_row(&f, "skill", "noti", &body(50), 0).await;

        let dry = run(&old, Some((&f.project, &f.root)), true).await;
        let real = run(&old, Some((&f.project, &f.root)), false).await;

        assert!(
            dry.error
                .as_deref()
                .is_some_and(|e| e.contains("older schema")),
            "{dry:?}"
        );
        assert!(dry.actions.is_empty());
        assert_eq!(real.error, None, "{real:?}");
        assert_eq!(real.actions[0].action, Action::Promoted { version: 2 });
    }

    #[test]
    fn report_lines_cover_skip_actions_and_error() {
        let skipped = DedupReport {
            skipped: Some("no global.db at /x".into()),
            ..DedupReport::default()
        };
        assert_eq!(
            skipped.lines(),
            vec!["skipped: no global.db at /x".to_string()]
        );
        let stopped = DedupReport {
            error: Some("stopped: boom".into()),
            actions: vec![DedupAction {
                kind: "skill",
                name: "noti".into(),
                action: Action::Promoted { version: 3 },
                reason: "the project copy is more extensive".into(),
            }],
            ..DedupReport::default()
        };
        assert_eq!(
            stopped.lines(),
            vec![
                "skill noti: promoted to global (v3) — the project copy is more extensive"
                    .to_string(),
                "error: stopped: boom".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn a_corrupt_global_db_is_an_error_not_a_skip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("global.db");
        std::fs::write(&path, "this is not a sqlite database, just text").unwrap();

        let report = run(&path, None, false).await;

        assert_eq!(report.skipped, None);
        assert!(report.error.is_some(), "{report:?}");
        assert!(report.actions.is_empty());
    }

    #[tokio::test]
    async fn an_unreadable_row_stops_the_run_with_an_error() {
        let f = fixture().await;
        global_row(&f, f.machine, "skill", "noti", "same").await;
        global_row(&f, f.machine, "skill", "auti", "same").await;
        project_row(&f, "skill", "auti", "same", 0).await;
        sqlx::query(
            "INSERT INTO policy_skills (name, description, body, source_path, content_hash, updated_at)
             VALUES ('noti', 'd', CAST('same' AS BLOB), 's', 'h', 0)",
        )
        .execute(&f.project)
        .await
        .unwrap();

        let report = run_on(&f, false).await;

        assert_eq!(report.skipped, None);
        assert!(
            report
                .error
                .as_deref()
                .is_some_and(|e| e.contains("decoding")),
            "{report:?}"
        );
        assert!(report.actions.is_empty());
        assert_eq!(
            project_body(&f, "skill", "auti").await.as_deref(),
            Some("same")
        );
    }

    #[tokio::test]
    async fn crlf_copy_counts_as_identical() {
        let f = fixture().await;
        global_row(&f, f.machine, "skill", "auti", "# Auti\n\nKort.\n").await;
        project_row(&f, "skill", "auti", "# Auti\r\n\r\nKort.\r\n", i64::MAX).await;

        let report = run_on(&f, false).await;

        assert_eq!(report.actions.len(), 1, "{report:?}");
        assert_eq!(report.actions[0].action, Action::RemovedFromProject);
        assert_eq!(report.actions[0].reason, "identical to the global copy");
        assert_eq!(
            global_bodies(&f, "skill", "auti").await,
            vec![(f.machine, "# Auti\n\nKort.\n".to_string())]
        );
    }

    #[tokio::test]
    async fn shorter_project_copy_is_removed_global_kept() {
        for kind in ["skill", "rule"] {
            let f = fixture().await;
            global_row(&f, f.machine, kind, "noti", &body(200)).await;
            project_row(&f, kind, "noti", &body(100), i64::MAX).await;

            let report = run_on(&f, false).await;

            assert_eq!(
                report.actions[0].action,
                Action::RemovedFromProject,
                "{kind}"
            );
            assert_eq!(project_body(&f, kind, "noti").await, None, "{kind}");
            assert_eq!(
                global_bodies(&f, kind, "noti").await,
                vec![(f.machine, body(200))],
                "{kind}"
            );
        }
    }

    #[tokio::test]
    async fn longer_project_copy_is_promoted() {
        for kind in ["skill", "rule"] {
            let f = fixture().await;
            global_row(&f, f.machine, kind, "noti", &body(200)).await;
            project_row(&f, kind, "noti", &body(300), 0).await;

            let report = run_on(&f, false).await;

            assert_eq!(
                report.actions[0].action,
                Action::Promoted { version: 2 },
                "{kind}: {report:?}"
            );
            assert_eq!(
                global_bodies(&f, kind, "noti").await,
                vec![(f.machine, body(300))],
                "{kind}"
            );
            let revs = gpolicy::list_revisions(&f.global, gkind(kind), "noti")
                .await
                .unwrap();
            assert_eq!(revs[0].version, 2);
            assert_eq!(revs[0].source, "dedup-promote");
            assert_eq!(project_body(&f, kind, "noti").await, None, "{kind}");
            assert_eq!(
                project_revision_sources(&f, kind, "noti").await,
                vec![("dedup".to_string(), body(300))],
                "{kind}"
            );
        }
    }

    #[tokio::test]
    async fn duplicate_global_rows_collapse_to_the_winner() {
        for kind in ["skill", "rule"] {
            let f = fixture().await;
            let other = gpolicy::ensure_project(&f.global, &f.root).await.unwrap();
            global_row(&f, f.machine, kind, "auti", &body(50)).await;
            global_row(&f, other, kind, "auti", &body(80)).await;

            let report = run(&f.global_path, None, false).await;

            assert_eq!(
                report
                    .actions
                    .iter()
                    .map(|a| a.action.clone())
                    .collect::<Vec<_>>(),
                vec![Action::RemovedGlobalDuplicate {
                    project_id: f.machine
                }],
                "{kind}"
            );
            assert_eq!(
                global_bodies(&f, kind, "auti").await,
                vec![(other, body(80))],
                "{kind}"
            );
            let revs = gpolicy::list_revisions(&f.global, gkind(kind), "auti")
                .await
                .unwrap();
            assert_eq!(revs[0].source, "dedup-removed");
            assert_eq!(revs[0].payload["body"], body(50));
        }
    }

    #[tokio::test]
    async fn mirrors_of_global_names_are_removed() {
        let f = fixture().await;
        let other = gpolicy::ensure_project(&f.global, &f.root).await.unwrap();
        global_row(&f, f.machine, "skill", "noti", "global").await;
        sqlx::query(
            "INSERT INTO global_policy_skills (project_id, item_id, payload, level) VALUES (?, 'noti', '{}', 'mirror')",
        )
        .bind(other)
        .execute(&f.global)
        .await
        .unwrap();

        let report = run(&f.global_path, None, false).await;

        assert_eq!(
            report.actions[0].action,
            Action::RemovedMirror { project_id: other }
        );
        assert_eq!(
            global_bodies(&f, "skill", "noti").await,
            vec![(f.machine, "global".to_string())]
        );
    }

    #[tokio::test]
    async fn dedup_never_touches_disk_files() {
        let f = fixture().await;
        let file = f.root.join(".agents/skills/noti/SKILL.md");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "---\nname: noti\n---\nsame body\n").unwrap();
        let before = std::fs::read(&file).unwrap();
        global_row(&f, f.machine, "skill", "noti", "same body").await;
        project_row(&f, "skill", "noti", "same body", 1).await;

        run_on(&f, false).await;

        assert_eq!(std::fs::read(&file).unwrap(), before);
    }

    #[tokio::test]
    async fn missing_global_db_is_a_noop() {
        let f = fixture().await;
        project_row(&f, "skill", "noti", "body", 1).await;
        let missing = f.root.join("nope.db");

        let report = run(&missing, Some((&f.project, &f.root)), false).await;

        assert!(
            report
                .skipped
                .as_deref()
                .unwrap_or_default()
                .contains("no global.db"),
            "{report:?}"
        );
        assert!(report.actions.is_empty());
        assert_eq!(
            project_body(&f, "skill", "noti").await.as_deref(),
            Some("body")
        );
        assert!(!missing.exists());
    }

    #[tokio::test]
    async fn second_run_changes_nothing() {
        let f = fixture().await;
        global_row(&f, f.machine, "skill", "noti", &body(10)).await;
        project_row(&f, "skill", "noti", &body(20), 0).await;
        project_row(&f, "rule", "utf8", &body(5), 0).await;
        global_row(&f, f.machine, "rule", "utf8", &body(5)).await;

        assert_eq!(run_on(&f, false).await.actions.len(), 2);
        let second = run_on(&f, false).await;

        assert!(second.actions.is_empty(), "{second:?}");
    }

    #[tokio::test]
    async fn tie_goes_to_the_newest() {
        let f = fixture().await;
        global_row(&f, f.machine, "skill", "newer", "abc").await;
        global_row(&f, f.machine, "skill", "older", "abc").await;
        project_row(&f, "skill", "newer", "abd", i64::MAX / 2).await;
        project_row(&f, "skill", "older", "abd", 0).await;

        let report = run_on(&f, false).await;

        let by_name = |n: &str| {
            report
                .actions
                .iter()
                .find(|a| a.name == n)
                .unwrap()
                .action
                .clone()
        };
        assert_eq!(by_name("newer"), Action::Promoted { version: 2 });
        assert_eq!(by_name("older"), Action::RemovedFromProject);
        assert_eq!(
            global_bodies(&f, "skill", "newer").await,
            vec![(f.machine, "abd".to_string())]
        );
        assert_eq!(
            global_bodies(&f, "skill", "older").await,
            vec![(f.machine, "abc".to_string())]
        );
    }

    #[tokio::test]
    async fn dry_run_changes_nothing() {
        let f = fixture().await;
        let other = gpolicy::ensure_project(&f.global, &f.root).await.unwrap();
        global_row(&f, f.machine, "skill", "noti", &body(10)).await;
        project_row(&f, "skill", "noti", &body(20), 0).await;
        global_row(&f, f.machine, "rule", "utf8", &body(9)).await;
        project_row(&f, "rule", "utf8", &body(3), 0).await;
        global_row(&f, f.machine, "skill", "auti", &body(4)).await;
        global_row(&f, other, "skill", "auti", &body(6)).await;
        sqlx::query(
            "INSERT INTO global_policy_skills (project_id, item_id, payload, level) VALUES (?, 'noti', '{}', 'mirror')",
        )
        .bind(other)
        .execute(&f.global)
        .await
        .unwrap();
        let snapshot = |f: &Fixture| {
            let (g, p) = (f.global.clone(), f.project.clone());
            async move {
                let gs: Vec<(i64, String, String)> = sqlx::query_as(
                    "SELECT project_id, item_id, payload FROM global_policy_skills ORDER BY 1, 2",
                )
                .fetch_all(&g)
                .await
                .unwrap();
                let gr: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM global_policy_revisions")
                    .fetch_one(&g)
                    .await
                    .unwrap();
                let ps: Vec<(String,)> = sqlx::query_as(
                    "SELECT name FROM policy_skills UNION ALL SELECT id FROM policy_rules",
                )
                .fetch_all(&p)
                .await
                .unwrap();
                let pr: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM policy_revisions")
                    .fetch_one(&p)
                    .await
                    .unwrap();
                (gs, gr.0, ps, pr.0)
            }
        };
        let before = snapshot(&f).await;

        let dry = run_on(&f, true).await;

        assert_eq!(snapshot(&f).await, before);
        assert!(dry.dry_run);
        let real = run_on(&f, false).await;
        assert_eq!(dry.actions, real.actions);
        assert_eq!(real.actions.len(), 4, "{real:?}");
    }
}
