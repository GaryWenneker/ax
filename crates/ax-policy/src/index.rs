use sqlx::SqlitePool;
use std::path::Path;

use ax_utils::errors::{AxError, DatabaseError};

use crate::config::{effective_storage, load_policy_config, PolicyStorage};
use crate::parse::{parse_rule_file, parse_skill_file, serialize_rule, serialize_skill};
use crate::paths::{
    ax_dir_from_project, ensure_policy_dirs, is_stub_body, policy_root, resolve_source_path,
    rule_file, rules_dir, skill_file, skills_dir,
};
use crate::types::{
    PolicyIndexResult, PolicyRuleDoc, PolicyRuleRow, PolicySkillDoc, PolicySkillRow,
    RuleFrontmatter, SkillFrontmatter,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    /// Upsert from disk; delete DB rows with no matching file (filesystem is source of truth).
    Replace,
    /// Upsert from disk; keep DB-only rows.
    Merge,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub rules_exported: u32,
    pub skills_exported: u32,
    pub output_dir: String,
}

pub async fn index_policy(
    pool: &SqlitePool,
    project_root: &Path,
    force: bool,
) -> Result<PolicyIndexResult, AxError> {
    let config = load_policy_config(project_root);
    match config.storage {
        PolicyStorage::Database => {
            if force {
                import_policy_from_files(pool, project_root, ImportMode::Merge).await
            } else {
                db_counts(pool).await
            }
        }
        PolicyStorage::Files => import_policy_from_files(pool, project_root, ImportMode::Replace).await,
    }
}

pub async fn import_policy_from_files(
    pool: &SqlitePool,
    project_root: &Path,
    mode: ImportMode,
) -> Result<PolicyIndexResult, AxError> {
    let ax_dir = ax_dir_from_project(project_root);
    ensure_policy_dirs(&ax_dir).map_err(|e| AxError::Other(e.to_string()))?;

    let mut rules_indexed = 0u32;
    let mut skills_indexed = 0u32;
    let mut seen_rules = Vec::new();
    let mut seen_skills = Vec::new();

    // Hierarchical merge: company → workspace → project → private (later upserts win).
    let layers = crate::hierarchy::policy_layers(project_root);
    let layer_list = if layers.is_empty() {
        vec![crate::hierarchy::PolicyLayer {
            dir: policy_root(&ax_dir),
            scope: crate::types::PolicyScope::Project,
            root_id: None,
        }]
    } else {
        layers
    };

    for layer in &layer_list {
        let (r, s, mut rule_ids, mut skill_ids) =
            import_one_policy_dir(pool, project_root, &layer.dir, layer.scope).await?;
        rules_indexed += r;
        skills_indexed += s;
        seen_rules.append(&mut rule_ids);
        seen_skills.append(&mut skill_ids);
    }

    if mode == ImportMode::Replace {
        // Hybrid-aware prune: keep per-item database overrides not present on disk.
        seen_rules.sort();
        seen_rules.dedup();
        seen_skills.sort();
        seen_skills.dedup();
        prune_rules_hybrid(pool, &seen_rules).await?;
        prune_skills_hybrid(pool, &seen_skills).await?;
    }

    Ok(PolicyIndexResult {
        rules_indexed,
        skills_indexed,
    })
}

