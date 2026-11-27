//! Policy review queue — pending rules/skills awaiting approve/reject.

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use ax_utils::errors::AxError;

use crate::index::index_policy;
use crate::parse::{format_validation, parse_rule_file, parse_skill_file, validation_error_at};
use crate::paths::{
    ax_dir_from_project, pending_dir, pending_rules_dir, pending_skills_dir, rule_file, rules_dir,
    skill_file, skills_dir, SKILL_FILENAME,
};
use crate::store::PolicyStore;
use crate::types::PolicyItemStatus;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingItem {
    pub kind: String,
    pub id: String,
    pub path: String,
    pub status: String,
    pub preview: String,
    pub level_or_description: String,
}

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewActionResult {
    pub ok: bool,
    pub kind: String,
    pub id: String,
    pub action: String,
}

/// A pending file that could not be read or parsed. It stays in the queue
/// folder until someone fixes or rejects it; the other items are unaffected.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvalidPending {
    pub kind: String,
    /// The id the file name implies (rule file stem or skill folder name).
    pub id: String,
    pub path: String,
    /// What is wrong, with every failing field.
    pub error: String,
}

/// The review queue: valid items plus the files that failed to parse.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingQueue {
    pub items: Vec<PendingItem>,
    pub invalid: Vec<InvalidPending>,
}

/// Valid pending items. Files that fail to parse are left out; use
/// [`list_pending_report`] to see them.
pub fn list_pending(project_root: &Path) -> Result<Vec<PendingItem>, AxError> {
    Ok(list_pending_report(project_root)?.items)
}

/// Every pending item, with unreadable or unparsable files reported one by
/// one instead of failing the whole queue.
pub fn list_pending_report(project_root: &Path) -> Result<PendingQueue, AxError> {
    let ax_dir = ax_dir_from_project(project_root);
    let mut queue = PendingQueue::default();

    let rules_path = pending_rules_dir(&ax_dir);
    if rules_path.is_dir() {
        for entry in std::fs::read_dir(&rules_path).map_err(|e| AxError::Other(e.to_string()))? {
            let entry = entry.map_err(|e| AxError::Other(e.to_string()))?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("mdc") {
                continue;
            }
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let parsed = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|raw| {
                    parse_rule_file(&path, &raw)
                        .map(|doc| (raw, doc))
                        .map_err(|e| format_validation(&e))
                });
            match parsed {
                Ok((raw, doc)) => queue.items.push(PendingItem {
                    kind: "rule".into(),
                    id: doc.frontmatter.id.clone(),
                    path: path.display().to_string(),
                    status: doc.frontmatter.status.clone(),
                    preview: raw.chars().take(400).collect(),
                    level_or_description: doc.frontmatter.level,
                }),
                Err(error) => queue.invalid.push(InvalidPending {
                    kind: "rule".into(),
                    id: stem,
                    path: path.display().to_string(),
                    error,
                }),
            }
        }
    }

    let skills_path = pending_skills_dir(&ax_dir);
    if skills_path.is_dir() {
        for entry in std::fs::read_dir(&skills_path).map_err(|e| AxError::Other(e.to_string()))? {
            let entry = entry.map_err(|e| AxError::Other(e.to_string()))?;
            if !entry.path().is_dir() {
                continue;
            }
            let skill_path = entry.path().join(SKILL_FILENAME);
            if !skill_path.is_file() {
                continue;
            }
            let parsed = std::fs::read_to_string(&skill_path)
                .map_err(|e| e.to_string())
                .and_then(|raw| {
                    parse_skill_file(&skill_path, &raw)
                        .map(|doc| (raw, doc))
                        .map_err(|e| format_validation(&e))
                });
            match parsed {
                Ok((raw, doc)) => queue.items.push(PendingItem {
                    kind: "skill".into(),
                    id: doc.frontmatter.name.clone(),
                    path: skill_path.display().to_string(),
                    status: doc.frontmatter.status.clone(),
                    preview: raw.chars().take(400).collect(),
                    level_or_description: doc.frontmatter.description,
                }),
                Err(error) => queue.invalid.push(InvalidPending {
                    kind: "skill".into(),
                    id: entry.file_name().to_string_lossy().to_string(),
                    path: skill_path.display().to_string(),
                    error,
                }),
            }
        }
    }

    queue
        .items
        .sort_by(|a, b| a.kind.cmp(&b.kind).then(a.id.cmp(&b.id)));
    queue
        .invalid
        .sort_by(|a, b| a.kind.cmp(&b.kind).then(a.id.cmp(&b.id)));
    Ok(queue)
}

