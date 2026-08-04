//! Per-project policy pack export/import for team git sync.
//!
//! Default flow (`--tag shared`):
//! 1. Export all **packable** scopes (project + workspace), enabled + approved
//! 2. Skip company / private scopes and items tagged `local` or `noshare`
//! 3. `ax policy pack export` writes `.ax/policy/shared/`
//! 4. Commit the pack; teammates run `ax policy pack import` (or post-merge hook)
//!
//! Custom `--tag foo` still filters to items that carry that tag.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sqlx::SqlitePool;

use ax_utils::errors::AxError;

use crate::config::load_policy_config;
use crate::index::{index_policy, list_rules, list_skills, rule_row_to_doc, skill_row_to_doc};
use crate::parse::{parse_rule_file, parse_skill_file, serialize_rule, serialize_skill};
use crate::paths::{
    ax_dir_from_project, pending_rules_dir, pending_skills_dir, rule_file, rules_dir, shared_dir,
    skill_file, skills_dir, SKILL_FILENAME,
};
use crate::store::PolicyStore;
use crate::types::{PolicyItemStatus, PolicyRuleDoc, PolicySkillDoc};

const MANIFEST_VERSION: u32 = 1;
const DEFAULT_TAG: &str = "shared";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackManifest {
    pub schema_version: u32,
    pub exported_at: i64,
    pub tag: String,
    pub rules: Vec<PackItemMeta>,
    pub skills: Vec<PackItemMeta>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackItemMeta {
    pub id: String,
    pub content_hash: String,
    pub updated_at: i64,
    pub kind: String,
}

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackExportResult {
    pub rules_exported: usize,
    pub skills_exported: usize,
    pub path: String,
}

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackImportResult {
    pub rules_added: usize,
    pub skills_added: usize,
    pub rules_updated: usize,
    pub skills_updated: usize,
    pub rules_pending: usize,
    pub skills_pending: usize,
    pub skipped: usize,
    pub conflicts: usize,
}

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackStatus {
    pub pack_path: String,
    pub has_manifest: bool,
    pub rules_in_pack: usize,
    pub skills_in_pack: usize,
    pub exported_at: Option<i64>,
    pub tag: Option<String>,
    pub local_shared_rules: usize,
    pub local_shared_skills: usize,
    pub require_review: bool,
    pub policy_sync: bool,
}

pub fn default_pack_path(project_root: &Path) -> PathBuf {
    shared_dir(&ax_dir_from_project(project_root))
}

fn content_hash(s: &str) -> String {
    blake3::hash(s.as_bytes()).to_hex().to_string()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn has_tag(tags: &[String], tag: &str) -> bool {
    tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
}

fn is_approved(status: &str) -> bool {
    matches!(
        PolicyItemStatus::parse(status).unwrap_or(PolicyItemStatus::Approved),
        PolicyItemStatus::Approved
    )
}

/// Whether a rule/skill should be included in a pack export for `tag_filter`.
fn is_pack_export_candidate(
    tags: &[String],
    scope: &str,
    enabled: bool,
    status: &str,
    tag_filter: &str,
) -> bool {
    if !enabled || !is_approved(status) {
        return false;
    }
    let scope = crate::types::PolicyScope::parse(scope)
        .unwrap_or(crate::types::PolicyScope::Project);
    if !scope.is_packable() {
        return false;
    }
    // Explicit opt-out from team packs.
    if has_tag(tags, "local") || has_tag(tags, "noshare") {
        return false;
    }
    // Default team pack: all packable project/workspace items.
    if tag_filter.eq_ignore_ascii_case(DEFAULT_TAG) {
        return true;
    }
    has_tag(tags, tag_filter)
}

fn write_utf8(path: &Path, content: &str) -> Result<(), AxError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AxError::Other(e.to_string()))?;
    }
    std::fs::write(path, content.as_bytes()).map_err(|e| AxError::Other(e.to_string()))
}