async fn import_one_policy_dir(
    pool: &SqlitePool,
    project_root: &Path,
    policy_dir: &Path,
    scope: crate::types::PolicyScope,
) -> Result<(u32, u32, Vec<String>, Vec<String>), AxError> {
    let mut rules_indexed = 0u32;
    let mut skills_indexed = 0u32;
    let mut seen_rules = Vec::new();
    let mut seen_skills = Vec::new();
    let scope_s = scope.as_str().to_string();

    let rules_path = policy_dir.join(crate::paths::RULES_DIR);
    if rules_path.is_dir() {
        for entry in walkdir::WalkDir::new(&rules_path)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("mdc") {
                continue;
            }
            let raw = std::fs::read_to_string(path).map_err(|e| AxError::Other(e.to_string()))?;
            let mut doc = parse_rule_file(path, &raw).map_err(|e| AxError::Other(e.error))?;
            // Directory layer wins over frontmatter when importing from a scoped path.
            doc.frontmatter.scope = scope_s.clone();
            if let Err(e) = materialize_rule_stub(project_root, &mut doc) {
                eprintln!("[ax policy] skip rule stub {}: {e}", path.display());
                continue;
            }
            let hash = blake3::hash(doc.raw.as_bytes()).to_hex().to_string();
            upsert_rule(pool, &doc, &hash, now_ms()).await?;
            seen_rules.push(doc.frontmatter.id.clone());
            rules_indexed += 1;
        }
    }

    let skills_path = policy_dir.join(crate::paths::SKILLS_DIR);
    if skills_path.is_dir() {
        for entry in walkdir::WalkDir::new(&skills_path)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let skill_path = skill_file(&skills_path, &name);
            if !skill_path.is_file() {
                continue;
            }
            let raw = std::fs::read_to_string(&skill_path).map_err(|e| AxError::Other(e.to_string()))?;
            let mut doc = parse_skill_file(&skill_path, &raw).map_err(|e| AxError::Other(e.error))?;
            doc.frontmatter.scope = scope_s.clone();
            if let Err(e) = materialize_skill_stub(project_root, &mut doc) {
                eprintln!("[ax policy] skip skill stub {}: {e}", skill_path.display());
                continue;
            }
            let hash = blake3::hash(doc.raw.as_bytes()).to_hex().to_string();
            upsert_skill(pool, &doc, &hash, now_ms()).await?;
            seen_skills.push(doc.frontmatter.name.clone());
            skills_indexed += 1;
        }
    }

    Ok((rules_indexed, skills_indexed, seen_rules, seen_skills))
}

/// If the doc is a stub (`source:` set), load body from the external file.
fn materialize_rule_stub(project_root: &Path, doc: &mut PolicyRuleDoc) -> Result<(), String> {
    let Some(ref source) = doc.frontmatter.source.clone() else {
        return Ok(());
    };
    if !is_stub_body(&doc.body) && !doc.body.trim().is_empty() {
        // Non-stub body with source: keep body, still record stub path as the file we scanned.
        doc.stub_path = Some(doc.source_path.clone());
        let resolved = resolve_source_path(project_root, source)?;
        doc.source_path = resolved.to_string_lossy().into();
        return Ok(());
    }
    let stub_path = doc.source_path.clone();
    let resolved = resolve_source_path(project_root, source)?;
    if !resolved.is_file() {
        return Err(format!("source not found: {}", resolved.display()));
    }
    let raw = std::fs::read_to_string(&resolved).map_err(|e| e.to_string())?;
    let external = parse_rule_file(&resolved, &raw).map_err(|e| e.error)?;
    doc.body = external.body;
    doc.raw = serialize_rule(&doc.frontmatter, &doc.body);
    doc.stub_path = Some(stub_path);
    doc.source_path = resolved.to_string_lossy().into();
    Ok(())
}

fn materialize_skill_stub(project_root: &Path, doc: &mut PolicySkillDoc) -> Result<(), String> {
    let Some(ref source) = doc.frontmatter.source.clone() else {
        return Ok(());
    };
    if !is_stub_body(&doc.body) && !doc.body.trim().is_empty() {
        doc.stub_path = Some(doc.source_path.clone());
        let resolved = resolve_source_path(project_root, source)?;
        doc.source_path = resolved.to_string_lossy().into();
        return Ok(());
    }
    let stub_path = doc.source_path.clone();
    let resolved = resolve_source_path(project_root, source)?;
    if !resolved.is_file() {
        return Err(format!("source not found: {}", resolved.display()));
    }
    let raw = std::fs::read_to_string(&resolved).map_err(|e| e.to_string())?;
    let external = parse_skill_file(&resolved, &raw).map_err(|e| e.error)?;
    doc.body = external.body;
    doc.raw = serialize_skill(&doc.frontmatter, &doc.body);
    doc.stub_path = Some(stub_path);
    doc.source_path = resolved.to_string_lossy().into();
    Ok(())
}

