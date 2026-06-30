use std::path::Path;

use ax_utils::errors::{AxError, DatabaseError};

use crate::index::index_policy;
use crate::parse::{parse_rule_file, parse_skill_file, serialize_rule, serialize_skill};
use crate::paths::{ax_dir_from_project, ensure_scaffold, rule_file, rules_dir, skill_file, skills_dir};
use crate::types::{
    PolicyRuleDoc, PolicyRuleRow, PolicySkillDoc, PolicySkillRow, RuleFrontmatter, SkillFrontmatter,
    ValidationError,
};

pub struct PolicyStore {
    pool: sqlx::SqlitePool,
    project_root: std::path::PathBuf,
}

impl PolicyStore {
    pub fn new(pool: sqlx::SqlitePool, project_root: std::path::PathBuf) -> Self {
        Self { pool, project_root }
    }

    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    pub async fn reindex(&self, force: bool) -> Result<crate::types::PolicyIndexResult, AxError> {
        index_policy(&self.pool, &self.project_root, force).await
    }

    pub async fn list_rules(&self) -> Result<Vec<PolicyRuleRow>, AxError> {
        crate::index::list_rules(&self.pool).await
    }

    pub async fn list_skills(&self) -> Result<Vec<PolicySkillRow>, AxError> {
        crate::index::list_skills(&self.pool).await
    }

    pub async fn get_rule_doc(&self, id: &str) -> Result<Option<PolicyRuleDoc>, AxError> {
        let ax_dir = ax_dir_from_project(&self.project_root);
        let path = rule_file(&rules_dir(&ax_dir), id);
        if !path.is_file() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&path).map_err(|e| AxError::Other(e.to_string()))?;
        let doc = parse_rule_file(&path, &raw).map_err(|e| AxError::Other(e.error))?;
        Ok(Some(doc))
    }

    pub async fn get_skill_doc(&self, name: &str) -> Result<Option<PolicySkillDoc>, AxError> {
        let ax_dir = ax_dir_from_project(&self.project_root);
        let path = skill_file(&skills_dir(&ax_dir), name);
        if !path.is_file() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&path).map_err(|e| AxError::Other(e.to_string()))?;
        let doc = parse_skill_file(&path, &raw).map_err(|e| AxError::Other(e.error))?;
        Ok(Some(doc))
    }

    pub async fn save_rule(
        &self,
        frontmatter: RuleFrontmatter,
        body: String,
    ) -> Result<PolicyRuleDoc, ValidationError> {
        let ax_dir = ax_dir_from_project(&self.project_root);
        ensure_scaffold(&ax_dir).map_err(|e| ValidationError {
            error: e.to_string(),
            fields: Default::default(),
        })?;
        let path = rule_file(&rules_dir(&ax_dir), &frontmatter.id);
        let raw = serialize_rule(&frontmatter, &body);
        parse_rule_file(&path, &raw)?;
        write_utf8(&path, &raw).map_err(|e| ValidationError {
            error: e,
            fields: Default::default(),
        })?;
        index_policy(&self.pool, &self.project_root, false)
            .await
            .map_err(|e| ValidationError {
                error: e.to_string(),
                fields: Default::default(),
            })?;
        parse_rule_file(&path, &raw)
    }

    pub async fn save_skill(
        &self,
        frontmatter: SkillFrontmatter,
        body: String,
    ) -> Result<PolicySkillDoc, ValidationError> {
        let ax_dir = ax_dir_from_project(&self.project_root);
        ensure_scaffold(&ax_dir).map_err(|e| ValidationError {
            error: e.to_string(),
            fields: Default::default(),
        })?;
        let dir = skills_dir(&ax_dir).join(&frontmatter.name);
        std::fs::create_dir_all(&dir).map_err(|e| ValidationError {
            error: e.to_string(),
            fields: Default::default(),
        })?;
        let path = skill_file(&skills_dir(&ax_dir), &frontmatter.name);
        let raw = serialize_skill(&frontmatter, &body);
        parse_skill_file(&path, &raw)?;
        write_utf8(&path, &raw).map_err(|e| ValidationError {
            error: e,
            fields: Default::default(),
        })?;
        index_policy(&self.pool, &self.project_root, false)
            .await
            .map_err(|e| ValidationError {
                error: e.to_string(),
                fields: Default::default(),
            })?;
        parse_skill_file(&path, &raw)
    }

    pub async fn delete_rule(&self, id: &str) -> Result<bool, AxError> {
        let ax_dir = ax_dir_from_project(&self.project_root);
        let path = rule_file(&rules_dir(&ax_dir), id);
        if !path.is_file() {
            return Ok(false);
        }
        std::fs::remove_file(&path).map_err(|e| AxError::Other(e.to_string()))?;
        index_policy(&self.pool, &self.project_root, false).await?;
        Ok(true)
    }

    pub async fn delete_skill(&self, name: &str) -> Result<bool, AxError> {
        let ax_dir = ax_dir_from_project(&self.project_root);
        let dir = skills_dir(&ax_dir).join(name);
        let path = skill_file(&skills_dir(&ax_dir), name);
        if !path.is_file() {
            return Ok(false);
        }
        std::fs::remove_file(&path).map_err(|e| AxError::Other(e.to_string()))?;
        if dir.is_dir() {
            let _ = std::fs::remove_dir(&dir);
        }
        index_policy(&self.pool, &self.project_root, false).await?;
        Ok(true)
    }
}

fn write_utf8(path: &Path, content: &str) -> Result<(), String> {
    std::fs::write(path, content.as_bytes()).map_err(|e| e.to_string())
}

pub async fn open_rw_pool(db_path: &Path) -> Result<sqlx::SqlitePool, AxError> {
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
    use sqlx::ConnectOptions;

    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(false)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .disable_statement_logging();

    SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(opts)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))
}