pub fn show_pending(project_root: &Path, id: &str) -> Result<PendingItem, AxError> {
    let queue = list_pending_report(project_root)?;
    if let Some(item) = queue.items.into_iter().find(|i| i.id == id) {
        return Ok(item);
    }
    if let Some(bad) = queue.invalid.into_iter().find(|i| i.id == id) {
        return Err(AxError::Other(format!(
            "pending {} {id} cannot be read: {}: {}",
            bad.kind, bad.path, bad.error
        )));
    }
    Err(AxError::Other(format!("pending item not found: {id}")))
}

pub async fn approve_pending(
    pool: &SqlitePool,
    project_root: &Path,
    id: &str,
) -> Result<ReviewActionResult, AxError> {
    approve_pending_kind(pool, project_root, id, None).await
}

pub async fn approve_pending_kind(
    pool: &SqlitePool,
    project_root: &Path,
    id: &str,
    kind: Option<&str>,
) -> Result<ReviewActionResult, AxError> {
    validate_review_identity(id, kind)?;
    let ax_dir = ax_dir_from_project(project_root);
    let store = PolicyStore::new(pool.clone(), project_root.to_path_buf());

    let rule_path = rule_file(&pending_rules_dir(&ax_dir), id);
    if kind != Some("skill") && rule_path.is_file() {
        let raw = std::fs::read_to_string(&rule_path).map_err(|e| AxError::Other(e.to_string()))?;
        let mut doc =
            parse_rule_file(&rule_path, &raw).map_err(|e| validation_error_at(&rule_path, &e))?;
        doc.frontmatter.status = PolicyItemStatus::Approved.as_str().into();
        store
            .save_rule(doc.frontmatter, doc.body)
            .await
            .map_err(|e| validation_error_at(&rule_path, &e))?;
        crate::provenance::remove_pending_origin(&rule_path).await?;
        tokio::fs::remove_file(&rule_path)
            .await
            .map_err(|e| AxError::Other(e.to_string()))?;
        index_policy(pool, project_root, false).await?;
        return Ok(ReviewActionResult {
            ok: true,
            kind: "rule".into(),
            id: id.into(),
            action: "approve".into(),
        });
    }

    let skill_path = pending_skills_dir(&ax_dir).join(id).join(SKILL_FILENAME);
    if kind != Some("rule") && skill_path.is_file() {
        let raw =
            std::fs::read_to_string(&skill_path).map_err(|e| AxError::Other(e.to_string()))?;
        let mut doc = parse_skill_file(&skill_path, &raw)
            .map_err(|e| validation_error_at(&skill_path, &e))?;
        doc.frontmatter.status = PolicyItemStatus::Approved.as_str().into();
        store
            .save_skill(doc.frontmatter, doc.body)
            .await
            .map_err(|e| validation_error_at(&skill_path, &e))?;
        let dir = skill_path.parent().map(Path::to_path_buf);
        crate::provenance::remove_pending_origin(&skill_path).await?;
        tokio::fs::remove_file(&skill_path)
            .await
            .map_err(|e| AxError::Other(e.to_string()))?;
        if let Some(d) = dir {
            let _ = std::fs::remove_dir(&d);
        }
        index_policy(pool, project_root, false).await?;
        return Ok(ReviewActionResult {
            ok: true,
            kind: "skill".into(),
            id: id.into(),
            action: "approve".into(),
        });
    }

    Err(AxError::Other(format!("pending item not found: {id}")))
}

