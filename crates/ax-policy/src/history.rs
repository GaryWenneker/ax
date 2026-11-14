//! Append-only record of bundled rules, skills, and IDE files.
//!
//! Seed reads the latest action. `removed` means the file stays absent.
//! A later ax version still creates an id this record has never seen.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

pub const CATALOG_VERSION: i64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyEvent {
    pub kind: String,
    pub item_id: String,
    pub action: String,
    pub catalog_version: i64,
    pub at: i64,
}

pub fn project_history_path(project_root: &Path) -> PathBuf {
    project_root.join(".ax").join("policy-history.jsonl")
}

pub fn global_history_path(global_policy_root: &Path) -> PathBuf {
    global_policy_root.join("history.jsonl")
}

pub fn read_events(path: &Path) -> std::io::Result<Vec<PolicyEvent>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path)?;
    let mut events = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let event: PolicyEvent = serde_json::from_str(line).map_err(|err| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{}:{}: {err}", path.display(), line_no + 1),
            )
        })?;
        events.push(event);
    }
    Ok(events)
}

fn append_event(path: &Path, event: &PolicyEvent) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let line = serde_json::to_string(event)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string()))?;
    writeln!(file, "{line}")?;
    Ok(())
}

pub fn latest<'a>(events: &'a [PolicyEvent], kind: &str, item_id: &str) -> Option<&'a str> {
    events
        .iter()
        .rev()
        .find(|event| event.kind == kind && event.item_id == item_id)
        .map(|event| event.action.as_str())
}

