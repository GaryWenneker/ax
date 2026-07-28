use std::path::Path;

use ax_utils::errors::AxError;

use crate::types::ValidationError;

use crate::config::{load_policy_config, PolicyStorage};
use crate::hierarchy::{ensure_scope_dirs, policy_dir_for_scope};
use crate::index::{
    delete_rule_by_id, delete_skill_by_name, export_policy_to_files, import_policy_from_files,
    rule_row_to_doc, skill_row_to_doc, upsert_rule_doc, upsert_skill_doc, ImportMode,
};
use crate::parse::{parse_rule_file, parse_skill_file, serialize_rule, serialize_skill};
use crate::paths::{rule_file, skill_file};
use crate::types::{
    PolicyRuleDoc, PolicyScope, PolicySkillDoc, RuleFrontmatter, SkillFrontmatter,
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
        if let Some(row) = crate::index::get_rule(&self.pool, id).await? {
            return Ok(Some(rule_row_to_doc(&row, &self.project_root)));
        }
        if self.storage == PolicyStorage::Database {
            return Ok(None);
        }
        // Fallback: search layer dirs by filename.
        for layer in crate::hierarchy::policy_layers(&self.project_root) {
            let path = rule_file(&layer.dir.join("rules"), id);
            if path.is_file() {
                let raw =
                    std::fs::read_to_string(&path).map_err(|e| AxError::Other(e.to_string()))?;
                let mut doc = parse_rule_file(&path, &raw).map_err(|e| AxError::Other(e.error))?;
                doc.frontmatter.scope = layer.scope.as_str().into();
                return Ok(Some(doc));
            }
        }
        Ok(None)
    }

    pub async fn get_skill_doc(&self, name: &str) -> Result<Option<PolicySkillDoc>, AxError> {
        if let Some(row) = crate::index::get_skill(&self.pool, name).await? {
            return Ok(Some(skill_row_to_doc(&row, &self.project_root)));
        }
        if self.storage == PolicyStorage::Database {
            return Ok(None);
        }
        for layer in crate::hierarchy::policy_layers(&self.project_root) {
            let path = skill_file(&layer.dir.join("skills"), name);
            if path.is_file() {
                let raw =
                    std::fs::read_to_string(&path).map_err(|e| AxError::Other(e.to_string()))?;
                let mut doc = parse_skill_file(&path, &raw).map_err(|e| AxError::Other(e.error))?;
                doc.frontmatter.scope = layer.scope.as_str().into();
                return Ok(Some(doc));
            }
        }
        Ok(None)
    }

    pub async fn save_rule(
        &self,
        mut frontmatter: RuleFrontmatter,
        body: String,
    ) -> Result<PolicyRuleDoc, ValidationError> {
        let scope = PolicyScope::parse(&frontmatter.scope).unwrap_or(PolicyScope::Project);
        frontmatter.scope = scope.as_str().into();

        let policy_dir = if self.storage == PolicyStorage::Files {
            ensure_scope_dirs(&self.project_root, scope).map_err(|e| ValidationError {
                error: e.to_string(),
                fields: Default::default(),
            })?
        } else {
            policy_dir_for_scope(&self.project_root, scope)
        };

        let path = rule_file(&policy_dir.join("rules"), &frontmatter.id);
        let raw = serialize_rule(&frontmatter, &body);
        let doc = parse_rule_file(&path, &raw)?;

        if self.storage == PolicyStorage::Database {
            upsert_rule_doc(&self.pool, &doc)
                .await
                .map_err(|e| ValidationError {
                    error: e.to_string(),
                    fields: Default::default(),
                })?;
            return Ok(doc);
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ValidationError {
                error: e.to_string(),
                fields: Default::default(),
            })?;
        }
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
        mut frontmatter: SkillFrontmatter,
        body: String,
    ) -> Result<PolicySkillDoc, ValidationError> {
        let scope = PolicyScope::parse(&frontmatter.scope).unwrap_or(PolicyScope::Project);
        frontmatter.scope = scope.as_str().into();

        let policy_dir = if self.storage == PolicyStorage::Files {
            ensure_scope_dirs(&self.project_root, scope).map_err(|e| ValidationError {
                error: e.to_string(),
                fields: Default::default(),
            })?
        } else {
            policy_dir_for_scope(&self.project_root, scope)
        };

        let skills = policy_dir.join("skills");
        let path = skill_file(&skills, &frontmatter.name);
        let raw = serialize_skill(&frontmatter, &body);
        let doc = parse_skill_file(&path, &raw)?;

        if self.storage == PolicyStorage::Database {
            upsert_skill_doc(&self.pool, &doc)
                .await
                .map_err(|e| ValidationError {
                    error: e.to_string(),
                    fields: Default::default(),
                })?;
            return Ok(doc);
        }

        std::fs::create_dir_all(skills.join(&frontmatter.name)).map_err(|e| ValidationError {
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
        if let Some(doc) = self.get_rule_doc(id).await? {
            let scope = PolicyScope::parse(&doc.frontmatter.scope).unwrap_or(PolicyScope::Project);
            let path = rule_file(
                &policy_dir_for_scope(&self.project_root, scope).join("rules"),
                id,
            );
            if path.is_file() {
                std::fs::remove_file(&path).map_err(|e| AxError::Other(e.to_string()))?;
                crate::index::index_policy(&self.pool, &self.project_root, false).await?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub async fn delete_skill(&self, name: &str) -> Result<bool, AxError> {
        if self.storage == PolicyStorage::Database {
            return delete_skill_by_name(&self.pool, name).await;
        }
        if let Some(doc) = self.get_skill_doc(name).await? {
            let scope = PolicyScope::parse(&doc.frontmatter.scope).unwrap_or(PolicyScope::Project);
            let skills = policy_dir_for_scope(&self.project_root, scope).join("skills");
            let path = skill_file(&skills, name);
            let dir = skills.join(name);
            if path.is_file() {
                std::fs::remove_file(&path).map_err(|e| AxError::Other(e.to_string()))?;
                if dir.is_dir() {
                    let _ = std::fs::remove_dir(&dir);
                }
                crate::index::index_policy(&self.pool, &self.project_root, false).await?;
                return Ok(true);
            }
        }
        Ok(false)
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

    /// Enable or disable a rule or skill by id/name (writes frontmatter + DB).
    pub async fn set_enabled(&self, id_or_name: &str, enabled: bool) -> Result<bool, AxError> {
        if let Some(mut doc) = self.get_rule_doc(id_or_name).await? {
            doc.frontmatter.enabled = enabled;
            self.save_rule(doc.frontmatter, doc.body)
                .await
                .map_err(|e| AxError::Other(e.error))?;
            return Ok(true);
        }
        if let Some(mut doc) = self.get_skill_doc(id_or_name).await? {
            doc.frontmatter.enabled = enabled;
            self.save_skill(doc.frontmatter, doc.body)
                .await
                .map_err(|e| AxError::Other(e.error))?;
            return Ok(true);
        }
        Ok(false)
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
