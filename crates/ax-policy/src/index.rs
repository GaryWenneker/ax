use sqlx::SqlitePool;
use std::path::Path;

use ax_utils::errors::{AxError, DatabaseError};

use crate::config::{effective_storage, load_policy_config, PolicyStorage};
use crate::parse::{parse_rule_file, parse_skill_file, serialize_rule, serialize_skill};
use crate::paths::{
    ax_dir_from_project, is_stub_body, resolve_source_path, rule_file, rules_dir, skill_file,
    skills_dir,
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
    let _ = crate::history::mirror_project(pool, project_root).await;
    let config = load_policy_config(project_root);
    match config.storage {
        PolicyStorage::Database => {
            if force {
                import_policy_from_files(pool, project_root, ImportMode::Merge).await
            } else {
                db_counts(pool).await
            }
        }
        PolicyStorage::Files => {
            import_policy_from_files(pool, project_root, ImportMode::Replace).await
        }
    }
}

pub async fn import_policy_from_files(
    pool: &SqlitePool,
    project_root: &Path,
    mode: ImportMode,
) -> Result<PolicyIndexResult, AxError> {
    let _ = crate::agents_share::migrate_legacy_policy_to_agents(project_root);
    crate::hierarchy::ensure_scope_dirs(project_root, crate::types::PolicyScope::Project)
        .map_err(|e| AxError::Other(e.to_string()))?;
    let leaks = crate::agents_share::agents_share_violations(project_root);
    if !leaks.is_empty() {
        return Err(AxError::Other(format!(
            ".agents contains private or inactive files: {}",
            leaks.join("; ")
        )));
    }

    let MergedLayers {
        rules,
        skills,
        skipped,
        kept_paths,
        notes,
        rules_indexed,
        skills_indexed,
    } = merge_policy_layers(project_root);
    for note in &notes {
        eprintln!("{note}");
    }
    let seen_rules: Vec<String> = rules
        .iter()
        .map(|(d, _)| d.frontmatter.id.clone())
        .collect();
    let seen_skills: Vec<String> = skills
        .iter()
        .map(|(d, _)| d.frontmatter.name.clone())
        .collect();

    // One transaction: a reader never sees a half-updated policy, and rows whose content did
    // not change are not rewritten (so the matcher cache generation stays the same).
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    let stored_rules = stored_state(&mut tx, PruneTable::Rules).await?;
    let stored_skills = stored_state(&mut tx, PruneTable::Skills).await?;
    let now = now_ms();
    for (doc, hash) in &rules {
        let current = StoredState::of(
            hash,
            &doc.frontmatter.scope,
            &doc.source_path,
            &doc.stub_path,
        );
        if stored_rules.get(&doc.frontmatter.id) != Some(&current) {
            upsert_rule(&mut *tx, doc, hash, now).await?;
        }
    }
    for (doc, hash) in &skills {
        let current = StoredState::of(
            hash,
            &doc.frontmatter.scope,
            &doc.source_path,
            &doc.stub_path,
        );
        if stored_skills.get(&doc.frontmatter.name) != Some(&current) {
            upsert_skill(&mut *tx, doc, hash, now).await?;
        }
    }

    if mode == ImportMode::Replace {
        // Hybrid-aware prune: keep per-item database overrides not present on disk.
        // A file that exists but cannot be read, or whose frontmatter is not YAML,
        // keeps the row it produced before. A file that parses but fails validation
        // (unknown status or scope, a bad name, a missing field) loses its row, so
        // an invalid rule never stays active as approved.
        prune_hybrid(&mut tx, PruneTable::Rules, &seen_rules, &kept_paths).await?;
        prune_hybrid(&mut tx, PruneTable::Skills, &seen_skills, &kept_paths).await?;
    }
    tx.commit()
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;

    Ok(PolicyIndexResult {
        rules_indexed,
        skills_indexed,
        skipped,
    })
}

/// Every policy layer parsed and merged the way an import writes it: the last
/// layer wins per id, with the hash an import stores.
struct MergedLayers {
    rules: Vec<(PolicyRuleDoc, String)>,
    skills: Vec<(PolicySkillDoc, String)>,
    skipped: Vec<crate::types::PolicySkip>,
    /// Skipped files whose stored row is kept (see [`LayerImport::kept`]).
    kept_paths: Vec<String>,
    notes: Vec<String>,
    rules_indexed: u32,
    skills_indexed: u32,
}

fn merge_policy_layers(project_root: &Path) -> MergedLayers {
    // Hierarchical merge: company → workspace → project → private (later upserts win).
    let layers = crate::hierarchy::policy_layers(project_root);
    let layer_list = if layers.is_empty() {
        vec![crate::hierarchy::PolicyLayer {
            dir: crate::agents_share::agents_dir(project_root),
            scope: crate::types::PolicyScope::Project,
            root_id: None,
            preserve_item_scope: false,
        }]
    } else {
        layers
    };

    // Later layers win: keep the last doc per id, in first-seen order.
    let mut out = MergedLayers {
        rules: Vec::new(),
        skills: Vec::new(),
        skipped: Vec::new(),
        kept_paths: Vec::new(),
        notes: Vec::new(),
        rules_indexed: 0,
        skills_indexed: 0,
    };
    for layer in &layer_list {
        let layer_import = import_one_policy_dir(
            project_root,
            &layer.dir,
            layer.scope,
            layer.preserve_item_scope,
        );
        out.rules_indexed += layer_import.rules.len() as u32;
        out.skills_indexed += layer_import.skills.len() as u32;
        for item in layer_import.rules {
            last_wins(&mut out.rules, item, |(d, _)| d.frontmatter.id.as_str());
        }
        for item in layer_import.skills {
            last_wins(&mut out.skills, item, |(d, _)| d.frontmatter.name.as_str());
        }
        out.skipped.extend(layer_import.skipped);
        out.kept_paths.extend(layer_import.kept);
        out.notes.extend(layer_import.notes);
    }
    out
}

fn last_wins<T>(items: &mut Vec<T>, item: T, key: impl Fn(&T) -> &str) {
    match items
        .iter()
        .position(|existing| key(existing) == key(&item))
    {
        Some(i) => items[i] = item,
        None => items.push(item),
    }
}

/// The stored fields an import compares to decide whether a row needs rewriting.
#[derive(Debug, PartialEq, Eq)]
struct StoredState {
    content_hash: String,
    scope: String,
    source_path: String,
    stub_path: Option<String>,
}

impl StoredState {
    fn of(hash: &str, scope: &str, source_path: &str, stub_path: &Option<String>) -> Self {
        Self {
            content_hash: hash.to_string(),
            scope: crate::types::PolicyScope::parse(scope)
                .unwrap_or(crate::types::PolicyScope::Project)
                .as_str()
                .to_string(),
            source_path: source_path.to_string(),
            stub_path: stub_path.clone(),
        }
    }
}

async fn stored_state(
    conn: &mut sqlx::SqliteConnection,
    table: PruneTable,
) -> Result<std::collections::HashMap<String, StoredState>, AxError> {
    let (table_name, key) = table.names();
    let rows: Vec<(String, String, String, String, Option<String>)> = sqlx::query_as(&format!(
        "SELECT {key}, content_hash, COALESCE(scope, 'project'), source_path, stub_path FROM {table_name}"
    ))
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(rows
        .into_iter()
        .map(|(k, content_hash, scope, source_path, stub_path)| {
            (
                k,
                StoredState {
                    content_hash,
                    scope,
                    source_path,
                    stub_path,
                },
            )
        })
        .collect())
}

