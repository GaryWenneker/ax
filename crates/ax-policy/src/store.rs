use std::path::{Path, PathBuf};

use ax_utils::errors::AxError;

use crate::types::ValidationError;

use crate::config::{effective_storage, load_policy_config, PolicyStorage};
use crate::hierarchy::policy_dir_for_scope;
use crate::index::{
    delete_rule_by_id, delete_skill_by_name, export_policy_to_files, import_policy_from_files,
    list_rules_enriched, list_skills_enriched, rule_row_to_doc, skill_row_to_doc, upsert_rule_doc,
    upsert_skill_doc, ImportMode,
};
use crate::parse::{
    parse_rule_file, parse_skill_file, serialize_rule, serialize_rule_stub, serialize_skill,
    serialize_skill_stub,
};
use crate::paths::{resolve_item_write_dir, resolve_source_path, rule_file, skill_file};
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

    /// Project-level default storage (from ax.json).
    pub fn storage(&self) -> PolicyStorage {
        self.storage
    }

    /// Reload project default from disk (after settings change).
    pub fn reload_storage(&mut self) {
        self.storage = load_policy_config(&self.project_root).storage;
    }

    pub fn item_effective_storage(&self, item_storage: Option<&str>) -> PolicyStorage {
        effective_storage(self.storage, item_storage)
    }

    pub async fn reindex(&self, force: bool) -> Result<crate::types::PolicyIndexResult, AxError> {
        crate::index::index_policy(&self.pool, &self.project_root, force).await
    }

    pub async fn list_rules(&self) -> Result<Vec<crate::types::PolicyRuleRow>, AxError> {
        list_rules_enriched(&self.pool, &self.project_root).await
    }

    pub async fn list_skills(&self) -> Result<Vec<crate::types::PolicySkillRow>, AxError> {
        list_skills_enriched(&self.pool, &self.project_root).await
    }

    pub async fn get_rule_doc(&self, id: &str) -> Result<Option<PolicyRuleDoc>, AxError> {
        if let Some(row) = crate::index::get_rule(&self.pool, id).await? {
            return Ok(Some(rule_row_to_doc(&row, &self.project_root)));
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
        if let Some(ref s) = frontmatter.storage {
            if PolicyStorage::parse(s).is_none() {
                return Err(ValidationError {
                    error: "validation_failed".into(),
                    fields: [("storage".into(), "must be files or database".into())]
                        .into_iter()
                        .collect(),
                });
            }
        }

        let eff = self.item_effective_storage(frontmatter.storage.as_deref());
        let existing = self.get_rule_doc(&frontmatter.id).await.ok().flatten();

        if eff == PolicyStorage::Database {
            let path_hint = existing
                .as_ref()
                .map(|d| d.source_path.clone())
                .unwrap_or_else(|| {
                    rule_file(
                        &policy_dir_for_scope(&self.project_root, scope).join("rules"),
                        &frontmatter.id,
                    )
                    .to_string_lossy()
                    .into()
                });
            let raw = serialize_rule(&frontmatter, &body);
            let mut doc = parse_rule_file(Path::new(&path_hint), &raw)?;
            doc.stub_path = None;
            upsert_rule_doc(&self.pool, &doc)
                .await
                .map_err(|e| ValidationError {
                    error: e.to_string(),
                    fields: Default::default(),
                })?;
            return Ok(doc);
        }

        // Files mode (effective)
        let (doc, stub_path) = self.write_rule_files(frontmatter, body, existing.as_ref())?;
        let mut doc = doc;
        doc.stub_path = stub_path;
        upsert_rule_doc(&self.pool, &doc)
            .await
            .map_err(|e| ValidationError {
                error: e.to_string(),
                fields: Default::default(),
            })?;
        Ok(doc)
    }

    fn write_rule_files(
        &self,
        frontmatter: RuleFrontmatter,
        body: String,
        existing: Option<&PolicyRuleDoc>,
    ) -> Result<(PolicyRuleDoc, Option<String>), ValidationError> {
        let scope = PolicyScope::parse(&frontmatter.scope).unwrap_or(PolicyScope::Project);

        // Prefer existing external source / stub when present.
        if let Some(source) = frontmatter
            .source
            .clone()
            .or_else(|| existing.and_then(|d| d.frontmatter.source.clone()))
        {
            let mut fm = frontmatter;
            fm.source = Some(source.clone());
            let target = resolve_source_path(&self.project_root, &source).map_err(|e| {
                ValidationError {
                    error: e,
                    fields: Default::default(),
                }
            })?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| ValidationError {
                    error: e.to_string(),
                    fields: Default::default(),
                })?;
            }
            let full_raw = serialize_rule(&fm, &body);
            write_utf8(&target, &full_raw).map_err(|e| ValidationError {
                error: e,
                fields: Default::default(),
            })?;

            let stub_dir = resolve_item_write_dir(&self.project_root, scope, None).map_err(|e| {
                ValidationError {
                    error: e,
                    fields: Default::default(),
                }
            })?;
            let stub_path = existing
                .and_then(|d| d.stub_path.clone())
                .map(PathBuf::from)
                .unwrap_or_else(|| rule_file(&stub_dir.join("rules"), &fm.id));
            if let Some(parent) = stub_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| ValidationError {
                    error: e.to_string(),
                    fields: Default::default(),
                })?;
            }
            let stub_raw = serialize_rule_stub(&fm);
            write_utf8(&stub_path, &stub_raw).map_err(|e| ValidationError {
                error: e,
                fields: Default::default(),
            })?;

            let mut doc = parse_rule_file(&target, &full_raw)?;
            doc.source_path = target.to_string_lossy().into();
            return Ok((doc, Some(stub_path.to_string_lossy().into())));
        }

        let policy_dir = resolve_item_write_dir(
            &self.project_root,
            scope,
            frontmatter.root_id.as_deref(),
        )
        .map_err(|e| ValidationError {
            error: e,
            fields: Default::default(),
        })?;
        let path = rule_file(&policy_dir.join("rules"), &frontmatter.id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ValidationError {
                error: e.to_string(),
                fields: Default::default(),
            })?;
        }
        let raw = serialize_rule(&frontmatter, &body);
        write_utf8(&path, &raw).map_err(|e| ValidationError {
            error: e,
            fields: Default::default(),
        })?;
        let doc = parse_rule_file(&path, &raw)?;
        Ok((doc, None))
    }

    /// Save a rule under a new id, removing the previous row/file when `old_id` differs.
    pub async fn rename_rule(
        &self,
        old_id: &str,
        frontmatter: RuleFrontmatter,
        body: String,
    ) -> Result<PolicyRuleDoc, ValidationError> {
        if old_id == frontmatter.id {
            return self.save_rule(frontmatter, body).await;
        }

        let exists_old = self.get_rule_doc(old_id).await.map_err(|e| ValidationError {
            error: e.to_string(),
            fields: Default::default(),
        })?;
        if exists_old.is_none() {
            return Err(ValidationError {
                error: "not found".into(),
                fields: Default::default(),
            });
        }

        if self
            .get_rule_doc(&frontmatter.id)
            .await
            .map_err(|e| ValidationError {
                error: e.to_string(),
                fields: Default::default(),
            })?
            .is_some()
        {
            return Err(ValidationError {
                error: "validation_failed".into(),
                fields: [("id".into(), "already exists".into())]
                    .into_iter()
                    .collect(),
            });
        }

        self.delete_rule(old_id).await.map_err(|e| ValidationError {
            error: e.to_string(),
            fields: Default::default(),
        })?;

        self.save_rule(frontmatter, body).await
    }

    pub async fn save_skill(
        &self,
        mut frontmatter: SkillFrontmatter,
        body: String,
    ) -> Result<PolicySkillDoc, ValidationError> {
        let scope = PolicyScope::parse(&frontmatter.scope).unwrap_or(PolicyScope::Project);
        frontmatter.scope = scope.as_str().into();
        if let Some(ref s) = frontmatter.storage {
            if PolicyStorage::parse(s).is_none() {
                return Err(ValidationError {
                    error: "validation_failed".into(),
                    fields: [("storage".into(), "must be files or database".into())]
                        .into_iter()
                        .collect(),
                });
            }
        }

        let eff = self.item_effective_storage(frontmatter.storage.as_deref());
        let existing = self.get_skill_doc(&frontmatter.name).await.ok().flatten();

        if eff == PolicyStorage::Database {
            let path_hint = existing
                .as_ref()
                .map(|d| d.source_path.clone())
                .unwrap_or_else(|| {
                    skill_file(
                        &policy_dir_for_scope(&self.project_root, scope).join("skills"),
                        &frontmatter.name,
                    )
                    .to_string_lossy()
                    .into()
                });
            let raw = serialize_skill(&frontmatter, &body);
            let mut doc = parse_skill_file(Path::new(&path_hint), &raw)?;
            doc.stub_path = None;
            upsert_skill_doc(&self.pool, &doc)
                .await
                .map_err(|e| ValidationError {
                    error: e.to_string(),
                    fields: Default::default(),
                })?;
            return Ok(doc);
        }

        let (doc, stub_path) = self.write_skill_files(frontmatter, body, existing.as_ref())?;
        let mut doc = doc;
        doc.stub_path = stub_path;
        upsert_skill_doc(&self.pool, &doc)
            .await
            .map_err(|e| ValidationError {
                error: e.to_string(),
                fields: Default::default(),
            })?;
        Ok(doc)
    }

    fn write_skill_files(
        &self,
        frontmatter: SkillFrontmatter,
        body: String,
        existing: Option<&PolicySkillDoc>,
    ) -> Result<(PolicySkillDoc, Option<String>), ValidationError> {
        let scope = PolicyScope::parse(&frontmatter.scope).unwrap_or(PolicyScope::Project);

        if let Some(source) = frontmatter
            .source
            .clone()
            .or_else(|| existing.and_then(|d| d.frontmatter.source.clone()))
        {
            let mut fm = frontmatter;
            fm.source = Some(source.clone());
            let target = resolve_source_path(&self.project_root, &source).map_err(|e| {
                ValidationError {
                    error: e,
                    fields: Default::default(),
                }
            })?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| ValidationError {
                    error: e.to_string(),
                    fields: Default::default(),
                })?;
            }
            let full_raw = serialize_skill(&fm, &body);
            write_utf8(&target, &full_raw).map_err(|e| ValidationError {
                error: e,
                fields: Default::default(),
            })?;

            let stub_dir = resolve_item_write_dir(&self.project_root, scope, None).map_err(|e| {
                ValidationError {
                    error: e,
                    fields: Default::default(),
                }
            })?;
            let stub_path = existing
                .and_then(|d| d.stub_path.clone())
                .map(PathBuf::from)
                .unwrap_or_else(|| skill_file(&stub_dir.join("skills"), &fm.name));
            if let Some(parent) = stub_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| ValidationError {
                    error: e.to_string(),
                    fields: Default::default(),
                })?;
            }
            let stub_raw = serialize_skill_stub(&fm);
            write_utf8(&stub_path, &stub_raw).map_err(|e| ValidationError {
                error: e,
                fields: Default::default(),
            })?;

            let mut doc = parse_skill_file(&target, &full_raw)?;
            doc.source_path = target.to_string_lossy().into();
            return Ok((doc, Some(stub_path.to_string_lossy().into())));
        }

        let policy_dir = resolve_item_write_dir(
            &self.project_root,
            scope,
            frontmatter.root_id.as_deref(),
        )
        .map_err(|e| ValidationError {
            error: e,
            fields: Default::default(),
        })?;
        let skills = policy_dir.join("skills");
        let path = skill_file(&skills, &frontmatter.name);
        std::fs::create_dir_all(skills.join(&frontmatter.name)).map_err(|e| ValidationError {
            error: e.to_string(),
            fields: Default::default(),
        })?;
        let raw = serialize_skill(&frontmatter, &body);
        write_utf8(&path, &raw).map_err(|e| ValidationError {
            error: e,
            fields: Default::default(),
        })?;
        let doc = parse_skill_file(&path, &raw)?;
        Ok((doc, None))
    }

    pub async fn delete_rule(&self, id: &str) -> Result<bool, AxError> {
        let doc = self.get_rule_doc(id).await?;
        let Some(doc) = doc else {
            return delete_rule_by_id(&self.pool, id).await;
        };
        let eff = self.item_effective_storage(doc.frontmatter.storage.as_deref());
        if eff == PolicyStorage::Database {
            return delete_rule_by_id(&self.pool, id).await;
        }
        self.remove_rule_files(&doc)?;
        delete_rule_by_id(&self.pool, id).await
    }

    fn remove_rule_files(&self, doc: &PolicyRuleDoc) -> Result<(), AxError> {
        if let Some(ref stub) = doc.stub_path {
            let p = Path::new(stub);
            if p.is_file() {
                std::fs::remove_file(p).map_err(|e| AxError::Other(e.to_string()))?;
            }
        }
        let path = Path::new(&doc.source_path);
        if path.is_file() {
            std::fs::remove_file(path).map_err(|e| AxError::Other(e.to_string()))?;
        } else {
            let scope =
                PolicyScope::parse(&doc.frontmatter.scope).unwrap_or(PolicyScope::Project);
            let fallback = rule_file(
                &policy_dir_for_scope(&self.project_root, scope).join("rules"),
                &doc.frontmatter.id,
            );
            if fallback.is_file() {
                std::fs::remove_file(&fallback).map_err(|e| AxError::Other(e.to_string()))?;
            }
        }
        Ok(())
    }

    pub async fn delete_skill(&self, name: &str) -> Result<bool, AxError> {
        let doc = self.get_skill_doc(name).await?;
        let Some(doc) = doc else {
            return delete_skill_by_name(&self.pool, name).await;
        };
        let eff = self.item_effective_storage(doc.frontmatter.storage.as_deref());
        if eff == PolicyStorage::Database {
            return delete_skill_by_name(&self.pool, name).await;
        }
        self.remove_skill_files(&doc)?;
        delete_skill_by_name(&self.pool, name).await
    }

    fn remove_skill_files(&self, doc: &PolicySkillDoc) -> Result<(), AxError> {
        if let Some(ref stub) = doc.stub_path {
            let p = Path::new(stub);
            if p.is_file() {
                let _ = std::fs::remove_file(p);
                if let Some(parent) = p.parent() {
                    let _ = std::fs::remove_dir(parent);
                }
            }
        }
        let path = Path::new(&doc.source_path);
        if path.is_file() {
            std::fs::remove_file(path).map_err(|e| AxError::Other(e.to_string()))?;
            if let Some(parent) = path.parent() {
                let _ = std::fs::remove_dir(parent);
            }
        }
        Ok(())
    }

    /// Migrate a single rule/skill between files and database storage.
    pub async fn set_item_storage(
        &self,
        id_or_name: &str,
        target: PolicyStorage,
        keep_file: bool,
    ) -> Result<serde_json::Value, AxError> {
        if let Some(mut doc) = self.get_rule_doc(id_or_name).await? {
            let before = self.item_effective_storage(doc.frontmatter.storage.as_deref());
            doc.frontmatter.storage = Some(target.as_str().into());
            if target == PolicyStorage::Database {
                let body = doc.body.clone();
                let fm = doc.frontmatter.clone();
                // Persist to DB with override.
                self.save_rule(fm.clone(), body)
                    .await
                    .map_err(|e| AxError::Other(e.error))?;
                if !keep_file && before == PolicyStorage::Files {
                    let _ = self.remove_rule_files(&doc);
                    // Re-upsert without stale file paths
                    if let Some(mut again) = self.get_rule_doc(id_or_name).await? {
                        again.stub_path = None;
                        again.frontmatter.source = None;
                        again.frontmatter.storage = Some("database".into());
                        let raw = serialize_rule(&again.frontmatter, &again.body);
                        again.raw = raw;
                        upsert_rule_doc(&self.pool, &again).await?;
                    }
                }
                return Ok(serde_json::json!({
                    "ok": true,
                    "kind": "rule",
                    "id": id_or_name,
                    "storage": target.as_str(),
                    "effectiveStorage": target.as_str(),
                    "storageIsOverride": true,
                }));
            }
            // to files
            self.save_rule(doc.frontmatter, doc.body)
                .await
                .map_err(|e| AxError::Other(e.error))?;
            return Ok(serde_json::json!({
                "ok": true,
                "kind": "rule",
                "id": id_or_name,
                "storage": target.as_str(),
                "effectiveStorage": target.as_str(),
                "storageIsOverride": true,
            }));
        }

        if let Some(mut doc) = self.get_skill_doc(id_or_name).await? {
            let before = self.item_effective_storage(doc.frontmatter.storage.as_deref());
            doc.frontmatter.storage = Some(target.as_str().into());
            if target == PolicyStorage::Database {
                let body = doc.body.clone();
                let fm = doc.frontmatter.clone();
                self.save_skill(fm, body)
                    .await
                    .map_err(|e| AxError::Other(e.error))?;
                if !keep_file && before == PolicyStorage::Files {
                    let _ = self.remove_skill_files(&doc);
                    if let Some(mut again) = self.get_skill_doc(id_or_name).await? {
                        again.stub_path = None;
                        again.frontmatter.source = None;
                        again.frontmatter.storage = Some("database".into());
                        again.raw = serialize_skill(&again.frontmatter, &again.body);
                        upsert_skill_doc(&self.pool, &again).await?;
                    }
                }
                return Ok(serde_json::json!({
                    "ok": true,
                    "kind": "skill",
                    "name": id_or_name,
                    "storage": target.as_str(),
                    "effectiveStorage": target.as_str(),
                    "storageIsOverride": true,
                }));
            }
            self.save_skill(doc.frontmatter, doc.body)
                .await
                .map_err(|e| AxError::Other(e.error))?;
            return Ok(serde_json::json!({
                "ok": true,
                "kind": "skill",
                "name": id_or_name,
                "storage": target.as_str(),
                "effectiveStorage": target.as_str(),
                "storageIsOverride": true,
            }));
        }

        Err(AxError::Other("not found".into()))
    }

    /// Clear per-item storage override (inherit project default again).
    pub async fn clear_item_storage_override(&self, id_or_name: &str) -> Result<bool, AxError> {
        if let Some(mut doc) = self.get_rule_doc(id_or_name).await? {
            doc.frontmatter.storage = None;
            self.save_rule(doc.frontmatter, doc.body)
                .await
                .map_err(|e| AxError::Other(e.error))?;
            return Ok(true);
        }
        if let Some(mut doc) = self.get_skill_doc(id_or_name).await? {
            doc.frontmatter.storage = None;
            self.save_skill(doc.frontmatter, doc.body)
                .await
                .map_err(|e| AxError::Other(e.error))?;
            return Ok(true);
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
    use sqlx::sqlite::SqlitePoolOptions;

    if let Some(ax_dir) = db_path.parent() {
        ax_utils::clear_stale_lock(&ax_dir.join("ax.lock"));
    }
    let opts = ax_db::connect_options(db_path, false);
    let timeout = ax_db::busy_timeout();
    let timeout_ms = timeout.as_millis() as i64;

    ax_db::with_busy_retry(|| {
        let opts = opts.clone();
        async move {
            SqlitePoolOptions::new()
                .max_connections(2)
                .acquire_timeout(timeout)
                .after_connect(move |conn, _meta| {
                    Box::pin(async move {
                        sqlx::query(&format!("PRAGMA busy_timeout = {timeout_ms}"))
                            .execute(&mut *conn)
                            .await?;
                        Ok(())
                    })
                })
                .connect_with(opts)
                .await
        }
    })
    .await
    .map_err(|e| AxError::Database(ax_utils::errors::DatabaseError::new(e.to_string())))
}
