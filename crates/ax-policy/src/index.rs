use sqlx::SqlitePool;
use std::path::Path;

use ax_utils::errors::{AxError, DatabaseError};

use crate::config::{load_policy_config, PolicyStorage};
use crate::parse::{parse_rule_file, parse_skill_file, serialize_rule, serialize_skill};
use crate::paths::{ensure_scaffold, rule_file, rules_dir, skill_file, skills_dir, ax_dir_from_project};
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
    ensure_scaffold(&ax_dir).map_err(|e| AxError::Other(e.to_string()))?;

    let mut rules_indexed = 0u32;
    let mut skills_indexed = 0u32;
    let mut seen_rules = Vec::new();
    let mut seen_skills = Vec::new();

    let rules_path = rules_dir(&ax_dir);
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
            let doc = parse_rule_file(path, &raw).map_err(|e| AxError::Other(e.error))?;
            let hash = blake3::hash(raw.as_bytes()).to_hex().to_string();
            upsert_rule(pool, &doc, &hash, now_ms()).await?;
            seen_rules.push(doc.frontmatter.id.clone());
            rules_indexed += 1;
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
            upsert_skill(pool, &doc, &hash, now_ms()).await?;
            seen_skills.push(doc.frontmatter.name.clone());
            skills_indexed += 1;
        }
    }

    if mode == ImportMode::Replace {
        prune_rules(pool, &seen_rules).await?;
        prune_skills(pool, &seen_skills).await?;
    }

    Ok(PolicyIndexResult {
        rules_indexed,
        skills_indexed,
    })
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

async fn upsert_rule(
    pool: &SqlitePool,
    doc: &PolicyRuleDoc,
    hash: &str,
    now: i64,
) -> Result<(), AxError> {
    let fm = &doc.frontmatter;
    sqlx::query(
        "INSERT INTO policy_rules (id, level, always_apply, globs, triggers, tags, priority, body, source_path, content_hash, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           level=excluded.level, always_apply=excluded.always_apply, globs=excluded.globs,
           triggers=excluded.triggers, tags=excluded.tags, priority=excluded.priority,
           body=excluded.body, source_path=excluded.source_path, content_hash=excluded.content_hash,
           updated_at=excluded.updated_at",
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
    sqlx::query(
        "INSERT INTO policy_skills (name, description, triggers, tags, priority, context_task, body, source_path, content_hash, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(name) DO UPDATE SET
           description=excluded.description, triggers=excluded.triggers, tags=excluded.tags,
           priority=excluded.priority, context_task=excluded.context_task, body=excluded.body,
           source_path=excluded.source_path, content_hash=excluded.content_hash, updated_at=excluded.updated_at",
    )
    .bind(&fm.name)
    .bind(&fm.description)
    .bind(serde_json::to_string(&fm.triggers).unwrap_or_else(|_| "[]".into()))
    .bind(serde_json::to_string(&fm.tags).unwrap_or_else(|_| "[]".into()))
    .bind(fm.priority)
    .bind(&fm.context_task)
    .bind(&doc.body)
    .bind(&doc.source_path)
    .bind(hash)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(())
}

async fn prune_rules(pool: &SqlitePool, keep: &[String]) -> Result<(), AxError> {
    if keep.is_empty() {
        sqlx::query("DELETE FROM policy_rules")
            .execute(pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        return Ok(());
    }
    let placeholders = keep.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("DELETE FROM policy_rules WHERE id NOT IN ({placeholders})");
    let mut q = sqlx::query(&sql);
    for id in keep {
        q = q.bind(id);
    }
    q.execute(pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(())
}

async fn prune_skills(pool: &SqlitePool, keep: &[String]) -> Result<(), AxError> {
    if keep.is_empty() {
        sqlx::query("DELETE FROM policy_skills")
            .execute(pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
        return Ok(());
    }
    let placeholders = keep.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!("DELETE FROM policy_skills WHERE name NOT IN ({placeholders})");
    let mut q = sqlx::query(&sql);
    for name in keep {
        q = q.bind(name);
    }
    q.execute(pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(())
}

pub async fn list_rules(pool: &SqlitePool) -> Result<Vec<PolicyRuleRow>, AxError> {
    let rows = sqlx::query_as::<_, RuleDbRow>(
        "SELECT id, level, always_apply, globs, triggers, tags, priority, body, source_path FROM policy_rules ORDER BY priority DESC, id",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(rows.into_iter().map(RuleDbRow::into_row).collect())
}

pub async fn list_skills(pool: &SqlitePool) -> Result<Vec<PolicySkillRow>, AxError> {
    let rows = sqlx::query_as::<_, SkillDbRow>(
        "SELECT name, description, triggers, tags, priority, context_task, body, source_path FROM policy_skills ORDER BY priority DESC, name",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(rows.into_iter().map(SkillDbRow::into_row).collect())
}

pub async fn get_rule(pool: &SqlitePool, id: &str) -> Result<Option<PolicyRuleRow>, AxError> {
    let row = sqlx::query_as::<_, RuleDbRow>(
        "SELECT id, level, always_apply, globs, triggers, tags, priority, body, source_path FROM policy_rules WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(row.map(RuleDbRow::into_row))
}

pub async fn get_skill(pool: &SqlitePool, name: &str) -> Result<Option<PolicySkillRow>, AxError> {
    let row = sqlx::query_as::<_, SkillDbRow>(
        "SELECT name, description, triggers, tags, priority, context_task, body, source_path FROM policy_skills WHERE name = ?",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(row.map(SkillDbRow::into_row))
}

/// Whether policy MCP tools should be listed for this project.
pub fn policy_tools_enabled(project_root: &Path) -> bool {
    let config = load_policy_config(project_root);
    match config.storage {
        PolicyStorage::Database => true,
        PolicyStorage::Files => policy_exists_filesystem(project_root),
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
    let fm = RuleFrontmatter {
        id: row.id.clone(),
        level: row.level.clone(),
        always_apply: row.always_apply,
        globs: row.globs.clone(),
        triggers: row.triggers.clone(),
        tags: row.tags.clone(),
        priority: row.priority,
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
    }
}

pub fn skill_row_to_doc(row: &PolicySkillRow, project_root: &Path) -> PolicySkillDoc {
    let fm = SkillFrontmatter {
        name: row.name.clone(),
        description: row.description.clone(),
        triggers: row.triggers.clone(),
        tags: row.tags.clone(),
        priority: row.priority,
        context_task: row.context_task.clone(),
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
}

impl RuleDbRow {
    fn into_row(self) -> PolicyRuleRow {
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
        }
    }
}

#[derive(sqlx::FromRow)]
struct SkillDbRow {
    name: String,
    description: String,
    triggers: String,
    tags: String,
    priority: i32,
    context_task: Option<String>,
    body: String,
    source_path: String,
}

impl SkillDbRow {
    fn into_row(self) -> PolicySkillRow {
        PolicySkillRow {
            name: self.name,
            description: self.description,
            triggers: parse_json_array(&self.triggers),
            tags: parse_json_array(&self.tags),
            priority: self.priority,
            context_task: self.context_task,
            body: self.body,
            source_path: self.source_path,
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
                updated_at INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS policy_skills (
                name TEXT PRIMARY KEY, description TEXT NOT NULL,
                triggers TEXT NOT NULL DEFAULT '[]', tags TEXT NOT NULL DEFAULT '[]',
                priority INTEGER NOT NULL DEFAULT 50, context_task TEXT,
                body TEXT NOT NULL, source_path TEXT NOT NULL, content_hash TEXT NOT NULL,
                updated_at INTEGER NOT NULL
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
}
