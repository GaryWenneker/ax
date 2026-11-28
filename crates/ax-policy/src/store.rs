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
    PolicyItemKind, PolicyRuleDoc, PolicyScope, PolicySkillDoc, RuleFrontmatter, SkillFrontmatter,
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

    async fn record_save_revision(
        &self,
        kind: &str,
        id: &str,
        raw: &str,
    ) -> Result<(), ValidationError> {
        crate::revisions::record_if_changed(
            &self.pool,
            kind,
            id,
            raw,
            crate::revisions::SOURCE_SAVE,
        )
        .await
        .map_err(|e| ValidationError {
            error: e.to_string(),
            fields: Default::default(),
        })?;
        Ok(())
    }

    pub async fn get_rule_doc(&self, id: &str) -> Result<Option<PolicyRuleDoc>, AxError> {
        if let Some(row) = crate::index::get_rule(&self.pool, id).await? {
            return Ok(Some(rule_row_to_doc(&row, &self.project_root)));
        }
        // Fallback: search layer dirs by filename. Only a valid id names a
        // file, so a path-like id cannot reach outside the layer folders.
        if !is_valid_item_id(id) {
            return Ok(None);
        }
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
        if !is_valid_item_id(name) {
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
        frontmatter: RuleFrontmatter,
        body: String,
    ) -> Result<PolicyRuleDoc, ValidationError> {
        let _writer = crate::write_lock::policy_write_lock(&self.project_root).await;
        self.save_rule_locked(frontmatter, body).await
    }

    /// [`Self::save_rule`] for a caller that already holds the write lock.
    async fn save_rule_locked(
        &self,
        mut frontmatter: RuleFrontmatter,
        body: String,
    ) -> Result<PolicyRuleDoc, ValidationError> {
        // The id becomes a file name: validate it before anything is written.
        if !is_valid_item_id(&frontmatter.id) {
            return Err(item_id_error("id", &frontmatter.id));
        }
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
            self.record_save_revision("rule", &doc.frontmatter.id, &doc.raw)
                .await?;
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
        self.record_save_revision("rule", &doc.frontmatter.id, &doc.raw)
            .await?;
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
            let target =
                resolve_source_path(&self.project_root, &source).map_err(|e| ValidationError {
                    error: e,
                    fields: Default::default(),
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

            let stub_dir =
                resolve_item_write_dir(&self.project_root, scope, None).map_err(|e| {
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

        let policy_dir = if frontmatter
            .root_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .is_some()
        {
            resolve_item_write_dir(&self.project_root, scope, frontmatter.root_id.as_deref())
                .map_err(|e| ValidationError {
                    error: e,
                    fields: Default::default(),
                })?
        } else {
            crate::agents_share::resolve_shareable_write_dir(
                &self.project_root,
                scope,
                frontmatter.enabled,
            )
        };
        let path = rule_file(&policy_dir.join("rules"), &frontmatter.id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ValidationError {
                error: e.to_string(),
                fields: Default::default(),
            })?;
        }
        if scope.is_packable() && !frontmatter.enabled {
            crate::agents_share::ensure_ax_share_gitignore(&self.project_root).map_err(|e| {
                ValidationError {
                    error: e.to_string(),
                    fields: Default::default(),
                }
            })?;
        }
        let raw = serialize_rule(&frontmatter, &body);
        write_utf8(&path, &raw).map_err(|e| ValidationError {
            error: e,
            fields: Default::default(),
        })?;
        if scope.is_packable() {
            crate::agents_share::relocate_rule_file(&self.project_root, &frontmatter.id, &path)
                .map_err(|e| ValidationError {
                    error: e.to_string(),
                    fields: Default::default(),
                })?;
        }
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
        // Check, save and delete under one hold, so no save or index pass
        // lands between the checks and the writes.
        let _writer = crate::write_lock::policy_write_lock(&self.project_root).await;
        if old_id == frontmatter.id {
            return self.save_rule_locked(frontmatter, body).await;
        }

        let exists_old = self
            .get_rule_doc(old_id)
            .await
            .map_err(|e| ValidationError {
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

        // Save the new rule first: if that fails, the old one is untouched.
        let new_doc = self.save_rule_locked(frontmatter, body).await?;
        if let Err(e) = self.delete_renamed_rule(old_id, &new_doc).await {
            return Err(ValidationError {
                error: format!(
                    "saved `{}`, but the old rule `{old_id}` could not be removed: {e}",
                    new_doc.frontmatter.id
                ),
                fields: Default::default(),
            });
        }
        Ok(new_doc)
    }

    /// Remove the old rule after a rename, keeping any file the new rule now
    /// uses (a shared `source:` file).
    async fn delete_renamed_rule(
        &self,
        old_id: &str,
        new_doc: &PolicyRuleDoc,
    ) -> Result<(), AxError> {
        if let Some(doc) = self.get_rule_doc(old_id).await? {
            let eff = self.item_effective_storage(doc.frontmatter.storage.as_deref());
            if eff == PolicyStorage::Files {
                let keep = [
                    Some(new_doc.source_path.as_str()),
                    new_doc.stub_path.as_deref(),
                ];
                self.remove_rule_files_except(&doc, &keep)?;
            }
        }
        delete_rule_by_id(&self.pool, old_id).await?;
        Ok(())
    }

    pub async fn save_skill(
        &self,
        frontmatter: SkillFrontmatter,
        body: String,
    ) -> Result<PolicySkillDoc, ValidationError> {
        let _writer = crate::write_lock::policy_write_lock(&self.project_root).await;
        self.save_skill_locked(frontmatter, body).await
    }

    /// [`Self::save_skill`] for a caller that already holds the write lock.
    async fn save_skill_locked(
        &self,
        mut frontmatter: SkillFrontmatter,
        body: String,
    ) -> Result<PolicySkillDoc, ValidationError> {
        // The name becomes a folder name: validate it before anything is written.
        if !is_valid_item_id(&frontmatter.name) {
            return Err(item_id_error("name", &frontmatter.name));
        }
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
            self.record_save_revision("skill", &doc.frontmatter.name, &doc.raw)
                .await?;
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
        self.record_save_revision("skill", &doc.frontmatter.name, &doc.raw)
            .await?;
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
            let target =
                resolve_source_path(&self.project_root, &source).map_err(|e| ValidationError {
                    error: e,
                    fields: Default::default(),
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

            let stub_dir =
                resolve_item_write_dir(&self.project_root, scope, None).map_err(|e| {
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

        let policy_dir = if frontmatter
            .root_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .is_some()
        {
            resolve_item_write_dir(&self.project_root, scope, frontmatter.root_id.as_deref())
                .map_err(|e| ValidationError {
                    error: e,
                    fields: Default::default(),
                })?
        } else {
            crate::agents_share::resolve_shareable_write_dir(
                &self.project_root,
                scope,
                frontmatter.enabled,
            )
        };
        let skills = policy_dir.join("skills");
        let path = skill_file(&skills, &frontmatter.name);
        std::fs::create_dir_all(skills.join(&frontmatter.name)).map_err(|e| ValidationError {
            error: e.to_string(),
            fields: Default::default(),
        })?;
        if scope.is_packable() && !frontmatter.enabled {
            crate::agents_share::ensure_ax_share_gitignore(&self.project_root).map_err(|e| {
                ValidationError {
                    error: e.to_string(),
                    fields: Default::default(),
                }
            })?;
        }
        let raw = serialize_skill(&frontmatter, &body);
        write_utf8(&path, &raw).map_err(|e| ValidationError {
            error: e,
            fields: Default::default(),
        })?;
        if scope.is_packable() {
            crate::agents_share::relocate_skill_dir(
                &self.project_root,
                &frontmatter.name,
                &skills.join(&frontmatter.name),
            )
            .map_err(|e| ValidationError {
                error: e.to_string(),
                fields: Default::default(),
            })?;
        }
        let doc = parse_skill_file(&path, &raw)?;
        Ok((doc, None))
    }

    pub async fn delete_rule(&self, id: &str) -> Result<bool, AxError> {
        let _writer = crate::write_lock::policy_write_lock(&self.project_root).await;
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
        self.remove_rule_files_except(doc, &[])
    }

    /// Remove a rule's files, leaving any path listed in `keep`.
    fn remove_rule_files_except(
        &self,
        doc: &PolicyRuleDoc,
        keep: &[Option<&str>],
    ) -> Result<(), AxError> {
        let kept = |p: &Path| keep.iter().flatten().any(|k| Path::new(k) == p);
        if let Some(ref stub) = doc.stub_path {
            let p = Path::new(stub);
            if p.is_file() && !kept(p) {
                std::fs::remove_file(p).map_err(|e| AxError::Other(e.to_string()))?;
            }
        }
        let path = Path::new(&doc.source_path);
        if path.is_file() {
            if !kept(path) {
                std::fs::remove_file(path).map_err(|e| AxError::Other(e.to_string()))?;
            }
        } else {
            let scope = PolicyScope::parse(&doc.frontmatter.scope).unwrap_or(PolicyScope::Project);
            let fallback = rule_file(
                &policy_dir_for_scope(&self.project_root, scope).join("rules"),
                &doc.frontmatter.id,
            );
            if fallback.is_file() {
                std::fs::remove_file(&fallback).map_err(|e| AxError::Other(e.to_string()))?;
            }
        }
        self.note_removed("rule", &format!("rules/{}.mdc", doc.frontmatter.id))
    }

    pub async fn delete_skill(&self, name: &str) -> Result<bool, AxError> {
        let _writer = crate::write_lock::policy_write_lock(&self.project_root).await;
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
        self.note_removed(
            "skill",
            &format!("skills/{}/SKILL.md", doc.frontmatter.name),
        )
    }

    fn note_removed(&self, kind: &str, rel: &str) -> Result<(), AxError> {
        crate::history::note_removed(
            &crate::history::project_history_path(&self.project_root),
            kind,
            rel,
        )
        .map_err(|e| AxError::Other(e.to_string()))
    }

    /// Find the rule or skill an id names. With `kind`, only that kind is
    /// looked up. Without it, an id that names both a rule and a skill is an
    /// error, so one of them is never changed by mistake.
    async fn find_item(
        &self,
        id_or_name: &str,
        kind: Option<PolicyItemKind>,
    ) -> Result<Option<FoundItem>, AxError> {
        let rule = if kind != Some(PolicyItemKind::Skill) {
            self.get_rule_doc(id_or_name).await?
        } else {
            None
        };
        let skill = if kind != Some(PolicyItemKind::Rule) {
            self.get_skill_doc(id_or_name).await?
        } else {
            None
        };
        match (rule, skill) {
            (Some(_), Some(_)) => Err(AxError::Other(format!(
                "both a rule and a skill are named '{id_or_name}': use rule:{id_or_name} or skill:{id_or_name}"
            ))),
            (Some(rule), None) => Ok(Some(FoundItem::Rule(Box::new(rule)))),
            (None, Some(skill)) => Ok(Some(FoundItem::Skill(Box::new(skill)))),
            (None, None) => Ok(None),
        }
    }

    /// Migrate a single rule/skill between files and database storage.
    pub async fn set_item_storage(
        &self,
        kind: Option<PolicyItemKind>,
        id_or_name: &str,
        target: PolicyStorage,
        keep_file: bool,
    ) -> Result<serde_json::Value, AxError> {
        self.try_set_item_storage(kind, id_or_name, target, keep_file)
            .await?
            .ok_or_else(|| AxError::Other("not found".into()))
    }

    /// Like [`Self::set_item_storage`], but `None` when no item of `kind` has
    /// this id, so a caller can tell "not found" from a failure.
    pub async fn try_set_item_storage(
        &self,
        kind: Option<PolicyItemKind>,
        id_or_name: &str,
        target: PolicyStorage,
        keep_file: bool,
    ) -> Result<Option<serde_json::Value>, AxError> {
        // Read, write back and clean up under one hold, so a save in between
        // is kept.
        let _writer = crate::write_lock::policy_write_lock(&self.project_root).await;
        let found = self.find_item(id_or_name, kind).await?;
        if let Some(FoundItem::Rule(doc)) = found {
            let mut doc = *doc;
            let before = self.item_effective_storage(doc.frontmatter.storage.as_deref());
            doc.frontmatter.storage = Some(target.as_str().into());
            if target == PolicyStorage::Database {
                let body = doc.body.clone();
                let fm = doc.frontmatter.clone();
                // Persist to DB with override.
                self.save_rule_locked(fm.clone(), body)
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
                return Ok(Some(serde_json::json!({
                    "ok": true,
                    "kind": "rule",
                    "id": id_or_name,
                    "storage": target.as_str(),
                    "effectiveStorage": target.as_str(),
                    "storageIsOverride": true,
                })));
            }
            // to files
            self.save_rule_locked(doc.frontmatter, doc.body)
                .await
                .map_err(|e| AxError::Other(e.error))?;
            return Ok(Some(serde_json::json!({
                "ok": true,
                "kind": "rule",
                "id": id_or_name,
                "storage": target.as_str(),
                "effectiveStorage": target.as_str(),
                "storageIsOverride": true,
            })));
        }

        if let Some(FoundItem::Skill(doc)) = found {
            let mut doc = *doc;
            let before = self.item_effective_storage(doc.frontmatter.storage.as_deref());
            doc.frontmatter.storage = Some(target.as_str().into());
            if target == PolicyStorage::Database {
                let body = doc.body.clone();
                let fm = doc.frontmatter.clone();
                self.save_skill_locked(fm, body)
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
                return Ok(Some(serde_json::json!({
                    "ok": true,
                    "kind": "skill",
                    "name": id_or_name,
                    "storage": target.as_str(),
                    "effectiveStorage": target.as_str(),
                    "storageIsOverride": true,
                })));
            }
            self.save_skill_locked(doc.frontmatter, doc.body)
                .await
                .map_err(|e| AxError::Other(e.error))?;
            return Ok(Some(serde_json::json!({
                "ok": true,
                "kind": "skill",
                "name": id_or_name,
                "storage": target.as_str(),
                "effectiveStorage": target.as_str(),
                "storageIsOverride": true,
            })));
        }

        Ok(None)
    }

    /// Clear per-item storage override (inherit project default again).
    pub async fn clear_item_storage_override(
        &self,
        kind: Option<PolicyItemKind>,
        id_or_name: &str,
    ) -> Result<bool, AxError> {
        // Read and write back under one lock, so a save in between is kept.
        let _writer = crate::write_lock::policy_write_lock(&self.project_root).await;
        match self.find_item(id_or_name, kind).await? {
            Some(FoundItem::Rule(doc)) => {
                let mut doc = *doc;
                doc.frontmatter.storage = None;
                self.save_rule_locked(doc.frontmatter, doc.body)
                    .await
                    .map_err(|e| AxError::Other(e.error))?;
                Ok(true)
            }
            Some(FoundItem::Skill(doc)) => {
                let mut doc = *doc;
                doc.frontmatter.storage = None;
                self.save_skill_locked(doc.frontmatter, doc.body)
                    .await
                    .map_err(|e| AxError::Other(e.error))?;
                Ok(true)
            }
            None => Ok(false),
        }
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
    /// `kind` picks the rule or the skill when both share the name.
    pub async fn set_enabled(
        &self,
        kind: Option<PolicyItemKind>,
        id_or_name: &str,
        enabled: bool,
    ) -> Result<bool, AxError> {
        // Read and write back under one lock, so a save in between is kept.
        let _writer = crate::write_lock::policy_write_lock(&self.project_root).await;
        match self.find_item(id_or_name, kind).await? {
            Some(FoundItem::Rule(doc)) => {
                let mut doc = *doc;
                doc.frontmatter.enabled = enabled;
                self.save_rule_locked(doc.frontmatter, doc.body)
                    .await
                    .map_err(|e| AxError::Other(e.error))?;
                Ok(true)
            }
            Some(FoundItem::Skill(doc)) => {
                let mut doc = *doc;
                doc.frontmatter.enabled = enabled;
                self.save_skill_locked(doc.frontmatter, doc.body)
                    .await
                    .map_err(|e| AxError::Other(e.error))?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

enum FoundItem {
    Rule(Box<PolicyRuleDoc>),
    Skill(Box<PolicySkillDoc>),
}

/// A rule id or skill name that can name a file: kebab-case, not empty.
fn is_valid_item_id(id: &str) -> bool {
    !id.is_empty() && crate::parse::is_kebab_case(id)
}

fn item_id_error(field: &str, value: &str) -> ValidationError {
    let message = if value.is_empty() {
        "required"
    } else {
        "must be kebab-case"
    };
    ValidationError {
        error: "validation_failed".into(),
        fields: [(field.into(), message.into())].into_iter().collect(),
    }
}

fn write_utf8(path: &Path, content: &str) -> Result<(), String> {
    std::fs::write(path, content.as_bytes()).map_err(|e| e.to_string())
}

pub async fn open_rw_pool(db_path: &Path) -> Result<sqlx::SqlitePool, AxError> {
    use sqlx::sqlite::SqlitePoolOptions;

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

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_store(root: &Path) -> PolicyStore {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        ax_db::schema::apply_initial_schema(&pool).await.unwrap();
        ax_db::migrations::run_migrations(&pool, 0).await.unwrap();
        PolicyStore::new(pool, root.to_path_buf())
    }

    fn rule_frontmatter(id: &str) -> RuleFrontmatter {
        let mut fm = parse_rule_file(
            Path::new("r.mdc"),
            "---\nid: placeholder\nlevel: INFO\nalwaysApply: true\n---\n\nBody.\n",
        )
        .unwrap()
        .frontmatter;
        fm.id = id.into();
        fm
    }

    #[tokio::test]
    async fn save_rule_refuses_a_bad_id_before_writing_any_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = test_store(dir.path()).await;
        let err = store
            .save_rule(rule_frontmatter("My Rule"), "Body.".into())
            .await
            .expect_err("a non-kebab id must be refused");
        assert_eq!(
            err.fields.get("id").map(String::as_str),
            Some("must be kebab-case")
        );
        let rules = crate::agents_share::agents_dir(dir.path()).join("rules");
        assert!(!rules.join("My Rule.mdc").exists());
        let err = store
            .save_rule(rule_frontmatter(""), "Body.".into())
            .await
            .expect_err("an empty id must be refused");
        assert_eq!(err.fields.get("id").map(String::as_str), Some("required"));
        assert!(!rules.join(".mdc").exists());
    }

    #[tokio::test]
    async fn a_rename_that_fails_validation_keeps_the_old_rule() {
        let dir = tempfile::tempdir().unwrap();
        let store = test_store(dir.path()).await;
        let saved = store
            .save_rule(rule_frontmatter("old-rule"), "Keep me.".into())
            .await
            .unwrap();
        let old_file = PathBuf::from(&saved.source_path);
        assert!(old_file.is_file());

        let err = store
            .rename_rule("old-rule", rule_frontmatter("Api Rules"), "Keep me.".into())
            .await
            .expect_err("a non-kebab id must be refused");
        assert_eq!(
            err.fields.get("id").map(String::as_str),
            Some("must be kebab-case")
        );
        let mut bad_storage = rule_frontmatter("new-rule");
        bad_storage.storage = Some("cloud".into());
        store
            .rename_rule("old-rule", bad_storage, "Keep me.".into())
            .await
            .expect_err("an unknown storage must be refused");

        assert!(old_file.is_file());
        let old = store.get_rule_doc("old-rule").await.unwrap().unwrap();
        assert_eq!(old.body.trim(), "Keep me.");
    }

    #[tokio::test]
    async fn a_rename_moves_the_rule_to_its_new_id() {
        let dir = tempfile::tempdir().unwrap();
        let store = test_store(dir.path()).await;
        let saved = store
            .save_rule(rule_frontmatter("old-rule"), "Moved.".into())
            .await
            .unwrap();
        let renamed = store
            .rename_rule("old-rule", rule_frontmatter("new-rule"), "Moved.".into())
            .await
            .unwrap();
        assert!(!Path::new(&saved.source_path).exists());
        assert!(Path::new(&renamed.source_path).is_file());
        assert!(store.get_rule_doc("old-rule").await.unwrap().is_none());
        let new = store.get_rule_doc("new-rule").await.unwrap().unwrap();
        assert_eq!(new.body.trim(), "Moved.");
    }

    #[tokio::test]
    async fn a_rename_keeps_the_shared_source_file() {
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("docs").join("team-rule.mdc");
        std::fs::create_dir_all(shared.parent().unwrap()).unwrap();
        std::fs::write(&shared, "placeholder\n").unwrap();
        let store = test_store(dir.path()).await;
        let mut fm = rule_frontmatter("old-rule");
        fm.source = Some("docs/team-rule.mdc".into());
        store.save_rule(fm.clone(), "Shared.".into()).await.unwrap();

        fm.id = "new-rule".into();
        let renamed = store
            .rename_rule("old-rule", fm, "Shared.".into())
            .await
            .unwrap();
        assert!(
            shared.is_file(),
            "the shared source must survive the rename"
        );
        assert!(std::fs::read_to_string(&shared)
            .unwrap()
            .contains("Shared."));
        assert_ne!(renamed.stub_path, None);
        assert!(store.get_rule_doc("old-rule").await.unwrap().is_none());
    }

    fn skill_frontmatter(name: &str) -> SkillFrontmatter {
        let mut fm = parse_skill_file(
            Path::new("SKILL.md"),
            "---\nname: placeholder\ndescription: d\n---\n\nBody.\n",
        )
        .unwrap()
        .frontmatter;
        fm.name = name.into();
        fm
    }

    #[tokio::test]
    async fn disabling_a_skill_leaves_the_rule_with_the_same_name() {
        let dir = tempfile::tempdir().unwrap();
        let store = test_store(dir.path()).await;
        store
            .save_rule(rule_frontmatter("subagents"), "Rule.".into())
            .await
            .unwrap();
        store
            .save_skill(skill_frontmatter("subagents"), "Skill.".into())
            .await
            .unwrap();
        let err = store
            .set_enabled(None, "subagents", false)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("skill:subagents"), "{err}");
        assert!(store
            .set_enabled(Some(PolicyItemKind::Skill), "subagents", false)
            .await
            .unwrap());
        let rule = store.get_rule_doc("subagents").await.unwrap().unwrap();
        let skill = store.get_skill_doc("subagents").await.unwrap().unwrap();
        assert!(rule.frontmatter.enabled, "the rule must stay enabled");
        assert!(!skill.frontmatter.enabled, "the skill must be disabled");

        store
            .set_item_storage(
                Some(PolicyItemKind::Rule),
                "subagents",
                PolicyStorage::Database,
                true,
            )
            .await
            .unwrap();
        let rule = store.get_rule_doc("subagents").await.unwrap().unwrap();
        let skill = store.get_skill_doc("subagents").await.unwrap().unwrap();
        assert_eq!(rule.frontmatter.storage.as_deref(), Some("database"));
        assert_eq!(skill.frontmatter.storage, None);
        assert!(store
            .clear_item_storage_override(Some(PolicyItemKind::Rule), "subagents")
            .await
            .unwrap());
        let rule = store.get_rule_doc("subagents").await.unwrap().unwrap();
        assert_eq!(rule.frontmatter.storage, None);
    }

    #[tokio::test]
    async fn lookups_ignore_a_path_like_id() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        let rules = crate::agents_share::agents_dir(&root).join("rules");
        std::fs::create_dir_all(&rules).unwrap();
        std::fs::write(
            dir.path().join("outside.mdc"),
            "---\nid: outside\nlevel: INFO\nalwaysApply: true\n---\n\nOutside.\n",
        )
        .unwrap();
        let store = test_store(&root).await;
        let id = "../../../outside";
        assert!(store.get_rule_doc(id).await.unwrap().is_none());
        assert!(store.get_skill_doc(id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn save_skill_refuses_a_name_that_is_not_kebab_case_before_writing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        std::fs::create_dir_all(&root).unwrap();
        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
        let store = PolicyStore::new(pool, root.clone());
        let doc = parse_skill_file(
            Path::new("SKILL.md"),
            "---\nname: placeholder\ndescription: d\n---\n\nBody.\n",
        )
        .unwrap();
        let mut fm = doc.frontmatter;
        fm.name = "../../escaped".into();
        let err = store
            .save_skill(fm, "Body.".into())
            .await
            .expect_err("a path-like skill name must be refused");
        assert_eq!(
            err.fields.get("name").map(String::as_str),
            Some("must be kebab-case")
        );
        assert!(!dir.path().join("escaped").exists());
        assert!(!root.join("escaped").exists());
    }
}

#[cfg(test)]
/// Tests that a read-modify-write keeps a save that queued before it.
mod lock_tests {
    use super::*;

    fn rule(id: &str, body: &str) -> (RuleFrontmatter, String) {
        let raw = format!("---\nid: {id}\nlevel: WARNING\nalwaysApply: true\n---\n\n{body}");
        let doc = crate::parse_rule_file(Path::new("x.mdc"), &raw).unwrap();
        (doc.frontmatter, doc.body)
    }

    /// Enabling or disabling reads the rule and writes it back. A save that
    /// lands between the read and the write must not be undone by the old
    /// body coming back, so the read happens under the write lock too.
    #[tokio::test]
    async fn disabling_a_rule_keeps_a_body_saved_while_it_waited() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join(".ax")).unwrap();
        let db = ax_db::Database::open(&root.join(".ax").join("ax.db"))
            .await
            .unwrap();
        let store = std::sync::Arc::new(PolicyStore::new(db.pool().clone(), root.clone()));
        let (fm, body) = rule("toggled", "Old body.");
        store.save_rule(fm, body).await.unwrap();

        // Hold the lock so both writers queue: the save first, then the toggle.
        let writer = crate::write_lock::policy_write_lock(&root).await;
        let saver = {
            let store = store.clone();
            let (fm, body) = rule("toggled", "New body.");
            tokio::spawn(async move { store.save_rule(fm, body).await.map(|_| ()) })
        };
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let toggler = {
            let store = store.clone();
            tokio::spawn(async move {
                store
                    .set_enabled(Some(PolicyItemKind::Rule), "toggled", false)
                    .await
            })
        };
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        drop(writer);
        saver.await.unwrap().unwrap();
        assert!(toggler.await.unwrap().unwrap());

        let row = crate::get_rule(store.pool(), "toggled")
            .await
            .unwrap()
            .unwrap();
        assert!(
            row.body.contains("New body."),
            "lost the save: {}",
            row.body
        );
        assert!(!row.enabled, "the rule stayed enabled");
    }

    async fn test_store() -> (
        tempfile::TempDir,
        std::path::PathBuf,
        std::sync::Arc<PolicyStore>,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join(".ax")).unwrap();
        let db = ax_db::Database::open(&root.join(".ax").join("ax.db"))
            .await
            .unwrap();
        let store = std::sync::Arc::new(PolicyStore::new(db.pool().clone(), root.clone()));
        (dir, root, store)
    }

    /// Runs `queued_save` and then `change` while the test holds the write
    /// lock, so both queue in that order once it is released.
    async fn queue_save_then<T: Send + 'static>(
        root: &Path,
        queued_save: impl std::future::Future<Output = ()> + Send + 'static,
        change: impl std::future::Future<Output = T> + Send + 'static,
    ) -> T {
        let writer = crate::write_lock::policy_write_lock(root).await;
        let saver = tokio::spawn(queued_save);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let changer = tokio::spawn(change);
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        drop(writer);
        saver.await.unwrap();
        changer.await.unwrap()
    }

    /// Moving a rule to the database reads it and writes it back. A save
    /// that lands in between must survive.
    #[tokio::test]
    async fn moving_a_rule_to_the_database_keeps_a_body_saved_while_it_waited() {
        let (_dir, root, store) = test_store().await;
        let (fm, body) = rule("moved", "Old body.");
        store.save_rule(fm, body).await.unwrap();

        let saved = {
            let store = store.clone();
            let (fm, body) = rule("moved", "New body.");
            async move {
                store.save_rule(fm, body).await.unwrap();
            }
        };
        let change = {
            let store = store.clone();
            async move {
                store
                    .try_set_item_storage(
                        Some(PolicyItemKind::Rule),
                        "moved",
                        PolicyStorage::Database,
                        false,
                    )
                    .await
            }
        };
        let out = queue_save_then(&root, saved, change).await.unwrap();
        assert!(out.is_some());

        let row = crate::get_rule(store.pool(), "moved")
            .await
            .unwrap()
            .unwrap();
        assert!(
            row.body.contains("New body."),
            "lost the save: {}",
            row.body
        );
        assert_eq!(row.storage.as_deref(), Some("database"));
    }

    /// Same as the rule case, for a skill.
    #[tokio::test]
    async fn moving_a_skill_to_the_database_keeps_a_body_saved_while_it_waited() {
        let (_dir, root, store) = test_store().await;
        let skill = |body: &str| {
            let raw = format!("---\nname: moved\ndescription: A skill.\n---\n\n{body}");
            let doc = crate::parse_skill_file(Path::new("SKILL.md"), &raw).unwrap();
            (doc.frontmatter, doc.body)
        };
        let (fm, body) = skill("Old body.");
        store.save_skill(fm, body).await.unwrap();

        let saved = {
            let store = store.clone();
            let (fm, body) = skill("New body.");
            async move {
                store.save_skill(fm, body).await.unwrap();
            }
        };
        let change = {
            let store = store.clone();
            async move {
                store
                    .try_set_item_storage(
                        Some(PolicyItemKind::Skill),
                        "moved",
                        PolicyStorage::Database,
                        false,
                    )
                    .await
            }
        };
        let out = queue_save_then(&root, saved, change).await.unwrap();
        assert!(out.is_some());

        let row = crate::get_skill(store.pool(), "moved")
            .await
            .unwrap()
            .unwrap();
        assert!(
            row.body.contains("New body."),
            "lost the save: {}",
            row.body
        );
    }

    /// Clearing a storage override reads and writes back too.
    #[tokio::test]
    async fn clearing_a_storage_override_keeps_a_body_saved_while_it_waited() {
        let (_dir, root, store) = test_store().await;
        let (mut fm, body) = rule("cleared", "Old body.");
        fm.storage = Some("files".into());
        store.save_rule(fm, body).await.unwrap();

        let saved = {
            let store = store.clone();
            let (mut fm, body) = rule("cleared", "New body.");
            fm.storage = Some("files".into());
            async move {
                store.save_rule(fm, body).await.unwrap();
            }
        };
        let change = {
            let store = store.clone();
            async move {
                store
                    .clear_item_storage_override(Some(PolicyItemKind::Rule), "cleared")
                    .await
            }
        };
        assert!(queue_save_then(&root, saved, change).await.unwrap());

        let row = crate::get_rule(store.pool(), "cleared")
            .await
            .unwrap()
            .unwrap();
        assert!(
            row.body.contains("New body."),
            "lost the save: {}",
            row.body
        );
    }

    /// A rename checks that the new id is free and then writes it. A rule
    /// saved under the new id in the meantime must not be overwritten.
    #[tokio::test]
    async fn renaming_a_rule_keeps_a_rule_saved_under_the_new_id_while_it_waited() {
        let (_dir, root, store) = test_store().await;
        let (fm, body) = rule("old-id", "Renamed body.");
        store.save_rule(fm, body).await.unwrap();

        let saved = {
            let store = store.clone();
            let (fm, body) = rule("new-id", "Other rule.");
            async move {
                store.save_rule(fm, body).await.unwrap();
            }
        };
        let change = {
            let store = store.clone();
            let (fm, body) = rule("new-id", "Renamed body.");
            async move { store.rename_rule("old-id", fm, body).await }
        };
        let err = queue_save_then(&root, saved, change)
            .await
            .expect_err("the rename overwrote a rule saved under its new id");
        assert_eq!(err.error, "validation_failed");

        let other = crate::get_rule(store.pool(), "new-id")
            .await
            .unwrap()
            .unwrap();
        assert!(other.body.contains("Other rule."), "{}", other.body);
        assert!(crate::get_rule(store.pool(), "old-id")
            .await
            .unwrap()
            .is_some());
    }
}