pub async fn reject_pending(
    pool: &SqlitePool,
    project_root: &Path,
    id: &str,
) -> Result<ReviewActionResult, AxError> {
    reject_pending_kind(pool, project_root, id, None).await
}

pub async fn reject_pending_kind(
    pool: &SqlitePool,
    project_root: &Path,
    id: &str,
    kind: Option<&str>,
) -> Result<ReviewActionResult, AxError> {
    validate_review_identity(id, kind)?;
    let ax_dir = ax_dir_from_project(project_root);

    let rule_path = rule_file(&pending_rules_dir(&ax_dir), id);
    if kind != Some("skill") && rule_path.is_file() {
        crate::provenance::remove_pending_origin(&rule_path).await?;
        std::fs::remove_file(&rule_path).map_err(|e| AxError::Other(e.to_string()))?;
        let _ = pool; // keep signature consistent; no DB row for pending files
        return Ok(ReviewActionResult {
            ok: true,
            kind: "rule".into(),
            id: id.into(),
            action: "reject".into(),
        });
    }

    let skill_dir = pending_skills_dir(&ax_dir).join(id);
    let skill_path = skill_dir.join(SKILL_FILENAME);
    if kind != Some("rule") && skill_path.is_file() {
        crate::provenance::remove_pending_origin(&skill_path).await?;
        std::fs::remove_file(&skill_path).map_err(|e| AxError::Other(e.to_string()))?;
        let _ = std::fs::remove_dir(&skill_dir);
        return Ok(ReviewActionResult {
            ok: true,
            kind: "skill".into(),
            id: id.into(),
            action: "reject".into(),
        });
    }

    Err(AxError::Other(format!("pending item not found: {id}")))
}

fn validate_review_identity(id: &str, kind: Option<&str>) -> Result<(), AxError> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || id.contains(['/', '\\'])
        || kind.is_some_and(|k| k != "rule" && k != "skill")
    {
        return Err(AxError::Other("invalid review identity".into()));
    }
    Ok(())
}

/// Diff helper: local active body vs pending body for UI.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingDiff {
    pub kind: String,
    pub id: String,
    pub pending_body: String,
    pub local_body: Option<String>,
    pub pending_raw: String,
    pub local_raw: Option<String>,
}

pub fn pending_diff(project_root: &Path, id: &str) -> Result<PendingDiff, AxError> {
    let item = show_pending(project_root, id)?;
    let pending_raw =
        std::fs::read_to_string(&item.path).map_err(|e| AxError::Other(e.to_string()))?;
    let ax_dir = ax_dir_from_project(project_root);

    if item.kind == "rule" {
        let local_path = rule_file(&rules_dir(&ax_dir), id);
        let local_raw = if local_path.is_file() {
            Some(std::fs::read_to_string(&local_path).map_err(|e| AxError::Other(e.to_string()))?)
        } else {
            None
        };
        let pending_doc = parse_rule_file(Path::new(&item.path), &pending_raw)
            .map_err(|e| validation_error_at(Path::new(&item.path), &e))?;
        let local_body = local_raw
            .as_ref()
            .and_then(|r| parse_rule_file(&local_path, r).ok().map(|d| d.body));
        return Ok(PendingDiff {
            kind: "rule".into(),
            id: id.into(),
            pending_body: pending_doc.body,
            local_body,
            pending_raw,
            local_raw,
        });
    }

    let local_path = skill_file(&skills_dir(&ax_dir), id);
    let local_raw = if local_path.is_file() {
        Some(std::fs::read_to_string(&local_path).map_err(|e| AxError::Other(e.to_string()))?)
    } else {
        None
    };
    let pending_doc = parse_skill_file(Path::new(&item.path), &pending_raw)
        .map_err(|e| validation_error_at(Path::new(&item.path), &e))?;
    let local_body = local_raw
        .as_ref()
        .and_then(|r| parse_skill_file(&local_path, r).ok().map(|d| d.body));
    Ok(PendingDiff {
        kind: "skill".into(),
        id: id.into(),
        pending_body: pending_doc.body,
        local_body,
        pending_raw,
        local_raw,
    })
}