/// Export shareable rules/skills into `.ax/policy/shared/`.
pub async fn export_pack(
    pool: &SqlitePool,
    project_root: &Path,
    tag: &str,
    out: Option<&Path>,
) -> Result<PackExportResult, AxError> {
    let pack_root = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_pack_path(project_root));
    let rules_out = pack_root.join("rules");
    let skills_out = pack_root.join("skills");
    // Clear previous pack contents for a clean export.
    if pack_root.exists() {
        let _ = std::fs::remove_dir_all(&pack_root);
    }
    std::fs::create_dir_all(&rules_out).map_err(|e| AxError::Other(e.to_string()))?;
    std::fs::create_dir_all(&skills_out).map_err(|e| AxError::Other(e.to_string()))?;

    let rules = list_rules(pool).await?;
    let skills = list_skills(pool).await?;
    let tag_l = tag.to_ascii_lowercase();
    let exported_at = now_ms();

    let mut manifest = PackManifest {
        schema_version: MANIFEST_VERSION,
        exported_at,
        tag: tag.to_string(),
        rules: Vec::new(),
        skills: Vec::new(),
    };

    for rule in &rules {
        if !is_pack_export_candidate(
            &rule.tags,
            &rule.scope,
            rule.enabled,
            &rule.status,
            &tag_l,
        ) {
            continue;
        }
        let doc = rule_row_to_doc(rule, project_root);
        let raw = serialize_rule(&doc.frontmatter, &doc.body);
        let hash = content_hash(&raw);
        let path = rules_out.join(format!("{}.mdc", rule.id));
        write_utf8(&path, &raw)?;
        manifest.rules.push(PackItemMeta {
            id: rule.id.clone(),
            content_hash: hash,
            updated_at: exported_at,
            kind: "rule".into(),
        });
    }

    for skill in &skills {
        if !is_pack_export_candidate(
            &skill.tags,
            &skill.scope,
            skill.enabled,
            &skill.status,
            &tag_l,
        ) {
            continue;
        }
        let doc = skill_row_to_doc(skill, project_root);
        let raw = serialize_skill(&doc.frontmatter, &doc.body);
        let hash = content_hash(&raw);
        let dir = skills_out.join(&skill.name);
        std::fs::create_dir_all(&dir).map_err(|e| AxError::Other(e.to_string()))?;
        write_utf8(&dir.join(SKILL_FILENAME), &raw)?;
        manifest.skills.push(PackItemMeta {
            id: skill.name.clone(),
            content_hash: hash,
            updated_at: exported_at,
            kind: "skill".into(),
        });
    }

    let manifest_json =
        serde_json::to_string_pretty(&manifest).map_err(|e| AxError::Other(e.to_string()))?;
    write_utf8(&pack_root.join("manifest.json"), &format!("{manifest_json}\n"))?;

    Ok(PackExportResult {
        rules_exported: manifest.rules.len(),
        skills_exported: manifest.skills.len(),
        path: pack_root.display().to_string(),
    })
}

/// Import pack into local policy. Respects `requireReview` and `--force`.
pub async fn import_pack(
    pool: &SqlitePool,
    project_root: &Path,
    pack_path: Option<&Path>,
    force: bool,
) -> Result<PackImportResult, AxError> {
    import_pack_with_options(pool, project_root, pack_path, force, None).await
}