fn import_one_policy_dir(
    project_root: &Path,
    policy_dir: &Path,
    scope: crate::types::PolicyScope,
    preserve_item_scope: bool,
) -> LayerImport {
    let mut out = LayerImport::default();
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
            let raw = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    out.skip("rule", path, e.to_string());
                    continue;
                }
            };
            let mut doc = match parse_rule_file(path, &raw) {
                Ok(d) => d,
                Err(e) => {
                    out.skip_unparsed("rule", path, &raw, &e);
                    continue;
                }
            };
            if !preserve_item_scope {
                doc.frontmatter.scope = scope_s.clone();
            }
            if let Err(e) = materialize_rule_stub(project_root, &mut doc) {
                out.skip("rule stub", path, e);
                continue;
            }
            let hash = blake3::hash(doc.raw.as_bytes()).to_hex().to_string();
            out.rules.push((doc, hash));
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
            let raw = match std::fs::read_to_string(&skill_path) {
                Ok(s) => s,
                Err(e) => {
                    out.skip("skill", &skill_path, e.to_string());
                    continue;
                }
            };
            let mut doc = match parse_skill_file(&skill_path, &raw) {
                Ok(d) => d,
                Err(e) => {
                    out.skip_unparsed("skill", &skill_path, &raw, &e);
                    continue;
                }
            };
            if !preserve_item_scope {
                doc.frontmatter.scope = scope_s.clone();
            }
            if let Err(e) = materialize_skill_stub(project_root, &mut doc) {
                out.skip("skill stub", &skill_path, e);
                continue;
            }
            let hash = blake3::hash(doc.raw.as_bytes()).to_hex().to_string();
            out.skills.push((doc, hash));
        }
    }

    out
}

fn skip_file_owned(
    skipped: &mut Vec<crate::types::PolicySkip>,
    what: &str,
    path: &Path,
    reason: String,
) {
    eprintln!(
        "[ax policy] keep stored {what}, file-owned source {} unusable: {reason}",
        path.display()
    );
    skipped.push(crate::types::PolicySkip {
        path: path.to_string_lossy().into_owned(),
        reason,
        kept: true,
    });
}

/// What one policy layer contributed to an import: parsed docs with their content hash.
#[derive(Default)]
struct LayerImport {
    rules: Vec<(PolicyRuleDoc, String)>,
    skills: Vec<(PolicySkillDoc, String)>,
    skipped: Vec<crate::types::PolicySkip>,
    /// Skipped files whose stored row an import keeps: the file could not be
    /// read, its frontmatter is not YAML, or its stub source is unusable. A
    /// file that parses but fails validation is skipped without being kept.
    kept: Vec<String>,
    /// One log line per skipped file.
    notes: Vec<String>,
}

impl LayerImport {
    /// Record a file the layer could not use. The caller decides whether to
    /// report it, so a read-only staleness check stays quiet.
    fn skip(&mut self, what: &str, path: &Path, reason: String) {
        self.kept.push(path.to_string_lossy().into_owned());
        self.record_skip(what, path, reason, true);
    }

    /// Record a file that did not parse. The stored row is kept only when the
    /// frontmatter is not YAML at all (likely a typo mid-edit); a validation
    /// failure drops the row, so the strict parser's default applies.
    fn skip_unparsed(
        &mut self,
        what: &str,
        path: &Path,
        raw: &str,
        err: &crate::types::ValidationError,
    ) {
        let reason = crate::parse::format_validation(err);
        if frontmatter_is_yaml(raw) {
            self.skip_dropping_row(what, path, reason);
        } else {
            self.skip(what, path, reason);
        }
    }

    fn skip_dropping_row(&mut self, what: &str, path: &Path, reason: String) {
        self.record_skip(what, path, reason, false);
    }

    fn record_skip(&mut self, what: &str, path: &Path, reason: String, kept: bool) {
        let skip = crate::types::PolicySkip {
            path: path.to_string_lossy().into_owned(),
            reason,
            kept,
        };
        self.notes.push(format!(
            "[ax policy] skip {what} {} ({}): {}",
            path.display(),
            skip.outcome(),
            skip.reason
        ));
        self.skipped.push(skip);
    }
}

/// Whether `raw` has a `---` frontmatter block that parses as YAML.
fn frontmatter_is_yaml(raw: &str) -> bool {
    crate::parse::split_frontmatter(raw)
        .ok()
        .is_some_and(|(yaml, _)| serde_yaml::from_str::<serde_yaml::Value>(&yaml).is_ok())
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
    export_policy_to_files_filtered(pool, project_root, out_dir, true).await
}

/// When `git_export_only` is false, every DB rule/skill is written (exclusive files mode).
pub async fn export_policy_to_files_filtered(
    pool: &SqlitePool,
    project_root: &Path,
    out_dir: &Path,
    git_export_only: bool,
) -> Result<ExportResult, AxError> {
    let rules_out = out_dir.join("rules");
    let skills_out = out_dir.join("skills");
    std::fs::create_dir_all(&rules_out).map_err(|e| AxError::Other(e.to_string()))?;
    std::fs::create_dir_all(&skills_out).map_err(|e| AxError::Other(e.to_string()))?;

    let rules = list_rules(pool).await?;
    let skills = list_skills(pool).await?;

    let mut rules_n = 0u32;
    for row in &rules {
        let scope = crate::types::PolicyScope::parse(&row.scope)
            .unwrap_or(crate::types::PolicyScope::Project);
        if git_export_only && !crate::agents_share::is_git_export_candidate(scope, row.enabled) {
            continue;
        }
        let doc = rule_row_to_doc(row, project_root);
        if !is_single_component(&doc.frontmatter.id) {
            eprintln!(
                "[ax policy] export skips rule {:?}: the id is not a plain file name",
                doc.frontmatter.id
            );
            continue;
        }
        let path = rule_file(&rules_out, &doc.frontmatter.id);
        write_utf8(&path, &doc.raw)?;
        rules_n += 1;
    }

    let mut skills_n = 0u32;
    for row in &skills {
        let scope = crate::types::PolicyScope::parse(&row.scope)
            .unwrap_or(crate::types::PolicyScope::Project);
        if git_export_only && !crate::agents_share::is_git_export_candidate(scope, row.enabled) {
            continue;
        }
        let doc = skill_row_to_doc(row, project_root);
        if !is_single_component(&doc.frontmatter.name) {
            eprintln!(
                "[ax policy] export skips skill {:?}: the name is not a plain folder name",
                doc.frontmatter.name
            );
            continue;
        }
        let dir = skills_out.join(&doc.frontmatter.name);
        std::fs::create_dir_all(&dir).map_err(|e| AxError::Other(e.to_string()))?;
        let path = skill_file(&skills_out, &doc.frontmatter.name);
        write_utf8(&path, &doc.raw)?;
        skills_n += 1;
    }

    Ok(ExportResult {
        rules_exported: rules_n,
        skills_exported: skills_n,
        output_dir: out_dir.to_string_lossy().to_string(),
    })
}

