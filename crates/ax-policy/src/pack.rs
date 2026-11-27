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
use crate::parse::{
    format_validation, is_kebab_case, parse_rule_file, parse_skill_file, serialize_rule,
    serialize_skill, validation_error_at,
};
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
    /// Pack entries that were not imported, each with the file and the reason
    /// (unparsable frontmatter, or an id or name that differs from the manifest).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub invalid: Vec<String>,
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
    let scope =
        crate::types::PolicyScope::parse(scope).unwrap_or(crate::types::PolicyScope::Project);
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

/// Removes what an earlier export wrote (`rules/`, `skills/`,
/// `manifest.json`) so the new export starts clean. Anything else in the
/// folder is left alone. A folder that exists, is not empty and holds no pack
/// manifest is refused: the export never deletes files it did not write.
fn clear_previous_pack(pack_root: &Path) -> Result<(), AxError> {
    let meta = match std::fs::symlink_metadata(pack_root) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(AxError::Other(format!(
                "cannot read pack folder {}: {e}",
                pack_root.display()
            )))
        }
    };
    if !meta.is_dir() {
        return Err(AxError::Other(format!(
            "{} is not a folder; choose an empty folder or an existing policy pack",
            pack_root.display()
        )));
    }
    let manifest_path = pack_root.join("manifest.json");
    let is_pack = std::fs::read_to_string(&manifest_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<PackManifest>(&raw).ok())
        .is_some();
    if !is_pack {
        let mut entries = std::fs::read_dir(pack_root).map_err(|e| {
            AxError::Other(format!(
                "cannot read pack folder {}: {e}",
                pack_root.display()
            ))
        })?;
        if entries.next().is_some() {
            return Err(AxError::Other(format!(
                "{} is not a policy pack (no manifest.json) and is not empty; \
                 choose an empty folder or an existing pack",
                pack_root.display()
            )));
        }
        return Ok(());
    }
    for sub in ["rules", "skills"] {
        let dir = pack_root.join(sub);
        match std::fs::symlink_metadata(&dir) {
            Ok(m) if m.is_dir() => std::fs::remove_dir_all(&dir),
            Ok(_) => std::fs::remove_file(&dir),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
        .map_err(|e| AxError::Other(format!("cannot clear {}: {e}", dir.display())))?;
    }
    std::fs::remove_file(&manifest_path)
        .map_err(|e| AxError::Other(format!("cannot clear {}: {e}", manifest_path.display())))
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
    clear_previous_pack(&pack_root)?;
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
        if !is_pack_export_candidate(&rule.tags, &rule.scope, rule.enabled, &rule.status, &tag_l) {
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
    write_utf8(
        &pack_root.join("manifest.json"),
        &format!("{manifest_json}\n"),
    )?;

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
    import_pack_with_origin(
        pool,
        project_root,
        pack_path,
        force,
        require_review_override,
        None,
    )
    .await
}

/// Import with a source label supplied by the authenticated share transport.
pub async fn import_pack_with_origin(
    pool: &SqlitePool,
    project_root: &Path,
    pack_path: Option<&Path>,
    force: bool,
    require_review_override: Option<bool>,
    origin: Option<&str>,
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
    let export_time = if manifest.exported_at > 0 {
        format!("exported at Unix milliseconds {}", manifest.exported_at)
    } else {
        "export time not recorded".into()
    };
    let origin = format!(
        "{} · {}",
        origin.map(str::to_owned).unwrap_or_else(|| {
            format!(
                "Local policy pack {}",
                pack_root.file_name().unwrap_or_default().to_string_lossy()
            )
        }),
        export_time
    );
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
        if !is_kebab_case(&meta.id) {
            result.invalid.push(format!(
                "rule `{}`: manifest id must be kebab-case",
                meta.id
            ));
            continue;
        }
        let path = pack_root.join("rules").join(format!("{}.mdc", meta.id));
        if !path.is_file() {
            result.skipped += 1;
            continue;
        }
        let raw = std::fs::read_to_string(&path).map_err(|e| AxError::Other(e.to_string()))?;
        let mut doc = match parse_rule_file(&path, &raw) {
            Ok(doc) => doc,
            Err(e) => {
                result
                    .invalid
                    .push(format!("{}: {}", path.display(), format_validation(&e)));
                continue;
            }
        };
        if doc.frontmatter.id != meta.id {
            result.invalid.push(format!(
                "{}: frontmatter id `{}` does not match manifest id `{}`",
                path.display(),
                doc.frontmatter.id,
                meta.id
            ));
            continue;
        }
        let pack_hash = content_hash(&raw);

        match local_rule_map.get(&meta.id) {
            None => {
                if require_review {
                    stage_pending_rule(&ax_dir, &mut doc, &origin).await?;
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
                    stage_pending_rule(&ax_dir, &mut doc, &origin).await?;
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
        if !is_kebab_case(&meta.id) {
            result.invalid.push(format!(
                "skill `{}`: manifest name must be kebab-case",
                meta.id
            ));
            continue;
        }
        let path = pack_root.join("skills").join(&meta.id).join(SKILL_FILENAME);
        if !path.is_file() {
            result.skipped += 1;
            continue;
        }
        let raw = std::fs::read_to_string(&path).map_err(|e| AxError::Other(e.to_string()))?;
        let mut doc = match parse_skill_file(&path, &raw) {
            Ok(doc) => doc,
            Err(e) => {
                result
                    .invalid
                    .push(format!("{}: {}", path.display(), format_validation(&e)));
                continue;
            }
        };
        if doc.frontmatter.name != meta.id {
            result.invalid.push(format!(
                "{}: frontmatter name `{}` does not match manifest name `{}`",
                path.display(),
                doc.frontmatter.name,
                meta.id
            ));
            continue;
        }
        let pack_hash = content_hash(&raw);

        match local_skill_map.get(&meta.id) {
            None => {
                if require_review {
                    stage_pending_skill(&ax_dir, &mut doc, &origin).await?;
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
                    stage_pending_skill(&ax_dir, &mut doc, &origin).await?;
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
    let _ = crate::ide_seed::sync_ide_bootstrap_with_force(project_root, true, false);

    Ok(result)
}

async fn apply_rule(store: &PolicyStore, doc: PolicyRuleDoc) -> Result<(), AxError> {
    let path = PathBuf::from(&doc.source_path);
    store
        .save_rule(doc.frontmatter, doc.body)
        .await
        .map_err(|e| validation_error_at(&path, &e))?;
    Ok(())
}

async fn apply_skill(store: &PolicyStore, doc: PolicySkillDoc) -> Result<(), AxError> {
    let path = PathBuf::from(&doc.source_path);
    store
        .save_skill(doc.frontmatter, doc.body)
        .await
        .map_err(|e| validation_error_at(&path, &e))?;
    Ok(())
}

async fn stage_pending_rule(
    ax_dir: &Path,
    doc: &mut PolicyRuleDoc,
    origin: &str,
) -> Result<(), AxError> {
    doc.frontmatter.status = PolicyItemStatus::Pending.as_str().into();
    let raw = serialize_rule(&doc.frontmatter, &doc.body);
    let dir = pending_rules_dir(ax_dir);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| AxError::Other(e.to_string()))?;
    write_pending_with_origin(rule_file(&dir, &doc.frontmatter.id), raw, origin.to_owned()).await
}

async fn stage_pending_skill(
    ax_dir: &Path,
    doc: &mut PolicySkillDoc,
    origin: &str,
) -> Result<(), AxError> {
    doc.frontmatter.status = PolicyItemStatus::Pending.as_str().into();
    let raw = serialize_skill(&doc.frontmatter, &doc.body);
    let dir = pending_skills_dir(ax_dir).join(&doc.frontmatter.name);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| AxError::Other(e.to_string()))?;
    write_pending_with_origin(dir.join(SKILL_FILENAME), raw, origin.to_owned()).await
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
        .filter(|r| is_pack_export_candidate(&r.tags, &r.scope, r.enabled, &r.status, DEFAULT_TAG))
        .count();
    let local_shared_skills = skills
        .iter()
        .filter(|s| is_pack_export_candidate(&s.tags, &s.scope, s.enabled, &s.status, DEFAULT_TAG))
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
    rule_file(&rules_dir(&ax_dir_from_project(project_root)), id)
}

pub fn active_skill_path(project_root: &Path, name: &str) -> PathBuf {
    skill_file(&skills_dir(&ax_dir_from_project(project_root)), name)
}

async fn write_pending_with_origin(
    path: PathBuf,
    raw: String,
    origin: String,
) -> Result<(), AxError> {
    tokio::task::spawn_blocking(move || {
        write_utf8(&path, &raw)?;
        crate::provenance::record_pending_origin(&path, &raw, &origin)
    })
    .await
    .map_err(|e| AxError::Other(e.to_string()))?
}

#[cfg(test)]
mod export_tests {
    use super::*;

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        ax_db::schema::apply_initial_schema(&pool).await.unwrap();
        ax_db::migrations::run_migrations(&pool, 0).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn export_refuses_a_folder_that_is_not_a_pack() {
        let root = tempfile::tempdir().unwrap();
        let out = root.path().join("documents");
        std::fs::create_dir_all(out.join("notes")).unwrap();
        std::fs::write(out.join("notes/keep.txt"), "mine\n").unwrap();
        std::fs::write(out.join("report.md"), "mine\n").unwrap();
        let pool = pool().await;

        let err = export_pack(&pool, root.path(), "shared", Some(&out))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not a policy pack"), "{err}");
        assert_eq!(
            std::fs::read_to_string(out.join("notes/keep.txt")).unwrap(),
            "mine\n"
        );
        assert_eq!(
            std::fs::read_to_string(out.join("report.md")).unwrap(),
            "mine\n"
        );
        assert!(!out.join("manifest.json").exists());
    }

    #[tokio::test]
    async fn export_into_an_existing_pack_replaces_only_pack_contents() {
        let root = tempfile::tempdir().unwrap();
        let out = root.path().join("pack");
        let pool = pool().await;
        export_pack(&pool, root.path(), "shared", Some(&out))
            .await
            .unwrap();
        std::fs::write(out.join("rules/stale.mdc"), "old\n").unwrap();
        std::fs::write(out.join("README.md"), "about this pack\n").unwrap();

        export_pack(&pool, root.path(), "shared", Some(&out))
            .await
            .unwrap();
        assert!(!out.join("rules/stale.mdc").exists());
        assert!(out.join("manifest.json").is_file());
        assert_eq!(
            std::fs::read_to_string(out.join("README.md")).unwrap(),
            "about this pack\n"
        );
    }

    #[tokio::test]
    async fn export_into_an_empty_or_new_folder_works() {
        let root = tempfile::tempdir().unwrap();
        let pool = pool().await;
        let empty = root.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        export_pack(&pool, root.path(), "shared", Some(&empty))
            .await
            .unwrap();
        assert!(empty.join("manifest.json").is_file());
        let fresh = root.path().join("fresh/pack");
        export_pack(&pool, root.path(), "shared", Some(&fresh))
            .await
            .unwrap();
        assert!(fresh.join("manifest.json").is_file());
    }
}

#[cfg(test)]
mod import_id_tests {
    use super::*;

    async fn project_with_local_policy() -> (tempfile::TempDir, SqlitePool) {
        let root = tempfile::tempdir().unwrap();
        let agents = root.path().join(".agents");
        std::fs::create_dir_all(agents.join("rules")).unwrap();
        std::fs::create_dir_all(agents.join("skills/deploy")).unwrap();
        std::fs::write(
            agents.join("rules/english-only.mdc"),
            "---\nid: english-only\nlevel: CRITICAL\nalwaysApply: true\n---\nLocal rule\n",
        )
        .unwrap();
        std::fs::write(
            agents.join("skills/deploy/SKILL.md"),
            "---\nname: deploy\ndescription: Local deploy\n---\nLocal skill\n",
        )
        .unwrap();
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        ax_db::schema::apply_initial_schema(&pool).await.unwrap();
        ax_db::migrations::run_migrations(&pool, 0).await.unwrap();
        index_policy(&pool, root.path(), false).await.unwrap();
        (root, pool)
    }

    fn write_mismatched_pack(pack: &Path) {
        std::fs::create_dir_all(pack.join("rules")).unwrap();
        std::fs::create_dir_all(pack.join("skills/new-skill")).unwrap();
        std::fs::write(
            pack.join("rules/new-rule.mdc"),
            "---\nid: english-only\nlevel: INFO\nalwaysApply: true\n---\nHijacked rule\n",
        )
        .unwrap();
        std::fs::write(
            pack.join("skills/new-skill/SKILL.md"),
            "---\nname: deploy\ndescription: Hijacked\n---\nHijacked skill\n",
        )
        .unwrap();
        let manifest = serde_json::json!({
            "schemaVersion": 1, "exportedAt": 1, "tag": "shared",
            "rules": [{"id": "new-rule", "contentHash": "", "updatedAt": 1, "kind": "rule"}],
            "skills": [{"id": "new-skill", "contentHash": "", "updatedAt": 1, "kind": "skill"}],
        });
        std::fs::write(pack.join("manifest.json"), manifest.to_string()).unwrap();
    }

    #[tokio::test]
    async fn an_entry_whose_file_names_another_id_cannot_overwrite_it() {
        let (root, pool) = project_with_local_policy().await;
        let pack = root.path().join("team-pack");
        write_mismatched_pack(&pack);

        let result = import_pack_with_options(&pool, root.path(), Some(&pack), false, Some(false))
            .await
            .unwrap();
        assert_eq!((result.rules_added, result.skills_added), (0, 0));
        assert_eq!(result.invalid.len(), 2, "{:?}", result.invalid);
        assert!(
            result.invalid[0].contains("english-only"),
            "{:?}",
            result.invalid
        );
        let rules = list_rules(&pool).await.unwrap();
        let rule = rules.iter().find(|r| r.id == "english-only").unwrap();
        assert_eq!(rule.body.trim(), "Local rule");
        assert_eq!(rule.level, "CRITICAL");
        let skills = list_skills(&pool).await.unwrap();
        let skill = skills.iter().find(|s| s.name == "deploy").unwrap();
        assert_eq!(skill.body.trim(), "Local skill");
    }

    #[tokio::test]
    async fn a_mismatched_entry_is_not_staged_for_review_either() {
        let (root, pool) = project_with_local_policy().await;
        let pack = root.path().join("team-pack");
        write_mismatched_pack(&pack);

        let result = import_pack_with_options(&pool, root.path(), Some(&pack), false, Some(true))
            .await
            .unwrap();
        assert_eq!((result.rules_pending, result.skills_pending), (0, 0));
        assert_eq!(result.invalid.len(), 2);
        assert!(crate::list_pending(root.path()).unwrap().is_empty());
    }
}

#[cfg(test)]
mod import_parse_tests {
    use super::*;

    #[tokio::test]
    async fn an_unparsable_pack_file_is_listed_and_the_rest_imports() {
        let root = tempfile::tempdir().unwrap();
        let pack = root.path().join("team-pack");
        std::fs::create_dir_all(pack.join("rules")).unwrap();
        std::fs::write(
            pack.join("rules/broken.mdc"),
            "---\nid: broken\nalwaysApply: true\n---\nNo level\n",
        )
        .unwrap();
        std::fs::write(
            pack.join("rules/fine.mdc"),
            "---\nid: fine\nlevel: INFO\nalwaysApply: true\n---\nFine\n",
        )
        .unwrap();
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        ax_db::schema::apply_initial_schema(&pool).await.unwrap();
        ax_db::migrations::run_migrations(&pool, 0).await.unwrap();
        let result = import_pack_with_options(&pool, root.path(), Some(&pack), false, Some(false))
            .await
            .unwrap();
        assert_eq!(result.rules_added, 1);
        assert_eq!(result.invalid.len(), 1, "{:?}", result.invalid);
        assert!(
            result.invalid[0].contains("broken.mdc"),
            "{:?}",
            result.invalid
        );
        assert!(
            result.invalid[0].contains("level: required"),
            "{:?}",
            result.invalid
        );
    }
}

#[cfg(test)]
mod provenance_tests {
    use super::*;

    #[tokio::test]
    async fn pack_import_records_sources_and_kind_safe_rejection_cleans_them() {
        let root = tempfile::tempdir().unwrap();
        let pack = root.path().join("team-pack");
        std::fs::create_dir_all(pack.join("rules")).unwrap();
        std::fs::create_dir_all(pack.join("skills/same")).unwrap();
        std::fs::write(
            pack.join("rules/same.mdc"),
            "---\nid: same\nlevel: INFO\nalwaysApply: true\nstatus: approved\n---\nIncoming rule\n",
        )
        .unwrap();
        std::fs::write(pack.join("skills/same/SKILL.md"), "---\nname: same\ndescription: Incoming skill\nstatus: approved\n---\nIncoming skill body\n").unwrap();
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        ax_db::schema::apply_initial_schema(&pool).await.unwrap();
        ax_db::migrations::run_migrations(&pool, 0).await.unwrap();
        let result = import_pack_with_options(&pool, root.path(), Some(&pack), false, Some(true))
            .await
            .unwrap();
        assert_eq!((result.rules_pending, result.skills_pending), (1, 1));
        let items = crate::list_pending(root.path()).unwrap();
        for item in &items {
            let path = Path::new(&item.path);
            let raw = std::fs::read_to_string(path).unwrap();
            assert!(crate::provenance::pending_origin(path, &raw)
                .await
                .unwrap()
                .contains("Local policy pack team-pack"));
            assert!(crate::provenance::pending_origin(path, &raw)
                .await
                .unwrap()
                .contains("export time not recorded"));
        }
        import_pack_with_origin(
            &pool,
            root.path(),
            Some(&pack),
            false,
            Some(true),
            Some("Git policy share https://example.com/team · branch main"),
        )
        .await
        .unwrap();
        for item in &items {
            let path = Path::new(&item.path);
            let raw = std::fs::read_to_string(path).unwrap();
            assert!(crate::provenance::pending_origin(path, &raw)
                .await
                .unwrap()
                .starts_with("Git policy share https://example.com/team"));
        }
        crate::reject_pending_kind(&pool, root.path(), "same", Some("rule"))
            .await
            .unwrap();
        assert!(!Path::new(&items[0].path)
            .with_extension("origin.json")
            .exists());
        assert!(Path::new(&items[1].path)
            .with_extension("origin.json")
            .exists());
        crate::reject_pending_kind(&pool, root.path(), "same", Some("skill"))
            .await
            .unwrap();
        assert!(!Path::new(&items[1].path)
            .with_extension("origin.json")
            .exists());
    }
    #[tokio::test]
    async fn pack_import_never_writes_a_rule_source_outside_policy_files() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let victim = outside.path().join("victim.mdc");
        std::fs::write(&victim, "keep me\n").unwrap();
        let hooks = root.path().join(".git/hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        let hook = hooks.join("post-merge");
        std::fs::write(&hook, "#!/bin/sh\necho ok\n").unwrap();
        let main_rs = root.path().join("src/main.rs");
        std::fs::create_dir_all(main_rs.parent().unwrap()).unwrap();
        std::fs::write(&main_rs, "fn main() {}\n").unwrap();

        let pack = root.path().join("team-pack");
        std::fs::create_dir_all(pack.join("rules")).unwrap();
        let abs = victim.display().to_string().replace('\\', "/");
        for (id, source) in [
            ("abs", abs.as_str()),
            ("hook", ".git/hooks/post-merge"),
            ("code", "src/main.rs"),
        ] {
            std::fs::write(
                pack.join(format!("rules/{id}.mdc")),
                format!("---\nid: {id}\nlevel: INFO\nalwaysApply: true\nstatus: approved\nsource: \"{source}\"\n---\nInjected body\n"),
            )
            .unwrap();
        }
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        ax_db::schema::apply_initial_schema(&pool).await.unwrap();
        ax_db::migrations::run_migrations(&pool, 0).await.unwrap();
        let _ = import_pack_with_options(&pool, root.path(), Some(&pack), true, Some(false)).await;
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep me\n");
        assert_eq!(
            std::fs::read_to_string(&hook).unwrap(),
            "#!/bin/sh\necho ok\n"
        );
        assert_eq!(std::fs::read_to_string(&main_rs).unwrap(), "fn main() {}\n");
    }
}
