use std::path::Path;

use ax_utils::errors::AxError;

use crate::types::ValidationError;

use crate::config::{load_policy_config, PolicyStorage};
use crate::index::{
    delete_rule_by_id, delete_skill_by_name, export_policy_to_files, import_policy_from_files,
    rule_row_to_doc, skill_row_to_doc, upsert_rule_doc, upsert_skill_doc, ImportMode,
};
use crate::parse::{parse_rule_file, parse_skill_file, serialize_rule, serialize_skill};
use crate::paths::{ensure_scaffold, rule_file, rules_dir, skill_file, skills_dir, ax_dir_from_project};
use crate::types::{
    PolicyRuleDoc, PolicySkillDoc, RuleFrontmatter, SkillFrontmatter,
};

pub struct PolicyStore {
    pool: sqlx::SqlitePool,
    project_root: std::path::PathBuf,
    storage: PolicyStorage,
}

impl PolicyStore {
    pub fn new(pool: sqlx::SqlitePool, project_root: std::path::PathBuf) -> Self {
        let storage = load_policy_config(&project_root).storage;
        Self {
            pool,
            project_root,
            storage,
        }
    }

    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    pub fn storage(&self) -> PolicyStorage {
        self.storage
    }

    pub async fn reindex(&self, force: bool) -> Result<crate::types::PolicyIndexResult, AxError> {
        crate::index::index_policy(&self.pool, &self.project_root, force).await
    }

    pub async fn list_rules(&self) -> Result<Vec<crate::types::PolicyRuleRow>, AxError> {
        crate::index::list_rules(&self.pool).await
    }

    pub async fn list_skills(&self) -> Result<Vec<crate::types::PolicySkillRow>, AxError> {
        crate::index::list_skills(&self.pool).await
    }

    pub async fn get_rule_doc(&self, id: &str) -> Result<Option<PolicyRuleDoc>, AxError> {
        if self.storage == PolicyStorage::Database {
            let row = crate::index::get_rule(&self.pool, id).await?;
            return Ok(row.map(|r| rule_row_to_doc(&r, &self.project_root)));
        }
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
        if self.storage == PolicyStorage::Database {
            let row = crate::index::get_skill(&self.pool, name).await?;
            return Ok(row.map(|r| skill_row_to_doc(&r, &self.project_root)));
        }
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
        let path = rule_file(
            &rules_dir(&ax_dir_from_project(&self.project_root)),
            &frontmatter.id,
        );
        let raw = serialize_rule(&frontmatter, &body);
        let doc = parse_rule_file(&path, &raw)?;

        if self.storage == PolicyStorage::Database {
            upsert_rule_doc(&self.pool, &doc).await.map_err(|e| ValidationError {
                error: e.to_string(),
                fields: Default::default(),
            })?;
            return Ok(doc);
        }

        ensure_scaffold(&ax_dir_from_project(&self.project_root)).map_err(|e| ValidationError {
            error: e.to_string(),
            fields: Default::default(),
        })?;
        write_utf8(&path, &raw).map_err(|e| ValidationError {
            error: e,
            fields: Default::default(),
        })?;
        crate::index::index_policy(&self.pool, &self.project_root, false)
            .await
            .map_err(|e| ValidationError {
                error: e.to_string(),
                fields: Default::default(),
            })?;
        Ok(doc)
    }

    pub async fn save_skill(
        &self,
        frontmatter: SkillFrontmatter,
        body: String,
    ) -> Result<PolicySkillDoc, ValidationError> {
        let path = skill_file(
            &skills_dir(&ax_dir_from_project(&self.project_root)),
            &frontmatter.name,
        );
        let raw = serialize_skill(&frontmatter, &body);
        let doc = parse_skill_file(&path, &raw)?;

        if self.storage == PolicyStorage::Database {
            upsert_skill_doc(&self.pool, &doc).await.map_err(|e| ValidationError {
                error: e.to_string(),
                fields: Default::default(),
            })?;
            return Ok(doc);
        }

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
        write_utf8(&path, &raw).map_err(|e| ValidationError {
            error: e,
            fields: Default::default(),
        })?;
        crate::index::index_policy(&self.pool, &self.project_root, false)
            .await
            .map_err(|e| ValidationError {
                error: e.to_string(),
                fields: Default::default(),
            })?;
        Ok(doc)
    }

    pub async fn delete_rule(&self, id: &str) -> Result<bool, AxError> {
        if self.storage == PolicyStorage::Database {
            return delete_rule_by_id(&self.pool, id).await;
        }
        let ax_dir = ax_dir_from_project(&self.project_root);
        let path = rule_file(&rules_dir(&ax_dir), id);
        if !path.is_file() {
            return Ok(false);
        }
        std::fs::remove_file(&path).map_err(|e| AxError::Other(e.to_string()))?;
        crate::index::index_policy(&self.pool, &self.project_root, false).await?;
        Ok(true)
    }

    pub async fn delete_skill(&self, name: &str) -> Result<bool, AxError> {
        if self.storage == PolicyStorage::Database {
            return delete_skill_by_name(&self.pool, name).await;
        }
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
        crate::index::index_policy(&self.pool, &self.project_root, false).await?;
        Ok(true)
    }

    pub async fn import_from_files(&self) -> Result<crate::types::PolicyIndexResult, AxError> {
        import_policy_from_files(&self.pool, &self.project_root, ImportMode::Merge).await
    }

    pub async fn export_to_files(
        &self,
        out_dir: &Path,
    ) -> Result<crate::index::ExportResult, AxError> {
        export_policy_to_files(&self.pool, &self.project_root, out_dir).await
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
        .map_err(|e| AxError::Database(ax_utils::errors::DatabaseError::new(e.to_string())))
}