/// Import pack with optional `require_review` override (remote share sync).
pub async fn import_pack_with_options(
    pool: &SqlitePool,
    project_root: &Path,
    pack_path: Option<&Path>,
    force: bool,
    require_review_override: Option<bool>,
) -> Result<PackImportResult, AxError> {
    let pack_root = pack_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_pack_path(project_root));
    if !pack_root.is_dir() {
        return Err(AxError::Other(format!(
            "pack not found: {}",
            pack_root.display()
        )));
    }

    let manifest = read_manifest(&pack_root)?;
    let cfg = load_policy_config(project_root);
    let require_review = require_review_override.unwrap_or(cfg.require_review);
    let store = PolicyStore::new(pool.clone(), project_root.to_path_buf());

    let local_rules = list_rules(pool).await?;
    let local_skills = list_skills(pool).await?;
    let local_rule_map: HashMap<String, String> = local_rules
        .iter()
        .map(|r| {
            let doc = rule_row_to_doc(r, project_root);
            let raw = serialize_rule(&doc.frontmatter, &doc.body);
            (r.id.clone(), content_hash(&raw))
        })
        .collect();
    let local_skill_map: HashMap<String, String> = local_skills
        .iter()
        .map(|s| {
            let doc = skill_row_to_doc(s, project_root);
            let raw = serialize_skill(&doc.frontmatter, &doc.body);
            (s.name.clone(), content_hash(&raw))
        })
        .collect();

    let mut result = PackImportResult::default();
    let ax_dir = ax_dir_from_project(project_root);

    for meta in &manifest.rules {
        let path = pack_root.join("rules").join(format!("{}.mdc", meta.id));
        if !path.is_file() {
            result.skipped += 1;
            continue;
        }
        let raw = std::fs::read_to_string(&path).map_err(|e| AxError::Other(e.to_string()))?;
        let mut doc = parse_rule_file(&path, &raw).map_err(|e| AxError::Other(e.error))?;
        let pack_hash = content_hash(&raw);

        match local_rule_map.get(&meta.id) {
            None => {
                if require_review {
                    stage_pending_rule(&ax_dir, &mut doc)?;
                    result.rules_pending += 1;
                } else {
                    apply_rule(&store, doc).await?;
                    result.rules_added += 1;
                }
            }
            Some(local_hash) if local_hash == &pack_hash => {
                result.skipped += 1;
            }
            Some(_) => {
                if require_review || !force {
                    stage_pending_rule(&ax_dir, &mut doc)?;
                    result.rules_pending += 1;
                    if !require_review {
                        result.conflicts += 1;
                    }
                } else {
                    apply_rule(&store, doc).await?;
                    result.rules_updated += 1;
                }
            }
        }
    }

    for meta in &manifest.skills {
        let path = pack_root
            .join("skills")
            .join(&meta.id)
            .join(SKILL_FILENAME);
        if !path.is_file() {
            result.skipped += 1;
            continue;
        }
        let raw = std::fs::read_to_string(&path).map_err(|e| AxError::Other(e.to_string()))?;
        let mut doc = parse_skill_file(&path, &raw).map_err(|e| AxError::Other(e.error))?;
        let pack_hash = content_hash(&raw);

        match local_skill_map.get(&meta.id) {
            None => {
                if require_review {
                    stage_pending_skill(&ax_dir, &mut doc)?;
                    result.skills_pending += 1;
                } else {
                    apply_skill(&store, doc).await?;
                    result.skills_added += 1;
                }
            }
            Some(local_hash) if local_hash == &pack_hash => {
                result.skipped += 1;
            }
            Some(_) => {
                if require_review || !force {
                    stage_pending_skill(&ax_dir, &mut doc)?;
                    result.skills_pending += 1;
                    if !require_review {
                        result.conflicts += 1;
                    }
                } else {
                    apply_skill(&store, doc).await?;
                    result.skills_updated += 1;
                }
            }
        }
    }

    index_policy(pool, project_root, false).await?;

    // Refresh every IDE bootstrap (Cursor, Continue, Claude, …) so a teammate on a
    // different agent still gets ax_preflight + Continue MCP after git pull/import.
    let _ = crate::ide_seed::sync_ide_bootstrap(project_root, true);

    Ok(result)
}

async fn apply_rule(store: &PolicyStore, doc: PolicyRuleDoc) -> Result<(), AxError> {
    store
        .save_rule(doc.frontmatter, doc.body)
        .await
        .map_err(|e| AxError::Other(e.error))?;
    Ok(())
}

async fn apply_skill(store: &PolicyStore, doc: PolicySkillDoc) -> Result<(), AxError> {
    store
        .save_skill(doc.frontmatter, doc.body)
        .await
        .map_err(|e| AxError::Other(e.error))?;
    Ok(())
}

fn stage_pending_rule(ax_dir: &Path, doc: &mut PolicyRuleDoc) -> Result<(), AxError> {
    doc.frontmatter.status = PolicyItemStatus::Pending.as_str().into();
    let raw = serialize_rule(&doc.frontmatter, &doc.body);
    let dir = pending_rules_dir(ax_dir);
    std::fs::create_dir_all(&dir).map_err(|e| AxError::Other(e.to_string()))?;
    write_utf8(&rule_file(&dir, &doc.frontmatter.id), &raw)
}