fn has_kind(events: &[PolicyEvent], kind: &str) -> bool {
    events.iter().any(|event| event.kind == kind)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn push(path: &Path, kind: &str, item_id: &str, action: &str) -> std::io::Result<PolicyEvent> {
    let event = PolicyEvent {
        kind: kind.to_string(),
        item_id: item_id.to_string(),
        action: action.to_string(),
        catalog_version: CATALOG_VERSION,
        at: now_ms(),
    };
    append_event(path, &event)?;
    Ok(event)
}

/// On the first sight of `kind` in an existing install, record what is already there.
/// Absent bundled ids become `removed` so a later seed does not recreate them.
pub fn establish(
    path: &Path,
    kind: &str,
    items: &[(String, bool)],
    established: bool,
) -> std::io::Result<()> {
    let events = read_events(path)?;
    if has_kind(&events, kind) || !established {
        return Ok(());
    }
    for (item_id, exists) in items {
        push(
            path,
            kind,
            item_id,
            if *exists { "present" } else { "removed" },
        )?;
    }
    Ok(())
}

/// `true` when a missing file must stay missing.
/// A `present` or `restored` id that has disappeared is recorded as `removed`.
pub fn leave_absent(path: &Path, kind: &str, item_id: &str, force: bool) -> std::io::Result<bool> {
    if force {
        return Ok(false);
    }
    let events = read_events(path)?;
    match latest(&events, kind, item_id) {
        Some("removed") => Ok(true),
        Some("present") | Some("restored") => {
            push(path, kind, item_id, "removed")?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

pub fn is_intentional_absence(path: &Path, kind: &str, item_id: &str) -> std::io::Result<bool> {
    Ok(latest(&read_events(path)?, kind, item_id) == Some("removed"))
}

pub fn note_restored(path: &Path, kind: &str, item_id: &str) -> std::io::Result<()> {
    push(path, kind, item_id, "restored")?;
    Ok(())
}

pub fn note_removed(path: &Path, kind: &str, item_id: &str) -> std::io::Result<()> {
    let events = read_events(path)?;
    if latest(&events, kind, item_id) == Some("removed") {
        return Ok(());
    }
    push(path, kind, item_id, "removed")?;
    Ok(())
}

pub fn note_written(path: &Path, kind: &str, item_id: &str, force: bool) -> std::io::Result<()> {
    let events = read_events(path)?;
    let prev = latest(&events, kind, item_id);
    let action = if force && prev == Some("removed") {
        "restored"
    } else {
        "present"
    };
    if prev == Some(action) {
        return Ok(());
    }
    push(path, kind, item_id, action)?;
    Ok(())
}

/// Copy the JSONL ledger into `policy_history` and archive each new row as a `policy` memory.
pub async fn mirror_project(pool: &SqlitePool, project_root: &Path) -> Result<usize, String> {
    let path = project_history_path(project_root);
    mirror_file(pool, &path).await
}

pub async fn mirror_file(pool: &SqlitePool, path: &Path) -> Result<usize, String> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS policy_history (
            kind TEXT NOT NULL,
            item_id TEXT NOT NULL,
            action TEXT NOT NULL,
            catalog_version INTEGER NOT NULL,
            at INTEGER NOT NULL,
            PRIMARY KEY (kind, item_id, action, at)
        )",
    )
    .execute(pool)
    .await
    .map_err(|err| err.to_string())?;

    let events = read_events(path).map_err(|err| err.to_string())?;
    let mut mirrored = 0usize;
    for event in &events {
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO policy_history (kind, item_id, action, catalog_version, at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&event.kind)
        .bind(&event.item_id)
        .bind(&event.action)
        .bind(event.catalog_version)
        .bind(event.at)
        .execute(pool)
        .await
        .map_err(|err| err.to_string())?;
        if inserted.rows_affected() == 0 {
            continue;
        }
        mirrored += 1;
        let id = format!(
            "policy-{}-{}-{}",
            event.kind.replace('/', "-"),
            event.item_id.replace('/', "-"),
            event.at
        );
        let title = format!("{} {} {}", event.action, event.kind, event.item_id);
        let body = format!(
            "{} {} {} at catalog {}",
            event.action, event.kind, event.item_id, event.catalog_version
        );
        ax_memory::archive_policy_memory(pool, &id, &title, &body)
            .await
            .map_err(|err| err.to_string())?;
    }
    Ok(mirrored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn an_unknown_id_is_created_and_a_removed_id_stays_gone() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        push(&path, "rule", "old", "removed").unwrap();
        assert!(leave_absent(&path, "rule", "old", false).unwrap());
        assert!(!leave_absent(&path, "rule", "brand-new", false).unwrap());
        note_written(&path, "rule", "brand-new", false).unwrap();
        assert_eq!(
            latest(&read_events(&path).unwrap(), "rule", "brand-new"),
            Some("present")
        );
    }

    #[test]
    fn a_present_file_that_disappears_is_recorded_removed() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        push(&path, "skill", "startup", "present").unwrap();
        assert!(leave_absent(&path, "skill", "startup", false).unwrap());
        assert!(is_intentional_absence(&path, "skill", "startup").unwrap());
        assert!(!leave_absent(&path, "skill", "startup", true).unwrap());
    }

    #[test]
    fn the_first_snapshot_of_an_existing_install_records_absence() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        let items = vec![
            ("rules/keep.mdc".into(), true),
            ("rules/gone.mdc".into(), false),
        ];
        establish(&path, "rule", &items, true).unwrap();
        establish(&path, "rule", &items, true).unwrap();
        let events = read_events(&path).unwrap();
        assert_eq!(latest(&events, "rule", "rules/keep.mdc"), Some("present"));
        assert_eq!(latest(&events, "rule", "rules/gone.mdc"), Some("removed"));
        assert_eq!(events.len(), 2);
    }

    #[tokio::test]
    async fn seed_keeps_graph_rows_and_archives_the_removal() {
        let dir = tempdir().unwrap();
        let project = dir.path();
        let db_path = project.join(".ax").join("ax.db");
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let db = ax_db::Database::open(&db_path).await.unwrap();
        sqlx::query(
            "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
             VALUES ('sym-1', 'function', 'save_turn', 'turns::save_turn', 'src/turns.rs', 'rust', 1, 4, 0, 1, 'fn save_turn()', 1)",
        )
        .execute(db.pool())
        .await
        .unwrap();
        crate::seed::seed_default_policy(&project.join(".ax")).unwrap();
        let nodes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(nodes, 1);
        let startup = project
            .join(".agents")
            .join("skills")
            .join("startup")
            .join("SKILL.md");
        assert!(
            !startup.exists(),
            "an existing database must not grow deleted templates"
        );
        let mirrored = mirror_project(db.pool(), project).await.unwrap();
        assert!(mirrored > 0);
        let archived: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE kind = 'policy'")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert!(archived > 0);
        let nodes_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(nodes_after, 1);
    }
}