pub async fn export_policy_to_files(
    pool: &SqlitePool,
    project_root: &Path,
    out_dir: &Path,
) -> Result<ExportResult, AxError> {
    let rules_out = out_dir.join("rules");
    let skills_out = out_dir.join("skills");
    std::fs::create_dir_all(&rules_out).map_err(|e| AxError::Other(e.to_string()))?;
    std::fs::create_dir_all(&skills_out).map_err(|e| AxError::Other(e.to_string()))?;

    let rules = list_rules(pool).await?;
    let skills = list_skills(pool).await?;

    for row in &rules {
        let doc = rule_row_to_doc(row, project_root);
        let path = rule_file(&rules_out, &doc.frontmatter.id);
        write_utf8(&path, &doc.raw)?;
    }

    for row in &skills {
        let doc = skill_row_to_doc(row, project_root);
        let dir = skills_out.join(&doc.frontmatter.name);
        std::fs::create_dir_all(&dir).map_err(|e| AxError::Other(e.to_string()))?;
        let path = skill_file(&skills_out, &doc.frontmatter.name);
        write_utf8(&path, &doc.raw)?;
    }

    Ok(ExportResult {
        rules_exported: rules.len() as u32,
        skills_exported: skills.len() as u32,
        output_dir: out_dir.to_string_lossy().to_string(),
    })
}

pub async fn upsert_rule_doc(pool: &SqlitePool, doc: &PolicyRuleDoc) -> Result<(), AxError> {
    let hash = blake3::hash(doc.raw.as_bytes()).to_hex().to_string();
    upsert_rule(pool, doc, &hash, now_ms()).await
}

pub async fn upsert_skill_doc(pool: &SqlitePool, doc: &PolicySkillDoc) -> Result<(), AxError> {
    let hash = blake3::hash(doc.raw.as_bytes()).to_hex().to_string();
    upsert_skill(pool, doc, &hash, now_ms()).await
}