fn stage_pending_skill(ax_dir: &Path, doc: &mut PolicySkillDoc) -> Result<(), AxError> {
    doc.frontmatter.status = PolicyItemStatus::Pending.as_str().into();
    let raw = serialize_skill(&doc.frontmatter, &doc.body);
    let dir = pending_skills_dir(ax_dir).join(&doc.frontmatter.name);
    std::fs::create_dir_all(&dir).map_err(|e| AxError::Other(e.to_string()))?;
    write_utf8(&dir.join(SKILL_FILENAME), &raw)
}

fn read_manifest(pack_root: &Path) -> Result<PackManifest, AxError> {
    let path = pack_root.join("manifest.json");
    if path.is_file() {
        let raw = std::fs::read_to_string(&path).map_err(|e| AxError::Other(e.to_string()))?;
        return serde_json::from_str(&raw).map_err(|e| AxError::Other(e.to_string()));
    }
    // Fallback: scan directory without manifest.
    let mut manifest = PackManifest {
        schema_version: MANIFEST_VERSION,
        exported_at: 0,
        tag: DEFAULT_TAG.into(),
        rules: Vec::new(),
        skills: Vec::new(),
    };
    let rules_dir = pack_root.join("rules");
    if rules_dir.is_dir() {
        for entry in std::fs::read_dir(&rules_dir).map_err(|e| AxError::Other(e.to_string()))? {
            let entry = entry.map_err(|e| AxError::Other(e.to_string()))?;
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(id) = name.strip_suffix(".mdc") {
                let raw = std::fs::read_to_string(entry.path())
                    .map_err(|e| AxError::Other(e.to_string()))?;
                manifest.rules.push(PackItemMeta {
                    id: id.to_string(),
                    content_hash: content_hash(&raw),
                    updated_at: 0,
                    kind: "rule".into(),
                });
            }
        }
    }
    let skills_dir = pack_root.join("skills");
    if skills_dir.is_dir() {
        for entry in std::fs::read_dir(&skills_dir).map_err(|e| AxError::Other(e.to_string()))? {
            let entry = entry.map_err(|e| AxError::Other(e.to_string()))?;
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let skill_path = entry.path().join(SKILL_FILENAME);
            if !skill_path.is_file() {
                continue;
            }
            let raw =
                std::fs::read_to_string(&skill_path).map_err(|e| AxError::Other(e.to_string()))?;
            manifest.skills.push(PackItemMeta {
                id: name,
                content_hash: content_hash(&raw),
                updated_at: 0,
                kind: "skill".into(),
            });
        }
    }
    Ok(manifest)
}

pub async fn pack_status(pool: &SqlitePool, project_root: &Path) -> Result<PackStatus, AxError> {
    let pack_root = default_pack_path(project_root);
    let cfg = load_policy_config(project_root);
    let rules = list_rules(pool).await?;
    let skills = list_skills(pool).await?;
    let local_shared_rules = rules
        .iter()
        .filter(|r| {
            is_pack_export_candidate(&r.tags, &r.scope, r.enabled, &r.status, DEFAULT_TAG)
        })
        .count();
    let local_shared_skills = skills
        .iter()
        .filter(|s| {
            is_pack_export_candidate(&s.tags, &s.scope, s.enabled, &s.status, DEFAULT_TAG)
        })
        .count();

    let mut status = PackStatus {
        pack_path: pack_root.display().to_string(),
        has_manifest: pack_root.join("manifest.json").is_file(),
        rules_in_pack: 0,
        skills_in_pack: 0,
        exported_at: None,
        tag: None,
        local_shared_rules,
        local_shared_skills,
        require_review: cfg.require_review,
        policy_sync: crate::config::policy_sync_enabled(project_root),
    };

    if let Ok(manifest) = read_manifest(&pack_root) {
        status.rules_in_pack = manifest.rules.len();
        status.skills_in_pack = manifest.skills.len();
        status.exported_at = Some(manifest.exported_at);
        status.tag = Some(manifest.tag);
    }

    Ok(status)
}

/// Convenience: paths helpers for tests / review.
pub fn active_rule_path(project_root: &Path, id: &str) -> PathBuf {
    rule_file(
        &rules_dir(&ax_dir_from_project(project_root)),
        id,
    )
}

pub fn active_skill_path(project_root: &Path, name: &str) -> PathBuf {
    skill_file(
        &skills_dir(&ax_dir_from_project(project_root)),
        name,
    )
}
