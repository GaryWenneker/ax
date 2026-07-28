//! Policy review queue — pending rules/skills awaiting approve/reject.

use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use ax_utils::errors::AxError;

use crate::index::index_policy;
use crate::parse::{parse_rule_file, parse_skill_file};
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

pub fn list_pending(project_root: &Path) -> Result<Vec<PendingItem>, AxError> {
    let ax_dir = ax_dir_from_project(project_root);
    let mut items = Vec::new();

    let rules_path = pending_rules_dir(&ax_dir);
    if rules_path.is_dir() {
        for entry in std::fs::read_dir(&rules_path).map_err(|e| AxError::Other(e.to_string()))? {
            let entry = entry.map_err(|e| AxError::Other(e.to_string()))?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("mdc") {
                continue;
            }
            let raw = std::fs::read_to_string(&path).map_err(|e| AxError::Other(e.to_string()))?;
            let doc = parse_rule_file(&path, &raw).map_err(|e| AxError::Other(e.error))?;
            items.push(PendingItem {
                kind: "rule".into(),
                id: doc.frontmatter.id.clone(),
                path: path.display().to_string(),
                status: doc.frontmatter.status.clone(),
                preview: raw.chars().take(400).collect(),
                level_or_description: doc.frontmatter.level,
            });
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
            let raw =
                std::fs::read_to_string(&skill_path).map_err(|e| AxError::Other(e.to_string()))?;
            let doc = parse_skill_file(&skill_path, &raw).map_err(|e| AxError::Other(e.error))?;
            items.push(PendingItem {
                kind: "skill".into(),
                id: doc.frontmatter.name.clone(),
                path: skill_path.display().to_string(),
                status: doc.frontmatter.status.clone(),
                preview: raw.chars().take(400).collect(),
                level_or_description: doc.frontmatter.description,
            });
        }
    }

    items.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.id.cmp(&b.id)));
    Ok(items)
}

pub fn show_pending(project_root: &Path, id: &str) -> Result<PendingItem, AxError> {
    list_pending(project_root)?
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| AxError::Other(format!("pending item not found: {id}")))
}

pub async fn approve_pending(
    pool: &SqlitePool,
    project_root: &Path,
    id: &str,
) -> Result<ReviewActionResult, AxError> {
    let ax_dir = ax_dir_from_project(project_root);
    let store = PolicyStore::new(pool.clone(), project_root.to_path_buf());

    let rule_path = rule_file(&pending_rules_dir(&ax_dir), id);
    if rule_path.is_file() {
        let raw = std::fs::read_to_string(&rule_path).map_err(|e| AxError::Other(e.to_string()))?;
        let mut doc = parse_rule_file(&rule_path, &raw).map_err(|e| AxError::Other(e.error))?;
        doc.frontmatter.status = PolicyItemStatus::Approved.as_str().into();
        store
            .save_rule(doc.frontmatter, doc.body)
            .await
            .map_err(|e| AxError::Other(e.error))?;
        let _ = std::fs::remove_file(&rule_path);
        index_policy(pool, project_root, false).await?;
        return Ok(ReviewActionResult {
            ok: true,
            kind: "rule".into(),
            id: id.into(),
            action: "approve".into(),
        });
    }

    let skill_path = pending_skills_dir(&ax_dir).join(id).join(SKILL_FILENAME);
    if skill_path.is_file() {
        let raw =
            std::fs::read_to_string(&skill_path).map_err(|e| AxError::Other(e.to_string()))?;
        let mut doc = parse_skill_file(&skill_path, &raw).map_err(|e| AxError::Other(e.error))?;
        doc.frontmatter.status = PolicyItemStatus::Approved.as_str().into();
        store
            .save_skill(doc.frontmatter, doc.body)
            .await
            .map_err(|e| AxError::Other(e.error))?;
        let dir = skill_path.parent().map(Path::to_path_buf);
        let _ = std::fs::remove_file(&skill_path);
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
    let ax_dir = ax_dir_from_project(project_root);

    let rule_path = rule_file(&pending_rules_dir(&ax_dir), id);
    if rule_path.is_file() {
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
    if skill_path.is_file() {
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
    let pending_raw = std::fs::read_to_string(&item.path).map_err(|e| AxError::Other(e.to_string()))?;
    let ax_dir = ax_dir_from_project(project_root);

    if item.kind == "rule" {
        let local_path = rule_file(&rules_dir(&ax_dir), id);
        let local_raw = if local_path.is_file() {
            Some(std::fs::read_to_string(&local_path).map_err(|e| AxError::Other(e.to_string()))?)
        } else {
            None
        };
        let pending_doc =
            parse_rule_file(Path::new(&item.path), &pending_raw).map_err(|e| AxError::Other(e.error))?;
        let local_body = local_raw.as_ref().and_then(|r| {
            parse_rule_file(&local_path, r)
                .ok()
                .map(|d| d.body)
        });
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
    let pending_doc =
        parse_skill_file(Path::new(&item.path), &pending_raw).map_err(|e| AxError::Other(e.error))?;
    let local_body = local_raw.as_ref().and_then(|r| {
        parse_skill_file(&local_path, r).ok().map(|d| d.body)
    });
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
    std::fs::create_dir_all(pending_rules_dir(&ax_dir)).map_err(|e| AxError::Other(e.to_string()))?;
    std::fs::create_dir_all(pending_skills_dir(&ax_dir))
        .map_err(|e| AxError::Other(e.to_string()))?;
    Ok(pending_dir(&ax_dir))
}