/// `true` when `name` is exactly one normal path component (no separator, `.`, `..` or root).
fn is_single_component(name: &str) -> bool {
    let mut parts = Path::new(name).components();
    matches!(parts.next(), Some(std::path::Component::Normal(_)))
        && parts.next().is_none()
        && !name.contains(['/', '\\'])
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
        skipped: Vec::new(),
    })
}

fn policy_storage_label(storage: PolicyStorage) -> &'static str {
    match storage {
        PolicyStorage::Database => "database",
        PolicyStorage::Files => "files",
    }
}

fn policy_files_nonempty(project_root: &Path) -> bool {
    crate::hierarchy::policy_layers(project_root)
        .into_iter()
        .any(|layer| policy_dir_nonempty(&layer.dir))
}

fn policy_dir_nonempty(policy_dir: &Path) -> bool {
    let rules_path = policy_dir.join(crate::paths::RULES_DIR);
    let has_rules = rules_path
        .read_dir()
        .map(|d| {
            d.flatten()
                .any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("mdc"))
        })
        .unwrap_or(false);
    let skills_path = policy_dir.join(crate::paths::SKILLS_DIR);
    let has_skills = skills_path
        .read_dir()
        .map(|d| {
            d.flatten().any(|e| {
                e.path().is_dir()
                    && skill_file(&skills_path, &e.file_name().to_string_lossy()).is_file()
            })
        })
        .unwrap_or(false);
    has_rules || has_skills
}

/// Bootstrap an empty database from files; preserve database authority thereafter.
/// or refresh from disk (files mode). Safe to call on every MCP policy tool invocation.
pub async fn ensure_policy_ready(
    pool: &SqlitePool,
    project_root: &Path,
) -> Result<PolicyIndexResult, AxError> {
    let config = load_policy_config(project_root);
    match config.storage {
        PolicyStorage::Database => {
            let counts = db_counts(pool).await?;
            if counts.rules_indexed == 0
                && counts.skills_indexed == 0
                && policy_files_nonempty(project_root)
            {
                import_policy_from_files(pool, project_root, ImportMode::Merge).await
            } else {
                // Only rows explicitly authored in files may refresh in database mode.
                // A stale export of a database-owned row is never imported here.
                // A file that is missing, unreadable or renamed to another id keeps its stored
                // row and is reported in `skipped`; it never fails the whole call.
                let mut skipped = Vec::new();
                let (stored_rules, stored_skills) = {
                    let mut conn = pool
                        .acquire()
                        .await
                        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
                    (
                        stored_state(&mut conn, PruneTable::Rules).await?,
                        stored_state(&mut conn, PruneTable::Skills).await?,
                    )
                };
                for row in list_rules(pool).await? {
                    if effective_storage(config.storage, row.storage.as_deref())
                        != PolicyStorage::Files
                    {
                        continue;
                    }
                    let path = Path::new(&row.source_path);
                    let doc = std::fs::read_to_string(path)
                        .map_err(|e| e.to_string())
                        .and_then(|raw| {
                            parse_rule_file(path, &raw)
                                .map_err(|e| crate::parse::format_validation(&e))
                        })
                        .and_then(|mut doc| {
                            if doc.frontmatter.id != row.id {
                                return Err(format!(
                                    "file now holds rule `{}` instead of `{}`; re-index policy",
                                    doc.frontmatter.id, row.id
                                ));
                            }
                            doc.frontmatter.scope = row.scope.clone();
                            materialize_rule_stub(project_root, &mut doc)?;
                            Ok(doc)
                        });
                    let doc = match doc {
                        Ok(doc) => doc,
                        Err(reason) => {
                            skip_file_owned(&mut skipped, "rule", path, reason);
                            continue;
                        }
                    };
                    let hash = blake3::hash(doc.raw.as_bytes()).to_hex().to_string();
                    let previous = stored_rules.get(&row.id).map(|s| s.content_hash.as_str());
                    if previous != Some(hash.as_str()) {
                        upsert_rule(pool, &doc, &hash, now_ms()).await?;
                    }
                }
                for row in list_skills(pool).await? {
                    if effective_storage(config.storage, row.storage.as_deref())
                        != PolicyStorage::Files
                    {
                        continue;
                    }
                    let path = Path::new(&row.source_path);
                    let doc = std::fs::read_to_string(path)
                        .map_err(|e| e.to_string())
                        .and_then(|raw| {
                            parse_skill_file(path, &raw)
                                .map_err(|e| crate::parse::format_validation(&e))
                        })
                        .and_then(|mut doc| {
                            if doc.frontmatter.name != row.name {
                                return Err(format!(
                                    "file now holds skill `{}` instead of `{}`; re-index policy",
                                    doc.frontmatter.name, row.name
                                ));
                            }
                            doc.frontmatter.scope = row.scope.clone();
                            materialize_skill_stub(project_root, &mut doc)?;
                            Ok(doc)
                        });
                    let doc = match doc {
                        Ok(doc) => doc,
                        Err(reason) => {
                            skip_file_owned(&mut skipped, "skill", path, reason);
                            continue;
                        }
                    };
                    let hash = blake3::hash(doc.raw.as_bytes()).to_hex().to_string();
                    let previous = stored_skills
                        .get(&row.name)
                        .map(|s| s.content_hash.as_str());
                    if previous != Some(hash.as_str()) {
                        upsert_skill(pool, &doc, &hash, now_ms()).await?;
                    }
                }
                Ok(PolicyIndexResult { skipped, ..counts })
            }
        }
        PolicyStorage::Files => {
            import_policy_from_files(pool, project_root, ImportMode::Replace).await
        }
    }
}

/// Whether an import from the policy files would change the database: a winning
/// file differs from its row, or a file-backed row has no file any more.
///
/// The files are merged across layers exactly as an import merges them (the last
/// layer wins per id, stubs are resolved), so a shadowed lower-layer file or a
/// stub never reads as changed. Stored state is loaded with one query per table.
pub async fn policy_disk_stale(pool: &SqlitePool, project_root: &Path) -> Result<bool, AxError> {
    let merged = merge_policy_layers(project_root);
    let storage = load_policy_config(project_root).storage;
    let kept_paths: std::collections::HashSet<&str> =
        merged.kept_paths.iter().map(String::as_str).collect();

    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    let stored_rules = stored_rows(&mut conn, PruneTable::Rules).await?;
    let stored_skills = stored_rows(&mut conn, PruneTable::Skills).await?;

    let rules = merged.rules.iter().map(|(doc, hash)| {
        (
            doc.frontmatter.id.as_str(),
            StoredState::of(
                hash,
                &doc.frontmatter.scope,
                &doc.source_path,
                &doc.stub_path,
            ),
            &doc.frontmatter.properties,
        )
    });
    let skills = merged.skills.iter().map(|(doc, hash)| {
        (
            doc.frontmatter.name.as_str(),
            StoredState::of(
                hash,
                &doc.frontmatter.scope,
                &doc.source_path,
                &doc.stub_path,
            ),
            &doc.frontmatter.properties,
        )
    });
    Ok(table_stale(rules, &stored_rules, storage, &kept_paths)
        || table_stale(skills, &stored_skills, storage, &kept_paths))
}