pub async fn delete_rule_by_id(pool: &SqlitePool, id: &str) -> Result<bool, AxError> {
    let result = sqlx::query("DELETE FROM policy_rules WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(result.rows_affected() > 0)
}

pub async fn delete_skill_by_name(pool: &SqlitePool, name: &str) -> Result<bool, AxError> {
    let result = sqlx::query("DELETE FROM policy_skills WHERE name = ?")
        .bind(name)
        .execute(pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(result.rows_affected() > 0)
}

async fn db_counts(pool: &SqlitePool) -> Result<PolicyIndexResult, AxError> {
    let rules: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM policy_rules")
        .fetch_one(pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    let skills: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM policy_skills")
        .fetch_one(pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(PolicyIndexResult {
        rules_indexed: rules as u32,
        skills_indexed: skills as u32,
    })
}

fn policy_storage_label(storage: PolicyStorage) -> &'static str {
    match storage {
        PolicyStorage::Database => "database",
        PolicyStorage::Files => "files",
    }
}

fn policy_files_nonempty(project_root: &Path) -> bool {
    let ax_dir = ax_dir_from_project(project_root);
    let has_rules = rules_dir(&ax_dir)
        .read_dir()
        .map(|d| d.flatten().any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("mdc")))
        .unwrap_or(false);
    let has_skills = skills_dir(&ax_dir)
        .read_dir()
        .map(|d| {
            d.flatten().any(|e| {
                e.path()
                    .is_dir()
                    .then(|| skill_file(&skills_dir(&ax_dir), &e.file_name().to_string_lossy()).is_file())
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    has_rules || has_skills
}

/// Load policy into SQLite when the DB is empty or disk files changed (database mode),
/// or refresh from disk (files mode). Safe to call on every MCP policy tool invocation.
pub async fn ensure_policy_ready(pool: &SqlitePool, project_root: &Path) -> Result<PolicyIndexResult, AxError> {
    let config = load_policy_config(project_root);
    match config.storage {
        PolicyStorage::Database => {
            let counts = db_counts(pool).await?;
            let stale = policy_disk_stale(pool, project_root).await?;
            if (counts.rules_indexed == 0 && policy_files_nonempty(project_root)) || stale {
                import_policy_from_files(pool, project_root, ImportMode::Merge).await
            } else {
                Ok(counts)
            }
        }
        PolicyStorage::Files => import_policy_from_files(pool, project_root, ImportMode::Replace).await,
    }
}

async fn policy_disk_stale(pool: &SqlitePool, project_root: &Path) -> Result<bool, AxError> {
    let ax_dir = ax_dir_from_project(project_root);
    let rules_path = rules_dir(&ax_dir);
    if rules_path.is_dir() {
        for entry in walkdir::WalkDir::new(&rules_path)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("mdc") {
                continue;
            }
            let raw = std::fs::read_to_string(path).map_err(|e| AxError::Other(e.to_string()))?;
            let doc = parse_rule_file(path, &raw).map_err(|e| AxError::Other(e.error))?;
            let hash = blake3::hash(raw.as_bytes()).to_hex().to_string();
            let db_hash: Option<String> = sqlx::query_scalar(
                "SELECT content_hash FROM policy_rules WHERE id = ?",
            )
            .bind(&doc.frontmatter.id)
            .fetch_optional(pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
            if db_hash.as_deref() != Some(hash.as_str()) {
                return Ok(true);
            }
        }
    }

    let skills_path = skills_dir(&ax_dir);
    if skills_path.is_dir() {
        for entry in walkdir::WalkDir::new(&skills_path)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let skill_path = skill_file(&skills_path, &name);
            if !skill_path.is_file() {
                continue;
            }
            let raw = std::fs::read_to_string(&skill_path).map_err(|e| AxError::Other(e.to_string()))?;
            let doc = parse_skill_file(&skill_path, &raw).map_err(|e| AxError::Other(e.error))?;
            let hash = blake3::hash(raw.as_bytes()).to_hex().to_string();
            let db_hash: Option<String> = sqlx::query_scalar(
                "SELECT content_hash FROM policy_skills WHERE name = ?",
            )
            .bind(&doc.frontmatter.name)
            .fetch_optional(pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
            if db_hash.as_deref() != Some(hash.as_str()) {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Policy counts and storage mode for status / diagnostics.
pub async fn policy_status(pool: &SqlitePool, project_root: &Path) -> Result<crate::types::PolicyStatus, AxError> {
    let counts = db_counts(pool).await?;
    let config = load_policy_config(project_root);
    Ok(crate::types::PolicyStatus {
        indexed: counts.rules_indexed > 0 || counts.skills_indexed > 0,
        rules: counts.rules_indexed,
        skills: counts.skills_indexed,
        mode: policy_storage_label(config.storage).to_string(),
    })
}

async fn upsert_rule(
    pool: &SqlitePool,
    doc: &PolicyRuleDoc,
    hash: &str,
    now: i64,
) -> Result<(), AxError> {
    let fm = &doc.frontmatter;
    let scope = crate::types::PolicyScope::parse(&fm.scope)
        .unwrap_or(crate::types::PolicyScope::Project)
        .as_str();
    sqlx::query(
        "INSERT INTO policy_rules (id, level, always_apply, globs, triggers, tags, priority, body, source_path, content_hash, updated_at, enabled, status, scope, storage, source, root_id, stub_path)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           level=excluded.level, always_apply=excluded.always_apply, globs=excluded.globs,
           triggers=excluded.triggers, tags=excluded.tags, priority=excluded.priority,
           body=excluded.body, source_path=excluded.source_path, content_hash=excluded.content_hash,
           updated_at=excluded.updated_at, enabled=excluded.enabled, status=excluded.status,
           scope=excluded.scope, storage=excluded.storage, source=excluded.source,
           root_id=excluded.root_id, stub_path=excluded.stub_path",
    )
    .bind(&fm.id)
    .bind(&fm.level)
    .bind(fm.always_apply as i32)
    .bind(serde_json::to_string(&fm.globs).unwrap_or_else(|_| "[]".into()))
    .bind(serde_json::to_string(&fm.triggers).unwrap_or_else(|_| "[]".into()))
    .bind(serde_json::to_string(&fm.tags).unwrap_or_else(|_| "[]".into()))
    .bind(fm.priority)
    .bind(&doc.body)
    .bind(&doc.source_path)
    .bind(hash)
    .bind(now)
    .bind(fm.enabled as i32)
    .bind(&fm.status)
    .bind(scope)
    .bind(&fm.storage)
    .bind(&fm.source)
    .bind(&fm.root_id)
    .bind(&doc.stub_path)
    .execute(pool)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(())
}

async fn upsert_skill(
    pool: &SqlitePool,
    doc: &PolicySkillDoc,
    hash: &str,
    now: i64,
) -> Result<(), AxError> {
    let fm = &doc.frontmatter;
    let scope = crate::types::PolicyScope::parse(&fm.scope)
        .unwrap_or(crate::types::PolicyScope::Project)
        .as_str();
    sqlx::query(
        "INSERT INTO policy_skills (name, description, always_apply, triggers, tags, priority, context_task, body, source_path, content_hash, updated_at, enabled, status, scope, storage, source, root_id, stub_path)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(name) DO UPDATE SET
           description=excluded.description, always_apply=excluded.always_apply, triggers=excluded.triggers, tags=excluded.tags,
           priority=excluded.priority, context_task=excluded.context_task, body=excluded.body,
           source_path=excluded.source_path, content_hash=excluded.content_hash, updated_at=excluded.updated_at,
           enabled=excluded.enabled, status=excluded.status, scope=excluded.scope,
           storage=excluded.storage, source=excluded.source, root_id=excluded.root_id,
           stub_path=excluded.stub_path",
    )
    .bind(&fm.name)
    .bind(&fm.description)
    .bind(fm.always_apply as i32)
    .bind(serde_json::to_string(&fm.triggers).unwrap_or_else(|_| "[]".into()))
    .bind(serde_json::to_string(&fm.tags).unwrap_or_else(|_| "[]".into()))
    .bind(fm.priority)
    .bind(&fm.context_task)
    .bind(&doc.body)
    .bind(&doc.source_path)
    .bind(hash)
    .bind(now)
    .bind(fm.enabled as i32)
    .bind(&fm.status)
    .bind(scope)
    .bind(&fm.storage)
    .bind(&fm.source)
    .bind(&fm.root_id)
    .bind(&doc.stub_path)
    .execute(pool)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(())
}

/// Delete file-backed rows not on disk; keep `storage = 'database'` overrides.
async fn prune_rules_hybrid(pool: &SqlitePool, keep: &[String]) -> Result<(), AxError> {
    if keep.is_empty() {
        sqlx::query(
            "DELETE FROM policy_rules WHERE COALESCE(NULLIF(storage, ''), 'files') != 'database'",
        )
        .execute(pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        return Ok(());
    }
    let placeholders = keep.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "DELETE FROM policy_rules WHERE id NOT IN ({placeholders}) \
         AND COALESCE(NULLIF(storage, ''), 'files') != 'database'"
    );
    let mut q = sqlx::query(&sql);
    for id in keep {
        q = q.bind(id);
    }
    q.execute(pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(())
}

async fn prune_skills_hybrid(pool: &SqlitePool, keep: &[String]) -> Result<(), AxError> {
    if keep.is_empty() {
        sqlx::query(
            "DELETE FROM policy_skills WHERE COALESCE(NULLIF(storage, ''), 'files') != 'database'",
        )
        .execute(pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        return Ok(());
    }
    let placeholders = keep.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "DELETE FROM policy_skills WHERE name NOT IN ({placeholders}) \
         AND COALESCE(NULLIF(storage, ''), 'files') != 'database'"
    );
    let mut q = sqlx::query(&sql);
    for name in keep {
        q = q.bind(name);
    }
    q.execute(pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(())
}

const RULE_SELECT: &str = "SELECT id, level, always_apply, globs, triggers, tags, priority, body, source_path,
                COALESCE(enabled, 1) as enabled, COALESCE(status, 'approved') as status,
                COALESCE(scope, 'project') as scope, storage, source, root_id, stub_path
         FROM policy_rules";

const SKILL_SELECT: &str = "SELECT name, description, COALESCE(always_apply, 0) as always_apply, triggers, tags, priority, context_task, body, source_path,
                COALESCE(enabled, 1) as enabled, COALESCE(status, 'approved') as status,
                COALESCE(scope, 'project') as scope, storage, source, root_id, stub_path
         FROM policy_skills";

pub async fn list_rules(pool: &SqlitePool) -> Result<Vec<PolicyRuleRow>, AxError> {
    let rows = sqlx::query_as::<_, RuleDbRow>(&format!(
        "{RULE_SELECT} ORDER BY id COLLATE NOCASE ASC"
    ))
    .fetch_all(pool)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(rows.into_iter().map(RuleDbRow::into_row).collect())
}

/// List rules with `effectiveStorage` / override flags filled from project default.
pub async fn list_rules_enriched(
    pool: &SqlitePool,
    project_root: &Path,
) -> Result<Vec<PolicyRuleRow>, AxError> {
    let default = load_policy_config(project_root).storage;
    let mut rows = list_rules(pool).await?;
    for row in &mut rows {
        enrich_rule_row(row, default);
    }
    Ok(rows)
}

pub async fn list_skills(pool: &SqlitePool) -> Result<Vec<PolicySkillRow>, AxError> {
    let rows = sqlx::query_as::<_, SkillDbRow>(&format!(
        "{SKILL_SELECT} ORDER BY name COLLATE NOCASE ASC"
    ))
    .fetch_all(pool)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(rows.into_iter().map(SkillDbRow::into_row).collect())
}

pub async fn list_skills_enriched(
    pool: &SqlitePool,
    project_root: &Path,
) -> Result<Vec<PolicySkillRow>, AxError> {
    let default = load_policy_config(project_root).storage;
    let mut rows = list_skills(pool).await?;
    for row in &mut rows {
        enrich_skill_row(row, default);
    }
    Ok(rows)
}

pub fn enrich_rule_row(row: &mut PolicyRuleRow, default: PolicyStorage) {
    let eff = effective_storage(default, row.storage.as_deref());
    row.effective_storage = eff.as_str().into();
    row.storage_is_override = row.storage.is_some();
}

pub fn enrich_skill_row(row: &mut PolicySkillRow, default: PolicyStorage) {
    let eff = effective_storage(default, row.storage.as_deref());
    row.effective_storage = eff.as_str().into();
    row.storage_is_override = row.storage.is_some();
}

pub async fn get_rule(pool: &SqlitePool, id: &str) -> Result<Option<PolicyRuleRow>, AxError> {
    let row = sqlx::query_as::<_, RuleDbRow>(&format!("{RULE_SELECT} WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(row.map(RuleDbRow::into_row))
}

pub async fn get_skill(pool: &SqlitePool, name: &str) -> Result<Option<PolicySkillRow>, AxError> {
    let row = sqlx::query_as::<_, SkillDbRow>(&format!("{SKILL_SELECT} WHERE name = ?"))
        .bind(name)
        .fetch_optional(pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(row.map(SkillDbRow::into_row))
}

/// Whether policy MCP tools should be listed for this project.
pub fn policy_tools_enabled(project_root: &Path) -> bool {
    if policy_exists_filesystem(project_root) {
        return true;
    }
    match load_policy_config(project_root).storage {
        PolicyStorage::Database => ax_dir_from_project(project_root).join("ax.db").exists(),
        PolicyStorage::Files => false,
    }
}

/// Legacy sync check — filesystem dirs only (files mode).
pub fn policy_exists(project_root: &Path) -> bool {
    policy_tools_enabled(project_root)
}

pub fn policy_exists_filesystem(project_root: &Path) -> bool {
    let ax_dir = ax_dir_from_project(project_root);
    rules_dir(&ax_dir).exists() || skills_dir(&ax_dir).exists()
}

pub async fn policy_has_content(pool: &SqlitePool) -> Result<bool, AxError> {
    let counts = db_counts(pool).await?;
    Ok(counts.rules_indexed > 0 || counts.skills_indexed > 0)
}

pub fn rule_row_to_doc(row: &PolicyRuleRow, project_root: &Path) -> PolicyRuleDoc {
    let share = row.tags.iter().any(|t| t.eq_ignore_ascii_case("shared"));
    let fm = RuleFrontmatter {
        id: row.id.clone(),
        level: row.level.clone(),
        always_apply: row.always_apply,
        globs: row.globs.clone(),
        triggers: row.triggers.clone(),
        tags: row.tags.clone(),
        priority: row.priority,
        enabled: row.enabled,
        status: row.status.clone(),
        share,
        scope: row.scope.clone(),
        storage: row.storage.clone(),
        source: row.source.clone(),
        root_id: row.root_id.clone(),
    };
    let raw = serialize_rule(&fm, &row.body);
    let source = if row.source_path.is_empty() {
        rule_file(&rules_dir(&ax_dir_from_project(project_root)), &row.id)
            .to_string_lossy()
            .to_string()
    } else {
        row.source_path.clone()
    };
    PolicyRuleDoc {
        frontmatter: fm,
        body: row.body.clone(),
        raw,
        source_path: source,
        stub_path: row.stub_path.clone(),
    }
}

pub fn skill_row_to_doc(row: &PolicySkillRow, project_root: &Path) -> PolicySkillDoc {
    let share = row.tags.iter().any(|t| t.eq_ignore_ascii_case("shared"));
    let fm = SkillFrontmatter {
        name: row.name.clone(),
        description: row.description.clone(),
        always_apply: row.always_apply,
        triggers: row.triggers.clone(),
        tags: row.tags.clone(),
        priority: row.priority,
        context_task: row.context_task.clone(),
        enabled: row.enabled,
        status: row.status.clone(),
        share,
        scope: row.scope.clone(),
        storage: row.storage.clone(),
        source: row.source.clone(),
        root_id: row.root_id.clone(),
    };
    let raw = serialize_skill(&fm, &row.body);
    let source = if row.source_path.is_empty() {
        skill_file(&skills_dir(&ax_dir_from_project(project_root)), &row.name)
            .to_string_lossy()
            .to_string()
    } else {
        row.source_path.clone()
    };
    PolicySkillDoc {
        frontmatter: fm,
        body: row.body.clone(),
        raw,
        source_path: source,
        stub_path: row.stub_path.clone(),
    }
}

fn write_utf8(path: &Path, content: &str) -> Result<(), AxError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AxError::Other(e.to_string()))?;
    }
    std::fs::write(path, content.as_bytes()).map_err(|e| AxError::Other(e.to_string()))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(sqlx::FromRow)]
struct RuleDbRow {
    id: String,
    level: String,
    always_apply: i32,
    globs: String,
    triggers: String,
    tags: String,
    priority: i32,
    body: String,
    source_path: String,
    enabled: i32,
    status: String,
    scope: String,
    storage: Option<String>,
    source: Option<String>,
    root_id: Option<String>,
    stub_path: Option<String>,
}

impl RuleDbRow {
    fn into_row(self) -> PolicyRuleRow {
        let storage = self.storage.filter(|s| !s.is_empty());
        PolicyRuleRow {
            id: self.id,
            level: self.level,
            always_apply: self.always_apply != 0,
            globs: parse_json_array(&self.globs),
            triggers: parse_json_array(&self.triggers),
            tags: parse_json_array(&self.tags),
            priority: self.priority,
            body: self.body,
            source_path: self.source_path,
            enabled: self.enabled != 0,
            status: if self.status.is_empty() {
                "approved".into()
            } else {
                self.status
            },
            scope: if self.scope.is_empty() {
                "project".into()
            } else {
                self.scope
            },
            storage,
            source: self.source.filter(|s| !s.is_empty()),
            root_id: self.root_id.filter(|s| !s.is_empty()),
            stub_path: self.stub_path.filter(|s| !s.is_empty()),
            effective_storage: String::new(),
            storage_is_override: false,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SkillDbRow {
    name: String,
    description: String,
    always_apply: i32,
    triggers: String,
    tags: String,
    priority: i32,
    context_task: Option<String>,
    body: String,
    source_path: String,
    enabled: i32,
    status: String,
    scope: String,
    storage: Option<String>,
    source: Option<String>,
    root_id: Option<String>,
    stub_path: Option<String>,
}

impl SkillDbRow {
    fn into_row(self) -> PolicySkillRow {
        let storage = self.storage.filter(|s| !s.is_empty());
        PolicySkillRow {
            name: self.name,
            description: self.description,
            always_apply: self.always_apply != 0,
            triggers: parse_json_array(&self.triggers),
            tags: parse_json_array(&self.tags),
            priority: self.priority,
            context_task: self.context_task,
            body: self.body,
            source_path: self.source_path,
            enabled: self.enabled != 0,
            status: if self.status.is_empty() {
                "approved".into()
            } else {
                self.status
            },
            scope: if self.scope.is_empty() {
                "project".into()
            } else {
                self.scope
            },
            storage,
            source: self.source.filter(|s| !s.is_empty()),
            root_id: self.root_id.filter(|s| !s.is_empty()),
            stub_path: self.stub_path.filter(|s| !s.is_empty()),
            effective_storage: String::new(),
            storage_is_override: false,
        }
    }
}

fn parse_json_array(s: &str) -> Vec<String> {
    serde_json::from_str(s).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RuleFrontmatter;
    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::TempDir;

    async fn test_pool() -> (TempDir, SqlitePool) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .connect_with(opts)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS policy_rules (
                id TEXT PRIMARY KEY, level TEXT NOT NULL, always_apply INTEGER NOT NULL DEFAULT 0,
                globs TEXT NOT NULL DEFAULT '[]', triggers TEXT NOT NULL DEFAULT '[]',
                tags TEXT NOT NULL DEFAULT '[]', priority INTEGER NOT NULL DEFAULT 50,
                body TEXT NOT NULL, source_path TEXT NOT NULL, content_hash TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                status TEXT NOT NULL DEFAULT 'approved',
                scope TEXT NOT NULL DEFAULT 'project',
                storage TEXT, source TEXT, root_id TEXT, stub_path TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS policy_skills (
                name TEXT PRIMARY KEY, description TEXT NOT NULL,
                always_apply INTEGER NOT NULL DEFAULT 0,
                triggers TEXT NOT NULL DEFAULT '[]', tags TEXT NOT NULL DEFAULT '[]',
                priority INTEGER NOT NULL DEFAULT 50, context_task TEXT,
                body TEXT NOT NULL, source_path TEXT NOT NULL, content_hash TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                status TEXT NOT NULL DEFAULT 'approved',
                scope TEXT NOT NULL DEFAULT 'project',
                storage TEXT, source TEXT, root_id TEXT, stub_path TEXT
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        (dir, pool)
    }

    #[tokio::test]
    async fn database_mode_save_without_files() {
        let (_dir, pool) = test_pool().await;
        let root = _dir.path();
        std::fs::write(
            root.join("ax.json"),
            r#"{"policy":{"storage":"database"}}"#,
        )
        .unwrap();

        let fm = RuleFrontmatter {
            id: "test-rule".into(),
            level: "CRITICAL".into(),
            always_apply: true,
            globs: vec![],
            triggers: vec![],
            tags: vec![],
            priority: 50,
            enabled: true,
            status: "approved".into(),
            share: false,
            scope: "project".into(),
            storage: Some("database".into()),
            source: None,
            root_id: None,
        };
        let raw = serialize_rule(&fm, "body text");
        let doc = parse_rule_file(Path::new("test-rule.mdc"), &raw).unwrap();
        upsert_rule_doc(&pool, &doc).await.unwrap();

        let rules = list_rules(&pool).await.unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "test-rule");

        // index without force should not wipe DB-only rows
        let result = index_policy(&pool, root, false).await.unwrap();
        assert_eq!(result.rules_indexed, 1);
    }

    #[tokio::test]
    async fn ensure_policy_ready_imports_when_db_empty() {
        let (_dir, pool) = test_pool().await;
        let root = _dir.path();
        std::fs::write(
            root.join("ax.json"),
            r#"{"policy":{"storage":"database"}}"#,
        )
        .unwrap();
        let ax_dir = root.join(".ax");
        let rules_path = rules_dir(&ax_dir);
        std::fs::create_dir_all(&rules_path).unwrap();
        std::fs::write(
            rule_file(&rules_path, "disk-rule"),
            "---\nid: disk-rule\nlevel: CRITICAL\nalwaysApply: true\n---\nfrom disk\n",
        )
        .unwrap();

        let counts = ensure_policy_ready(&pool, root).await.unwrap();
        assert!(counts.rules_indexed >= 1);
        let rules = list_rules(&pool).await.unwrap();
        assert!(rules.iter().any(|r| r.id == "disk-rule"));
    }

    #[tokio::test]
    async fn skill_always_apply_persists() {
        let (_dir, pool) = test_pool().await;
        let raw = "---\nname: old-coder\ndescription: evidence\nalwaysApply: true\n---\n\nbody\n";
        let doc = parse_skill_file(Path::new("SKILL.md"), raw).unwrap();
        upsert_skill_doc(&pool, &doc).await.unwrap();
        let skills = list_skills(&pool).await.unwrap();
        assert_eq!(skills.len(), 1);
        assert!(skills[0].always_apply);
        assert_eq!(skills[0].name, "old-coder");
    }
}
