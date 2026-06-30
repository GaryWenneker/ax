use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

use crate::parse::{parse_rule_file, parse_skill_file};
use crate::paths::{ensure_scaffold, rules_dir, skill_file, skills_dir, ax_dir_from_project};
use crate::types::{PolicyIndexResult, PolicyRuleRow, PolicySkillRow};

pub async fn index_policy(
    pool: &SqlitePool,
    project_root: &std::path::Path,
    _force: bool,
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
            let raw = std::fs::read_to_string(path)
                .map_err(|e| AxError::Other(e.to_string()))?;
            let doc = parse_rule_file(path, &raw).map_err(|e| AxError::Other(e.error))?;
            let hash = blake3::hash(raw.as_bytes()).to_hex().to_string();
            let now = now_ms();
            upsert_rule(pool, &doc, &hash, now).await?;
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
            let raw = std::fs::read_to_string(&skill_path)
                .map_err(|e| AxError::Other(e.to_string()))?;
            let doc = parse_skill_file(&skill_path, &raw).map_err(|e| AxError::Other(e.error))?;
            let hash = blake3::hash(raw.as_bytes()).to_hex().to_string();
            let now = now_ms();
            upsert_skill(pool, &doc, &hash, now).await?;
            seen_skills.push(doc.frontmatter.name.clone());
            skills_indexed += 1;
        }
    }

    prune_rules(pool, &seen_rules).await?;
    prune_skills(pool, &seen_skills).await?;

    Ok(PolicyIndexResult {
        rules_indexed,
        skills_indexed,
    })
}

async fn upsert_rule(
    pool: &SqlitePool,
    doc: &crate::types::PolicyRuleDoc,
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
    doc: &crate::types::PolicySkillDoc,
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

pub fn policy_exists(project_root: &std::path::Path) -> bool {
    let ax_dir = ax_dir_from_project(project_root);
    rules_dir(&ax_dir).exists() || skills_dir(&ax_dir).exists()
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