/// One stored row as the staleness check needs it.
struct StoredRow {
    state: StoredState,
    storage: Option<String>,
    properties: crate::types::PolicyProperties,
}

async fn stored_rows(
    conn: &mut sqlx::SqliteConnection,
    table: PruneTable,
) -> Result<std::collections::HashMap<String, StoredRow>, AxError> {
    type Row = (
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let (table_name, key) = table.names();
    let rows: Vec<Row> = sqlx::query_as(&format!(
        "SELECT {key}, content_hash, COALESCE(scope, 'project'), source_path, stub_path, storage, properties FROM {table_name}"
    ))
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(rows
        .into_iter()
        .map(
            |(k, content_hash, scope, source_path, stub_path, storage, properties)| {
                (
                    k,
                    StoredRow {
                        state: StoredState {
                            content_hash,
                            scope,
                            source_path,
                            stub_path,
                        },
                        storage,
                        properties: properties
                            .as_deref()
                            .map(parse_properties)
                            .unwrap_or_default(),
                    },
                )
            },
        )
        .collect())
}

fn table_stale<'a>(
    on_disk: impl Iterator<Item = (&'a str, StoredState, &'a crate::types::PolicyProperties)>,
    stored: &std::collections::HashMap<String, StoredRow>,
    project_storage: PolicyStorage,
    kept_paths: &std::collections::HashSet<&str>,
) -> bool {
    let mut seen = std::collections::HashSet::new();
    for (id, state, properties) in on_disk {
        seen.insert(id);
        let Some(row) = stored.get(id) else {
            return true;
        };
        if row.state != state {
            return true;
        }
        if !properties.is_empty() && row.properties != *properties {
            return true;
        }
    }
    // A file-backed row whose file is gone (and not merely unreadable) would be pruned.
    stored.iter().any(|(id, row)| {
        !seen.contains(id.as_str())
            && effective_storage(project_storage, row.storage.as_deref()) == PolicyStorage::Files
            && !kept_paths.contains(row.state.source_path.as_str())
            && !row
                .state
                .stub_path
                .as_deref()
                .is_some_and(|p| kept_paths.contains(p))
    })
}

/// Policy counts and storage mode for status / diagnostics.
pub async fn policy_status(
    pool: &SqlitePool,
    project_root: &Path,
) -> Result<crate::types::PolicyStatus, AxError> {
    let counts = db_counts(pool).await?;
    let config = load_policy_config(project_root);
    Ok(crate::types::PolicyStatus {
        indexed: counts.rules_indexed > 0 || counts.skills_indexed > 0,
        rules: counts.rules_indexed,
        skills: counts.skills_indexed,
        mode: policy_storage_label(config.storage).to_string(),
    })
}