/// Ensure pending dirs exist (for UI / import).
pub fn ensure_pending_dirs(project_root: &Path) -> Result<PathBuf, AxError> {
    let ax_dir = ax_dir_from_project(project_root);
    std::fs::create_dir_all(pending_rules_dir(&ax_dir))
        .map_err(|e| AxError::Other(e.to_string()))?;
    std::fs::create_dir_all(pending_skills_dir(&ax_dir))
        .map_err(|e| AxError::Other(e.to_string()))?;
    Ok(pending_dir(&ax_dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue_with_one_bad_file_each() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let ax_dir = ax_dir_from_project(root.path());
        let rules = pending_rules_dir(&ax_dir);
        std::fs::create_dir_all(&rules).unwrap();
        std::fs::write(
            rules.join("good.mdc"),
            "---\nid: good\nlevel: INFO\nalwaysApply: true\nstatus: pending\n---\nGood rule\n",
        )
        .unwrap();
        std::fs::write(
            rules.join("bad.mdc"),
            "---\nid: bad\nalwaysApply: true\nstatus: pending\n---\nNo level\n",
        )
        .unwrap();
        let skills = pending_skills_dir(&ax_dir);
        std::fs::create_dir_all(skills.join("good-skill")).unwrap();
        std::fs::create_dir_all(skills.join("bad-skill")).unwrap();
        std::fs::write(
            skills.join("good-skill").join(SKILL_FILENAME),
            "---\nname: good-skill\ndescription: fine\nstatus: pending\n---\nGood skill\n",
        )
        .unwrap();
        std::fs::write(
            skills.join("bad-skill").join(SKILL_FILENAME),
            "---\nname: bad-skill\nstatus: pending\n---\nNo description\n",
        )
        .unwrap();
        root
    }

    #[test]
    fn one_bad_pending_file_does_not_hide_the_others() {
        let root = queue_with_one_bad_file_each();
        let items = list_pending(root.path()).unwrap();
        let ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["good", "good-skill"]);
    }

    #[test]
    fn a_good_item_can_be_diffed_next_to_a_bad_one() {
        let root = queue_with_one_bad_file_each();
        let diff = pending_diff(root.path(), "good").unwrap();
        assert_eq!(diff.pending_body.trim(), "Good rule");
        let diff = pending_diff(root.path(), "good-skill").unwrap();
        assert_eq!(diff.pending_body.trim(), "Good skill");
    }

    #[test]
    fn showing_a_bad_item_names_its_file_and_field() {
        let root = queue_with_one_bad_file_each();
        let err = show_pending(root.path(), "bad").unwrap_err().to_string();
        assert!(err.contains("bad.mdc"), "{err}");
        assert!(err.contains("level: required"), "{err}");
        let err = pending_diff(root.path(), "bad-skill")
            .unwrap_err()
            .to_string();
        assert!(err.contains("bad-skill"), "{err}");
        assert!(err.contains("description: required"), "{err}");
    }

    #[tokio::test]
    async fn approving_a_bad_item_names_its_file_and_field() {
        let root = queue_with_one_bad_file_each();
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        ax_db::schema::apply_initial_schema(&pool).await.unwrap();
        ax_db::migrations::run_migrations(&pool, 0).await.unwrap();
        let err = approve_pending(&pool, root.path(), "bad")
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("bad.mdc"), "{err}");
        assert!(err.contains("level: required"), "{err}");
    }
}