async fn upsert_rule<'e>(
    exec: impl sqlx::SqliteExecutor<'e>,
    doc: &PolicyRuleDoc,
    hash: &str,
    now: i64,
) -> Result<(), AxError> {
    let fm = &doc.frontmatter;
    let scope = crate::types::PolicyScope::parse(&fm.scope)
        .unwrap_or(crate::types::PolicyScope::Project)
        .as_str();
    sqlx::query(
        "INSERT INTO policy_rules (id, level, always_apply, globs, triggers, tags, priority, body, source_path, content_hash, updated_at, enabled, status, scope, storage, source, root_id, stub_path, skill_group, properties)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           level=excluded.level, always_apply=excluded.always_apply, globs=excluded.globs,
           triggers=excluded.triggers, tags=excluded.tags, priority=excluded.priority,
           body=excluded.body, source_path=excluded.source_path, content_hash=excluded.content_hash,
           updated_at=excluded.updated_at, enabled=excluded.enabled, status=excluded.status,
           scope=excluded.scope, storage=excluded.storage, source=excluded.source,
           root_id=excluded.root_id, stub_path=excluded.stub_path, skill_group=excluded.skill_group,
           properties=excluded.properties",
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
    .bind(&fm.group)
    .bind(properties_json(&fm.properties))
    .execute(exec)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(())
}

fn properties_json(properties: &crate::types::PolicyProperties) -> String {
    serde_json::to_string(properties).unwrap_or_else(|_| "{}".into())
}

fn parse_properties(raw: &str) -> crate::types::PolicyProperties {
    serde_json::from_str(raw).unwrap_or_default()
}

async fn upsert_skill<'e>(
    exec: impl sqlx::SqliteExecutor<'e>,
    doc: &PolicySkillDoc,
    hash: &str,
    now: i64,
) -> Result<(), AxError> {
    let fm = &doc.frontmatter;
    let scope = crate::types::PolicyScope::parse(&fm.scope)
        .unwrap_or(crate::types::PolicyScope::Project)
        .as_str();
    sqlx::query(
        "INSERT INTO policy_skills (name, description, always_apply, triggers, tags, priority, context_task, body, source_path, content_hash, updated_at, enabled, status, scope, storage, source, root_id, stub_path, skill_group, properties)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(name) DO UPDATE SET
           description=excluded.description, always_apply=excluded.always_apply, triggers=excluded.triggers, tags=excluded.tags,
           priority=excluded.priority, context_task=excluded.context_task, body=excluded.body,
           source_path=excluded.source_path, content_hash=excluded.content_hash, updated_at=excluded.updated_at,
           enabled=excluded.enabled, status=excluded.status, scope=excluded.scope,
           storage=excluded.storage, source=excluded.source, root_id=excluded.root_id,
           stub_path=excluded.stub_path, skill_group=excluded.skill_group,
           properties=excluded.properties",
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
    .bind(&fm.group)
    .bind(properties_json(&fm.properties))
    .execute(exec)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(())
}

#[derive(Clone, Copy)]
enum PruneTable {
    Rules,
    Skills,
}

impl PruneTable {
    fn names(self) -> (&'static str, &'static str) {
        match self {
            PruneTable::Rules => ("policy_rules", "id"),
            PruneTable::Skills => ("policy_skills", "name"),
        }
    }
}

/// Delete file-backed rows not on disk; keep `storage = 'database'` overrides and rows whose
/// source file (or stub) is in `kept_paths` because it exists but could not be parsed.
async fn prune_hybrid(
    conn: &mut sqlx::SqliteConnection,
    table: PruneTable,
    keep: &[String],
    kept_paths: &[String],
) -> Result<(), AxError> {
    let (table_name, key) = table.names();
    let placeholders = |n: usize| vec!["?"; n].join(",");
    let mut sql = format!(
        "DELETE FROM {table_name} WHERE COALESCE(NULLIF(storage, ''), 'files') != 'database'"
    );
    if !keep.is_empty() {
        sql.push_str(&format!(" AND {key} NOT IN ({})", placeholders(keep.len())));
    }
    if !kept_paths.is_empty() {
        let p = placeholders(kept_paths.len());
        sql.push_str(&format!(
            " AND source_path NOT IN ({p}) AND COALESCE(stub_path, '') NOT IN ({p})"
        ));
    }
    let mut q = sqlx::query(&sql);
    for id in keep {
        q = q.bind(id);
    }
    for _ in 0..2 {
        for path in kept_paths {
            q = q.bind(path);
        }
    }
    q.execute(&mut *conn)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(())
}

const RULE_SELECT: &str = "SELECT id, level, always_apply, globs, triggers, tags, priority, body, source_path,
                COALESCE(enabled, 1) as enabled, COALESCE(status, 'approved') as status,
                COALESCE(scope, 'project') as scope, storage, source, root_id, stub_path, skill_group,
                COALESCE(properties, '{}') as properties
         FROM policy_rules";

const SKILL_SELECT: &str = "SELECT name, description, COALESCE(always_apply, 0) as always_apply, triggers, tags, priority, context_task, body, source_path,
                COALESCE(enabled, 1) as enabled, COALESCE(status, 'approved') as status,
                COALESCE(scope, 'project') as scope, storage, source, root_id, stub_path, skill_group,
                COALESCE(properties, '{}') as properties
         FROM policy_skills";

pub async fn list_rules(pool: &SqlitePool) -> Result<Vec<PolicyRuleRow>, AxError> {
    let rows =
        sqlx::query_as::<_, RuleDbRow>(&format!("{RULE_SELECT} ORDER BY id COLLATE NOCASE ASC"))
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
    let stored = row.group.trim();
    row.group = crate::skill_groups::resolve_skill_group(
        if stored.is_empty() {
            None
        } else {
            Some(stored)
        },
        &row.id,
        &row.tags,
    );
}

pub fn enrich_skill_row(row: &mut PolicySkillRow, default: PolicyStorage) {
    let eff = effective_storage(default, row.storage.as_deref());
    row.effective_storage = eff.as_str().into();
    row.storage_is_override = row.storage.is_some();
    let stored = row.group.trim();
    row.group = crate::skill_groups::resolve_skill_group(
        if stored.is_empty() {
            None
        } else {
            Some(stored)
        },
        &row.name,
        &row.tags,
    );
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
    let agents = crate::agents_share::agents_dir(project_root);
    let ax_dir = ax_dir_from_project(project_root);
    agents.join(crate::paths::RULES_DIR).exists()
        || agents.join(crate::paths::SKILLS_DIR).exists()
        || rules_dir(&ax_dir).exists()
        || skills_dir(&ax_dir).exists()
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
        group: if row.group.trim().is_empty() {
            None
        } else {
            Some(row.group.clone())
        },
        properties: row.properties.clone(),
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
        group: if row.group.trim().is_empty() {
            None
        } else {
            Some(row.group.clone())
        },
        properties: row.properties.clone(),
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
    skill_group: Option<String>,
    properties: String,
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
            group: self
                .skill_group
                .filter(|s| !s.is_empty())
                .unwrap_or_default(),
            properties: parse_properties(&self.properties),
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
    skill_group: Option<String>,
    properties: String,
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
            group: self
                .skill_group
                .filter(|s| !s.is_empty())
                .unwrap_or_default(),
            properties: parse_properties(&self.properties),
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

    /// A database with the real migrations, so the tests see the real schema.
    async fn test_pool() -> (TempDir, SqlitePool) {
        let dir = TempDir::new().unwrap();
        let db = ax_db::Database::open(&dir.path().join("test.db"))
            .await
            .unwrap();
        let pool = db.pool().clone();
        (dir, pool)
    }

    #[tokio::test]
    async fn database_mode_save_without_files() {
        let (_dir, pool) = test_pool().await;
        let root = _dir.path();
        std::fs::write(root.join("ax.json"), r#"{"policy":{"storage":"database"}}"#).unwrap();

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
            group: None,
            properties: Default::default(),
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
    async fn properties_survive_database_roundtrip() {
        let (dir, pool) = test_pool().await;
        let raw = "---\nid: demo\nlevel: INFO\nalwaysApply: true\nowner: platform\nfiles: [\"src/a.rs\"]\n---\n\nBody.\n";
        let doc = parse_rule_file(Path::new("demo.mdc"), raw).unwrap();
        upsert_rule_doc(&pool, &doc).await.unwrap();
        let rows = list_rules(&pool).await.unwrap();
        let row = rows.iter().find(|r| r.id == "demo").unwrap();
        assert_eq!(row.properties["owner"], "platform");
        assert_eq!(row.properties["files"], serde_json::json!(["src/a.rs"]));
        let back = rule_row_to_doc(row, dir.path());
        assert!(back.raw.contains("owner: platform"), "{}", back.raw);

        let skill_raw = "---\nname: demo-skill\ndescription: d\nkind: review\n---\n\nSteps.\n";
        let sdoc = parse_skill_file(Path::new("SKILL.md"), skill_raw).unwrap();
        upsert_skill_doc(&pool, &sdoc).await.unwrap();
        let skills = list_skills(&pool).await.unwrap();
        let skill = skills.iter().find(|s| s.name == "demo-skill").unwrap();
        assert_eq!(skill.properties["kind"], "review");
        let skill_doc = skill_row_to_doc(skill, dir.path());
        assert!(skill_doc.raw.contains("kind: review"), "{}", skill_doc.raw);
    }

    #[tokio::test]
    async fn import_skips_unparseable_cursor_files() {
        let (_dir, pool) = test_pool().await;
        let root = _dir.path();
        std::fs::create_dir_all(root.join(".agents/rules")).unwrap();
        std::fs::write(
            root.join(".agents/rules/no-ab-prefix.mdc"),
            "---\ndescription: Cursor rule\nalwaysApply: true\n---\n\nNo AB prefix.\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join(".agents/skills/contoso-pr")).unwrap();
        std::fs::write(
            root.join(".agents/skills/contoso-pr/SKILL.md"),
            "---\nname: contoso-pr\n---\n\nPR workflow.\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".agents/rules/ok.mdc"),
            "---\nid: ok\nlevel: INFO\nalwaysApply: true\nscope: project\n---\n\nax body\n",
        )
        .unwrap();

        let result = import_policy_from_files(&pool, root, ImportMode::Replace)
            .await
            .expect("cursor-native neighbors must not fail index");
        assert!(result.rules_indexed >= 1);
        let rules = list_rules(&pool).await.unwrap();
        assert!(rules.iter().any(|r| r.id == "ok"));
        assert!(
            !rules
                .iter()
                .any(|r| r.id.contains("no-ab-prefix") || r.id == "contoso-pr"),
            "cursor-native files must not be upserted: {:?}",
            rules.iter().map(|r| r.id.as_str()).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn database_authority_never_auto_imports_a_stale_disk_copy() {
        let (dir, pool) = test_pool().await;
        let root = dir.path();
        std::fs::write(root.join("ax.json"), r#"{"policy":{"storage":"database"}}"#).unwrap();
        let file = root.join(".agents/rules/authority.mdc");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            "---\nid: authority\nlevel: INFO\nalwaysApply: true\n---\nnew database body\n",
        )
        .unwrap();
        ensure_policy_ready(&pool, root).await.unwrap();
        std::fs::write(
            &file,
            "---\nid: authority\nlevel: INFO\nalwaysApply: true\n---\nstale disk body\n",
        )
        .unwrap();
        ensure_policy_ready(&pool, root).await.unwrap();
        assert_eq!(
            get_rule(&pool, "authority").await.unwrap().unwrap().body,
            "new database body"
        );
    }

    #[tokio::test]
    async fn ensure_policy_ready_skips_cursor_files_when_checking_stale() {
        let (_dir, pool) = test_pool().await;
        let root = _dir.path();
        std::fs::write(root.join("ax.json"), r#"{"policy":{"storage":"database"}}"#).unwrap();
        std::fs::create_dir_all(root.join(".agents/rules")).unwrap();
        std::fs::write(
            root.join(".agents/rules/ok.mdc"),
            "---\nid: ok\nlevel: INFO\nalwaysApply: true\nscope: project\n---\n\nax body\n",
        )
        .unwrap();
        ensure_policy_ready(&pool, root)
            .await
            .expect("first import of valid ax rule");

        std::fs::write(
            root.join(".agents/rules/no-ab-prefix.mdc"),
            "---\ndescription: Cursor rule\nalwaysApply: true\n---\n\nNo AB prefix.\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join(".agents/skills/contoso-pr")).unwrap();
        std::fs::write(
            root.join(".agents/skills/contoso-pr/SKILL.md"),
            "---\nname: contoso-pr\n---\n\nPR workflow.\n",
        )
        .unwrap();

        ensure_policy_ready(&pool, root)
            .await
            .expect("stale check must skip Cursor-native files (MCP session / ax_preflight)");
        let rules = list_rules(&pool).await.unwrap();
        assert!(rules.iter().any(|r| r.id == "ok"));
    }

    #[tokio::test]
    async fn restore_then_index_with_cursor_native_neighbor() {
        let (_dir, pool) = test_pool().await;
        let dest = _dir.path();
        std::fs::create_dir_all(dest.join(".agents/rules")).unwrap();
        std::fs::write(
            dest.join(".agents/rules/no-ab-prefix.mdc"),
            "---\ndescription: Cursor rule\nalwaysApply: true\n---\n\nNo AB prefix.\n",
        )
        .unwrap();

        let src = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(src.path().join(".agents/rules")).unwrap();
        std::fs::write(
            src.path().join(".agents/rules/packed.mdc"),
            "---\nid: packed\nlevel: INFO\nalwaysApply: true\nenabled: true\nscope: project\n---\n\nfrom zip\n",
        )
        .unwrap();
        let zip = crate::zip_package::build_policy_zip(
            src.path(),
            &crate::zip_package::PackSpec {
                name: "t".into(),
                description: "d".into(),
                rule_ids: vec!["packed".into()],
                skill_names: vec![],
                ax_version: "4.8.0".into(),
                package_version: None,
                author: None,
            },
        )
        .unwrap();
        crate::zip_package::restore_policy_zip(dest, &zip, &std::collections::HashMap::new())
            .unwrap();

        let result = import_policy_from_files(&pool, dest, ImportMode::Replace)
            .await
            .expect("restore + index must succeed with cursor-native neighbor");
        assert!(result.rules_indexed >= 1);
        let rules = list_rules(&pool).await.unwrap();
        assert!(rules.iter().any(|r| r.id == "packed"));
    }

    #[tokio::test]
    async fn ensure_policy_ready_imports_when_db_empty() {
        let (_dir, pool) = test_pool().await;
        let root = _dir.path();
        std::fs::write(root.join("ax.json"), r#"{"policy":{"storage":"database"}}"#).unwrap();
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
    async fn a_missing_file_owned_source_does_not_fail_database_mode() {
        let (dir, pool) = test_pool().await;
        let root = dir.path();
        std::fs::write(root.join("ax.json"), r#"{"policy":{"storage":"database"}}"#).unwrap();
        std::fs::create_dir_all(root.join(".agents/rules")).unwrap();
        let file_rule = root.join(".agents/rules/file-owned.mdc");
        std::fs::write(
            &file_rule,
            "---\nid: file-owned\nlevel: INFO\nalwaysApply: true\nstorage: files\n---\n\nfrom a file\n",
        )
        .unwrap();
        write_skill(root, "kept-skill", "skill body");
        let skill_file_path = root.join(".agents/skills/kept-skill/SKILL.md");
        std::fs::write(
            &skill_file_path,
            "---\nname: kept-skill\ndescription: d\nstorage: files\n---\n\nskill body\n",
        )
        .unwrap();
        write_rule(root, "db-owned", "database row");
        ensure_policy_ready(&pool, root).await.unwrap();
        assert!(list_rules(&pool)
            .await
            .unwrap()
            .iter()
            .any(|r| r.id == "file-owned"));

        std::fs::remove_file(&file_rule).unwrap();
        std::fs::write(&skill_file_path, "---\nname: [broken\n---\n").unwrap();
        let ready = ensure_policy_ready(&pool, root)
            .await
            .expect("one missing file must not fail the whole call");
        assert!(ready
            .skipped
            .iter()
            .any(|s| s.path.ends_with("file-owned.mdc")));
        assert!(ready.skipped.iter().any(|s| s.path.ends_with("SKILL.md")));
        let rules = list_rules(&pool).await.unwrap();
        assert!(rules.iter().any(|r| r.id == "file-owned"), "row is kept");
        assert!(rules.iter().any(|r| r.id == "db-owned"));
        assert!(list_skills(&pool)
            .await
            .unwrap()
            .iter()
            .any(|s| s.name == "kept-skill" && s.body.contains("skill body")));
    }

    #[tokio::test]
    async fn export_refuses_a_skill_name_that_leaves_the_output_folder() {
        let (dir, pool) = test_pool().await;
        let root = dir.path().join("project");
        std::fs::create_dir_all(&root).unwrap();
        let raw = "---\nname: placeholder\ndescription: d\n---\n\nEvil.\n";
        let mut doc = parse_skill_file(Path::new("SKILL.md"), raw).unwrap();
        doc.frontmatter.name = "../../escaped".into();
        upsert_skill_doc(&pool, &doc).await.unwrap();
        let mut good = parse_skill_file(Path::new("SKILL.md"), raw).unwrap();
        good.frontmatter.name = "fine".into();
        upsert_skill_doc(&pool, &good).await.unwrap();

        let out = root.join("out");
        let result = export_policy_to_files_filtered(&pool, &root, &out, false)
            .await
            .unwrap();
        assert!(
            !dir.path().join("escaped").exists(),
            "wrote outside out_dir"
        );
        assert!(!root.join("escaped").exists(), "wrote outside out_dir");
        assert!(out.join("skills/fine/SKILL.md").is_file());
        assert_eq!(result.skills_exported, 1);
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

    async fn global_db_with(path: &Path, rows: &[(&str, &str, &str, &str)]) {
        let pool = SqlitePoolOptions::new()
            .connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(path)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        for table in ["global_policy_rules", "global_policy_skills"] {
            sqlx::query(&format!(
                "CREATE TABLE {table} (project_id INTEGER NOT NULL, item_id TEXT NOT NULL,
                    payload TEXT NOT NULL, synced_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                    level TEXT NOT NULL DEFAULT 'global', PRIMARY KEY (project_id, item_id))"
            ))
            .execute(&pool)
            .await
            .unwrap();
        }
        for (table, name, body, level) in rows {
            sqlx::query(&format!(
                "INSERT INTO {table} (project_id, item_id, payload, level) VALUES (1, ?, ?, ?)"
            ))
            .bind(name)
            .bind(serde_json::json!({ "body": body }).to_string())
            .bind(level)
            .execute(&pool)
            .await
            .unwrap();
        }
        pool.close().await;
    }

    fn write_skill(root: &Path, name: &str, body: &str) {
        let dir = root.join(".agents/skills").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: d\n---\n\n{body}\n"),
        )
        .unwrap();
    }

    fn write_rule(root: &Path, id: &str, body: &str) {
        std::fs::create_dir_all(root.join(".agents/rules")).unwrap();
        std::fs::write(
            root.join(".agents/rules").join(format!("{id}.mdc")),
            format!("---\nid: {id}\nlevel: INFO\nalwaysApply: true\n---\n\n{body}\n"),
        )
        .unwrap();
    }

    fn fixture_policy(root: &Path) {
        write_skill(root, "noti", "short");
        write_skill(
            root,
            "extra",
            "a project copy that is longer than the global one",
        );
        write_skill(root, "solo", "only here");
        write_rule(root, "utf8", "short");
        write_rule(
            root,
            "longrule",
            "a project rule body longer than the global one",
        );
    }

    #[tokio::test]
    async fn import_preserves_distinct_project_policy_despite_global_name_collision() {
        let (dir, pool) = test_pool().await;
        let root = dir.path();
        let global = root.join("global.db");
        global_db_with(
            &global,
            &[
                (
                    "global_policy_skills",
                    "noti",
                    "the global copy is the longer one",
                    "global",
                ),
                ("global_policy_skills", "extra", "short global", "global"),
                (
                    "global_policy_skills",
                    "solo",
                    "a mirror only, never shadows a project copy",
                    "mirror",
                ),
                (
                    "global_policy_rules",
                    "utf8",
                    "the global rule is the longer one",
                    "global",
                ),
                ("global_policy_rules", "longrule", "short", "global"),
            ],
        )
        .await;
        fixture_policy(root);
        crate::global_level::set_test_global_db(Some(global));

        import_policy_from_files(&pool, root, ImportMode::Replace)
            .await
            .unwrap();

        let skills: Vec<String> = list_skills(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .collect();
        let rules: Vec<String> = list_rules(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        crate::global_level::set_test_global_db(None);
        assert!(skills.contains(&"noti".to_string()), "{skills:?}");
        assert!(skills.contains(&"extra".to_string()), "{skills:?}");
        assert!(skills.contains(&"solo".to_string()), "{skills:?}");
        assert!(rules.contains(&"utf8".to_string()), "{rules:?}");
        assert!(rules.contains(&"longrule".to_string()), "{rules:?}");
        assert!(root.join(".agents/skills/noti/SKILL.md").is_file());
    }

    #[tokio::test]
    async fn a_rule_that_stops_parsing_keeps_its_row_and_is_reported() {
        let (dir, pool) = test_pool().await;
        let root = dir.path();
        write_rule(root, "secrets", "Never commit secrets.");
        write_skill(root, "review", "Review steps.");
        import_policy_from_files(&pool, root, ImportMode::Replace)
            .await
            .unwrap();

        // A YAML typo in the frontmatter of both files.
        std::fs::write(
            root.join(".agents/rules/secrets.mdc"),
            "---\nid: secrets\nlevel: [CRITICAL\n---\n\nNever commit secrets.\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".agents/skills/review/SKILL.md"),
            "---\nname: review\ndescription: [d\n---\n\nReview steps.\n",
        )
        .unwrap();
        let result = import_policy_from_files(&pool, root, ImportMode::Replace)
            .await
            .unwrap();

        let rules = list_rules(&pool).await.unwrap();
        assert!(
            rules
                .iter()
                .any(|r| r.id == "secrets" && r.body.contains("Never commit")),
            "the previous row must survive a parse error: {rules:?}"
        );
        let skills = list_skills(&pool).await.unwrap();
        assert!(skills.iter().any(|s| s.name == "review"), "{skills:?}");
        // Only count files in this project; the user's global layers are not under test.
        let here = |r: &PolicyIndexResult| {
            r.skipped
                .iter()
                .filter(|s| Path::new(&s.path).starts_with(root))
                .count()
        };
        assert_eq!(here(&result), 2, "{:?}", result.skipped);
        assert!(result
            .skipped
            .iter()
            .any(|s| s.path.ends_with("secrets.mdc") && !s.reason.is_empty() && s.kept));
        assert!(result
            .skipped
            .iter()
            .any(|s| s.path.ends_with("SKILL.md") && !s.reason.is_empty() && s.kept));

        // Once the broken file is deleted, its row goes as before.
        std::fs::remove_file(root.join(".agents/rules/secrets.mdc")).unwrap();
        let result = import_policy_from_files(&pool, root, ImportMode::Replace)
            .await
            .unwrap();
        assert_eq!(here(&result), 1, "{:?}", result.skipped);
        assert!(!list_rules(&pool)
            .await
            .unwrap()
            .iter()
            .any(|r| r.id == "secrets"));
        assert!(list_skills(&pool)
            .await
            .unwrap()
            .iter()
            .any(|s| s.name == "review"));
    }

    #[tokio::test]
    async fn a_file_that_parses_but_fails_validation_loses_its_row() {
        let (dir, pool) = test_pool().await;
        let root = dir.path();
        write_rule(root, "typo-status", "Keep this pending.");
        write_skill(root, "review", "Review steps.");
        import_policy_from_files(&pool, root, ImportMode::Replace)
            .await
            .unwrap();
        // The rows an older parser stored: approved and matched.
        let rules = list_rules(&pool).await.unwrap();
        assert!(rules
            .iter()
            .any(|r| r.id == "typo-status" && r.status == "approved"));

        // Valid YAML that fails validation: an unknown status, a non-kebab name.
        std::fs::write(
            root.join(".agents/rules/typo-status.mdc"),
            "---\nid: typo-status\nlevel: INFO\nalwaysApply: true\nstatus: pendng\n---\n\nKeep this pending.\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".agents/skills/review/SKILL.md"),
            "---\nname: My_Skill\ndescription: d\n---\n\nReview steps.\n",
        )
        .unwrap();
        assert!(policy_disk_stale(&pool, root).await.unwrap());
        let result = import_policy_from_files(&pool, root, ImportMode::Replace)
            .await
            .unwrap();
        assert!(result
            .skipped
            .iter()
            .any(|s| s.path.ends_with("typo-status.mdc")
                && s.reason.contains("status")
                && !s.kept));

        let rules = list_rules(&pool).await.unwrap();
        assert!(
            !rules.iter().any(|r| r.id == "typo-status"),
            "an invalid rule must not stay approved: {rules:?}"
        );
        let skills = list_skills(&pool).await.unwrap();
        assert!(
            !skills
                .iter()
                .any(|s| s.name == "review" || s.name == "My_Skill"),
            "{skills:?}"
        );
        let matched = crate::matcher::match_policy(
            &pool,
            &crate::types::MatchInput {
                prompt: "keep this pending typo-status".into(),
                cwd: root.to_path_buf(),
                open_files: Vec::new(),
                changed_files: Vec::new(),
            },
        )
        .await
        .unwrap();
        assert!(
            !matched.rules.iter().any(|r| r.id == "typo-status"),
            "the matcher must not return it"
        );
        assert!(!policy_disk_stale(&pool, root).await.unwrap());
    }

    async fn row_stamps(pool: &SqlitePool) -> Vec<(String, i64)> {
        sqlx::query_as(
            "SELECT id, updated_at FROM policy_rules
             UNION ALL SELECT name, updated_at FROM policy_skills ORDER BY 1",
        )
        .fetch_all(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn an_unchanged_import_writes_nothing() {
        let (dir, pool) = test_pool().await;
        let root = dir.path();
        fixture_policy(root);
        // A rule with the same id in a second layer must not flip back and forth either.
        let private = root.join(".ax/policy-private");
        std::fs::create_dir_all(private.join("rules")).unwrap();
        std::fs::write(
            private.join("rules/longrule.mdc"),
            "---\nid: longrule\nlevel: INFO\nalwaysApply: true\n---\n\nthe private copy\n",
        )
        .unwrap();
        import_policy_from_files(&pool, root, ImportMode::Replace)
            .await
            .unwrap();
        let before = row_stamps(&pool).await;
        let generation = policy_generation_for_test(&pool).await;
        std::thread::sleep(std::time::Duration::from_millis(5));
        import_policy_from_files(&pool, root, ImportMode::Replace)
            .await
            .unwrap();
        assert_eq!(row_stamps(&pool).await, before);
        assert_eq!(policy_generation_for_test(&pool).await, generation);

        // A real edit is still written.
        write_rule(root, "utf8", "an edited body");
        import_policy_from_files(&pool, root, ImportMode::Replace)
            .await
            .unwrap();
        let rules = list_rules(&pool).await.unwrap();
        let row = rules.iter().find(|r| r.id == "utf8").unwrap();
        assert!(row.body.contains("an edited body"), "{}", row.body);
    }

    async fn policy_generation_for_test(pool: &SqlitePool) -> String {
        sqlx::query_scalar(
            "SELECT group_concat(id || content_hash || updated_at, '|') FROM policy_rules",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn import_without_global_db_keeps_everything() {
        let (dir, pool) = test_pool().await;
        let root = dir.path();
        fixture_policy(root);
        crate::global_level::set_test_global_db(Some(root.join("missing-global.db")));

        import_policy_from_files(&pool, root, ImportMode::Replace)
            .await
            .unwrap();

        let skills: Vec<String> = list_skills(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .collect();
        let rules: Vec<String> = list_rules(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        crate::global_level::set_test_global_db(None);
        for name in ["noti", "extra", "solo"] {
            assert!(
                skills.contains(&name.to_string()),
                "{name} missing: {skills:?}"
            );
        }
        for id in ["utf8", "longrule"] {
            assert!(rules.contains(&id.to_string()), "{id} missing: {rules:?}");
        }
        assert!(!root.join("missing-global.db").exists());
    }

    fn write_file(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn rule_text(id: &str, body: &str) -> String {
        format!("---\nid: {id}\nlevel: INFO\nalwaysApply: true\n---\n\n{body}\n")
    }

    #[tokio::test]
    async fn disk_stale_is_false_right_after_import_with_a_shadowed_id() {
        let (dir, pool) = test_pool().await;
        let root = dir.path().join("project");
        let company = dir.path().join("company");
        write_file(
            &company.join("rules/shared.mdc"),
            &rule_text("shared", "company text"),
        );
        write_file(
            &root.join(".agents/rules/shared.mdc"),
            &rule_text("shared", "project text"),
        );
        write_file(
            &root.join("ax.json"),
            &format!(
                r#"{{"policy":{{"roots":[{{"id":"co","path":"{}","scope":"company"}}]}}}}"#,
                company.display().to_string().replace('\\', "/")
            ),
        );
        import_policy_from_files(&pool, &root, ImportMode::Replace)
            .await
            .unwrap();
        assert!(!policy_disk_stale(&pool, &root).await.unwrap());

        // Editing the shadowed (losing) file changes nothing an import would write.
        write_file(
            &company.join("rules/shared.mdc"),
            &rule_text("shared", "company text, edited"),
        );
        assert!(!policy_disk_stale(&pool, &root).await.unwrap());

        // Editing the winning file does.
        write_file(
            &root.join(".agents/rules/shared.mdc"),
            &rule_text("shared", "project text, edited"),
        );
        assert!(policy_disk_stale(&pool, &root).await.unwrap());
    }

    #[tokio::test]
    async fn disk_stale_is_false_right_after_import_of_a_stub() {
        let (dir, pool) = test_pool().await;
        let root = dir.path();
        write_file(
            &root.join("shared/rules/body.mdc"),
            &rule_text("body", "external body"),
        );
        write_file(
            &root.join(".agents/rules/stub.mdc"),
            &format!(
                "---\nid: stub\nlevel: INFO\nalwaysApply: true\nsource: shared/rules/body.mdc\n---\n\n{}\n",
                crate::paths::STUB_BODY_MARKER
            ),
        );
        import_policy_from_files(&pool, root, ImportMode::Replace)
            .await
            .unwrap();
        assert!(list_rules(&pool)
            .await
            .unwrap()
            .iter()
            .any(|r| r.id == "stub"));
        assert!(!policy_disk_stale(&pool, root).await.unwrap());

        write_file(
            &root.join("shared/rules/body.mdc"),
            &rule_text("body", "external body, edited"),
        );
        assert!(policy_disk_stale(&pool, root).await.unwrap());
    }

    #[tokio::test]
    async fn disk_stale_sees_a_deleted_rule_and_skill() {
        let (dir, pool) = test_pool().await;
        let root = dir.path();
        write_file(
            &root.join(".agents/rules/keep.mdc"),
            &rule_text("keep", "k"),
        );
        write_file(
            &root.join(".agents/rules/gone.mdc"),
            &rule_text("gone", "g"),
        );
        write_file(
            &root.join(".agents/skills/gone-skill/SKILL.md"),
            "---\nname: gone-skill\ndescription: d\n---\n\nSteps.\n",
        );
        import_policy_from_files(&pool, root, ImportMode::Replace)
            .await
            .unwrap();
        assert!(!policy_disk_stale(&pool, root).await.unwrap());

        std::fs::remove_file(root.join(".agents/rules/gone.mdc")).unwrap();
        assert!(policy_disk_stale(&pool, root).await.unwrap());
        import_policy_from_files(&pool, root, ImportMode::Replace)
            .await
            .unwrap();
        assert!(!policy_disk_stale(&pool, root).await.unwrap());

        std::fs::remove_dir_all(root.join(".agents/skills/gone-skill")).unwrap();
        assert!(policy_disk_stale(&pool, root).await.unwrap());
    }

    #[tokio::test]
    async fn disk_stale_ignores_database_owned_rows() {
        let (dir, pool) = test_pool().await;
        let root = dir.path();
        write_file(
            &root.join(".agents/rules/keep.mdc"),
            &rule_text("keep", "k"),
        );
        import_policy_from_files(&pool, root, ImportMode::Replace)
            .await
            .unwrap();
        let raw =
            "---\nid: db-only\nlevel: INFO\nalwaysApply: true\nstorage: database\n---\n\nBody.\n";
        let doc = parse_rule_file(Path::new("db-only.mdc"), raw).unwrap();
        upsert_rule_doc(&pool, &doc).await.unwrap();
        assert!(!policy_disk_stale(&pool, root).await.unwrap());
    }
}
