//! Policy storage mode — filesystem (default) or database-first.
//! Also reads `requireReview`, `policySync`, and external `policy.roots`.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::types::PolicyScope;

const CONFIG_FILENAME: &str = "ax.json";
const CONFIG_ALIAS: &str = ".ax.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyStorage {
    #[default]
    Files,
    Database,
}

impl PolicyStorage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Database => "database",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "files" | "file" => Some(Self::Files),
            "database" | "db" => Some(Self::Database),
            _ => None,
        }
    }
}

/// Resolve per-item storage override against the project default.
pub fn effective_storage(
    project_default: PolicyStorage,
    item_storage: Option<&str>,
) -> PolicyStorage {
    item_storage
        .and_then(PolicyStorage::parse)
        .unwrap_or(project_default)
}

/// One configured external (or sibling-repo) policy mount.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRoot {
    pub id: String,
    /// Absolute resolved path.
    pub path: PathBuf,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<String>,
    /// Whether the directory currently exists on disk.
    pub exists: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolicyRootFile {
    id: String,
    path: String,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    member: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolicyConfigFile {
    #[serde(default)]
    storage: Option<PolicyStorage>,
    #[serde(default)]
    require_review: Option<bool>,
    #[serde(default)]
    roots: Vec<PolicyRootFile>,
    #[serde(default)]
    stacks: Vec<String>,
    #[serde(default)]
    stack_detect: Option<String>,
    #[serde(default)]
    agents_dir: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PolicyConfig {
    pub storage: PolicyStorage,
    /// When true, pack import (and capture when configured) lands as pending.
    pub require_review: bool,
    pub roots: Vec<PolicyRoot>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            storage: PolicyStorage::Files,
            require_review: false,
            roots: Vec::new(),
        }
    }
}

/// The config file of a project or workspace folder: `ax.json`, or
/// `.ax.json` when only that one exists. Every read and write of the folder's
/// settings goes through this path, so the two names never split a setting.
pub fn config_path(dir: &Path) -> PathBuf {
    let primary = dir.join(CONFIG_FILENAME);
    if !primary.exists() {
        let alias = dir.join(CONFIG_ALIAS);
        if alias.exists() {
            return alias;
        }
    }
    primary
}

/// Merge global `~/.ax/config.json` `"policy"` section with per-project `ax.json`
/// (and workspace root `ax.json` roots when present).
pub fn load_policy_config(project_root: &Path) -> PolicyConfig {
    #[cfg(test)]
    test_counters::CONFIG_LOADS.with(|c| c.set(c.get() + 1));
    let global_path = global_config_path();
    let global = read_policy_section(&global_path);
    let local = read_policy_section(&config_path(project_root));
    let workspace = crate::hierarchy::find_workspace_root(project_root);
    let member_name = workspace
        .as_deref()
        .and_then(|ws| current_member_name(ws, project_root));
    let member = (project_root, member_name.as_deref());
    let mut roots = Vec::new();
    append_roots(
        &mut roots,
        &global.roots,
        global_path.parent().unwrap_or_else(|| Path::new(".")),
        member,
    );
    if let Some(ws) = &workspace {
        if ws != project_root {
            let ws_section = read_policy_section(&config_path(ws));
            append_roots(&mut roots, &ws_section.roots, ws, member);
        }
    }
    append_roots(&mut roots, &local.roots, project_root, member);

    PolicyConfig {
        storage: local
            .storage
            .or(global.storage)
            .unwrap_or(PolicyStorage::Files),
        require_review: local
            .require_review
            .or(global.require_review)
            .unwrap_or(false),
        roots,
    }
}

/// Configured roots applicable to this project (member filter applied).
pub fn load_policy_roots(project_root: &Path) -> Vec<PolicyRoot> {
    load_policy_config(project_root).roots
}

/// Look up a root by id for the project.
pub fn find_policy_root(project_root: &Path, root_id: &str) -> Option<PolicyRoot> {
    load_policy_roots(project_root)
        .into_iter()
        .find(|r| r.id == root_id)
}

/// `member` is the project root and its workspace member name, if any.
fn append_roots(
    out: &mut Vec<PolicyRoot>,
    entries: &[PolicyRootFile],
    config_dir: &Path,
    (project_root, member_name): (&Path, Option<&str>),
) {
    for entry in entries {
        if entry.id.trim().is_empty() || entry.path.trim().is_empty() {
            continue;
        }
        if let Some(ref m) = entry.member {
            match member_name {
                Some(name) if name == m || member_path_matches(project_root, m) => {}
                _ => continue,
            }
        }
        let abs = resolve_config_path(config_dir, &entry.path);
        let scope = PolicyScope::parse(entry.scope.as_deref().unwrap_or("project"))
            .unwrap_or(PolicyScope::Project)
            .as_str()
            .to_string();
        // Later configs (project) override earlier ones with the same id.
        out.retain(|r| r.id != entry.id);
        let exists = abs.is_dir();
        out.push(PolicyRoot {
            id: entry.id.clone(),
            path: abs,
            scope,
            member: entry.member.clone(),
            exists,
        });
    }
}

fn resolve_config_path(config_dir: &Path, raw: &str) -> PathBuf {
    let p = PathBuf::from(raw.trim());
    let joined = if p.is_absolute() {
        p
    } else {
        config_dir.join(p)
    };
    joined.canonicalize().unwrap_or(joined)
}

fn current_member_name(ws: &Path, project_root: &Path) -> Option<String> {
    let content = std::fs::read_to_string(config_path(ws)).ok()?;
    let v = serde_json::from_str::<serde_json::Value>(&content).ok()?;
    let members = v.get("members").and_then(|m| m.as_array())?;
    let project_canon = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    for m in members {
        let rel = m.get("path").and_then(|p| p.as_str()).unwrap_or("");
        if rel.is_empty() {
            continue;
        }
        let abs = ws.join(rel);
        let abs_canon = abs.canonicalize().unwrap_or(abs);
        if abs_canon == project_canon {
            if let Some(n) = m.get("name").and_then(|n| n.as_str()) {
                return Some(n.to_string());
            }
            return Some(
                Path::new(rel)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| rel.to_string()),
            );
        }
    }
    None
}

fn member_path_matches(project_root: &Path, member_key: &str) -> bool {
    project_root
        .file_name()
        .map(|n| n.to_string_lossy() == member_key)
        .unwrap_or(false)
        || project_root
            .to_string_lossy()
            .replace('\\', "/")
            .ends_with(member_key)
}

/// True when project `ax.json` has `"policySync": true`.
pub fn policy_sync_enabled(project_root: &Path) -> bool {
    read_root_bool(project_root, "policySync")
}

/// Read / write top-level `policySync` in project ax.json.
pub fn write_project_policy_sync(project_root: &Path, enabled: bool) -> Result<PathBuf, String> {
    let path = config_path(project_root);
    write_root_bool_at(&path, "policySync", enabled)?;
    Ok(path)
}

pub fn write_project_require_review(project_root: &Path, enabled: bool) -> Result<PathBuf, String> {
    let path = config_path(project_root);
    write_policy_bool_at(&path, "requireReview", enabled)?;
    Ok(path)
}

/// Where the advisory lock for writing `path` lives: `.ax/<name>.lock` next
/// to it, or `<name>.lock` when `path` is already inside an `.ax` folder (the
/// global `~/.ax/config.json`). The lock stays out of the project root.
fn config_lock_path(path: &Path) -> PathBuf {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| CONFIG_FILENAME.into());
    let lock_dir = if dir.file_name().is_some_and(|n| n == ".ax") {
        dir.to_path_buf()
    } else {
        dir.join(".ax")
    };
    lock_dir.join(format!("{name}.lock"))
}

/// Changes a config file under an advisory lock: reads it, lets `change`
/// edit the root object, and replaces the file atomically. A missing or empty
/// file is an empty object. A file that exists but is not valid JSON (or whose
/// root is not an object) is an error and is left untouched, so a writer never
/// replaces a hand-edited file and loses its other keys. Parallel writers of
/// different keys keep each other's changes.
fn update_config<T>(
    path: &Path,
    change: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> T,
) -> Result<T, String> {
    let mut refusal: Option<String> = None;
    let mut out: Option<T> = None;
    let result = ax_utils::update_locked_at(path, &config_lock_path(path), |current| {
        let text = current
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default();
        let mut root = if text.trim().is_empty() {
            serde_json::json!({})
        } else {
            match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(root) => root,
                Err(e) => {
                    let msg = format!(
                        "{} is not valid JSON ({e}); fix the file and try again, nothing was changed",
                        path.display()
                    );
                    refusal = Some(msg.clone());
                    return Err(std::io::Error::other(msg));
                }
            }
        };
        let Some(obj) = root.as_object_mut() else {
            let msg = format!(
                "{}: config root must be a JSON object; nothing was changed",
                path.display()
            );
            refusal = Some(msg.clone());
            return Err(std::io::Error::other(msg));
        };
        out = Some(change(obj));
        let text = serde_json::to_string_pretty(&root).map_err(std::io::Error::other)? + "\n";
        Ok(Some(text.into_bytes()))
    });
    match result {
        Ok(_) => out.ok_or_else(|| format!("cannot write {}", path.display())),
        Err(_) if refusal.is_some() => Err(refusal.unwrap_or_default()),
        Err(e) => Err(format!("cannot write {}: {e}", path.display())),
    }
}

/// Takes the `policy` object out of `root` (an empty one when it is missing
/// or not an object), so the caller can edit it and put it back.
fn policy_section_mut(
    root: &mut serde_json::Map<String, serde_json::Value>,
) -> &mut serde_json::Map<String, serde_json::Value> {
    let policy = root
        .entry("policy")
        .or_insert_with(|| serde_json::json!({}));
    if !policy.is_object() {
        *policy = serde_json::json!({});
    }
    policy.as_object_mut().expect("policy is an object")
}

fn read_root_bool(project_root: &Path, key: &str) -> bool {
    std::fs::read_to_string(config_path(project_root))
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .and_then(|v| v.get(key).and_then(|x| x.as_bool()))
        .unwrap_or(false)
}

fn write_root_bool_at(path: &Path, key: &str, value: bool) -> Result<(), String> {
    update_config(path, |root| {
        root.insert(key.into(), serde_json::Value::Bool(value));
    })
}

fn write_policy_bool_at(path: &Path, key: &str, value: bool) -> Result<(), String> {
    update_config(path, |root| {
        policy_section_mut(root).insert(key.into(), serde_json::Value::Bool(value));
    })
}

fn read_policy_section(path: &Path) -> PolicyConfigFile {
    let Ok(content) = std::fs::read_to_string(path) else {
        return PolicyConfigFile::default();
    };
    let Ok(root) = serde_json::from_str::<serde_json::Value>(&content) else {
        return PolicyConfigFile::default();
    };
    root.get("policy")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn global_config_path() -> PathBuf {
    crate::paths::home_dir()
        .map(|h| h.join(".ax").join("config.json"))
        .unwrap_or_else(|| PathBuf::from(".ax/config.json"))
}

/// Where the effective storage mode comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyStorageSource {
    Project,
    Global,
    Default,
}

impl PolicyStorageSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Global => "global",
            Self::Default => "default",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyStorageStatus {
    pub effective: String,
    pub source: String,
    pub config_path: String,
    pub global_config_path: String,
    pub project_value: Option<String>,
    pub global_value: Option<String>,
    pub require_review: bool,
    pub policy_sync: bool,
    pub roots: Vec<PolicyRoot>,
}

/// Resolved storage mode and which config layer set it.
pub fn policy_storage_status(project_root: &Path) -> PolicyStorageStatus {
    let global = read_policy_section(&global_config_path());
    let project_path = config_path(project_root);
    let local = read_policy_section(&project_path);
    let cfg = load_policy_config(project_root);

    let (effective, source) = match local.storage {
        Some(s) => (s, PolicyStorageSource::Project),
        None => match global.storage {
            Some(s) => (s, PolicyStorageSource::Global),
            None => (PolicyStorage::Files, PolicyStorageSource::Default),
        },
    };

    PolicyStorageStatus {
        effective: effective.as_str().into(),
        source: source.label().into(),
        config_path: project_path.display().to_string(),
        global_config_path: global_config_path().display().to_string(),
        project_value: local.storage.map(|s| s.as_str().into()),
        global_value: global.storage.map(|s| s.as_str().into()),
        require_review: cfg.require_review,
        policy_sync: policy_sync_enabled(project_root),
        roots: cfg.roots,
    }
}

/// Set `policy.storage` in per-project `ax.json` (merges other keys).
pub fn write_project_policy_storage(
    project_root: &Path,
    storage: PolicyStorage,
) -> Result<PathBuf, String> {
    let path = config_path(project_root);
    write_policy_storage_at(&path, storage)?;
    Ok(path)
}

/// Set `policy.storage` in global `~/.ax/config.json` (merges other keys).
pub fn write_global_policy_storage(storage: PolicyStorage) -> Result<PathBuf, String> {
    let path = global_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    write_policy_storage_at(&path, storage)?;
    Ok(path)
}

fn write_policy_storage_at(path: &Path, storage: PolicyStorage) -> Result<(), String> {
    update_config(path, |root| {
        policy_section_mut(root).insert(
            "storage".into(),
            serde_json::Value::String(storage.as_str().into()),
        );
    })
}

/// Stack ids saved in the project `ax.json` only (not the global config).
pub fn read_project_stack_ids(project_root: &Path) -> Vec<String> {
    read_policy_section(&config_path(project_root)).stacks
}

/// `policy.stackDetect` from the project `ax.json` (`off` or `suggest`).
pub fn read_project_stack_detect(project_root: &Path) -> String {
    read_policy_section(&config_path(project_root))
        .stack_detect
        .unwrap_or_else(|| "off".into())
}

/// Merge `policy.stacks` and `policy.stackDetect` into project `ax.json`.
pub fn write_project_stacks(
    project_root: &Path,
    stacks: &[String],
    stack_detect: &str,
) -> Result<(), String> {
    let path = config_path(project_root);
    update_config(&path, |root| {
        let policy_obj = policy_section_mut(root);
        policy_obj.insert(
            "stacks".into(),
            serde_json::Value::Array(
                stacks
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
        policy_obj.insert(
            "stackDetect".into(),
            serde_json::Value::String(stack_detect.into()),
        );
    })
}

/// `agents.ides` from the project `ax.json`; `None` when it was never saved.
pub fn read_project_ides(project_root: &Path) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(config_path(project_root)).ok()?;
    let root: serde_json::Value = serde_json::from_str(&text).ok()?;
    let list = root.get("agents")?.get("ides")?.as_array()?;
    Some(
        list.iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
    )
}

/// Save `agents.ides` in the project `ax.json`, keeping every other key.
pub fn write_project_ides(project_root: &Path, ides: &[String]) -> Result<(), String> {
    let path = config_path(project_root);
    update_config(&path, |root| {
        let agents = root
            .entry("agents")
            .or_insert_with(|| serde_json::json!({}));
        if !agents.is_object() {
            *agents = serde_json::json!({});
        }
        agents["ides"] = serde_json::json!(ides);
    })
}

pub const DEFAULT_AGENTS_DIR: &str = ".agents";

/// `policy.agentsDir` when the project `ax.json` defines it. A value that
/// fails [`validate_agents_dir_name`] (a path, `..`, or a reserved folder) is
/// ignored with a warning, so rules and skills always stay inside the project.
pub fn configured_agents_dir(project_root: &Path) -> Option<String> {
    let raw = read_policy_section(&config_path(project_root))
        .agents_dir
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())?;
    match validate_agents_dir_name(&raw) {
        Ok(name) => Some(name),
        Err(e) => {
            eprintln!(
                "[ax policy] ignoring policy.agentsDir {raw:?} in {}: {e}; using {DEFAULT_AGENTS_DIR}",
                config_path(project_root).display()
            );
            None
        }
    }
}

/// Folder name for on-disk rules and skills. `.agents` when unset.
pub fn agents_dir_name(project_root: &Path) -> String {
    configured_agents_dir(project_root).unwrap_or_else(|| DEFAULT_AGENTS_DIR.to_string())
}

/// A single relative directory name. No absolute paths, slashes, `..`, `.`,
/// drive prefixes, or the folders ax and git own (`.ax`, `.git`).
pub fn validate_agents_dir_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Ok(DEFAULT_AGENTS_DIR.to_string());
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") || name.contains(':') {
        return Err("agents directory must be a single folder name, not a path".into());
    }
    if Path::new(name).is_absolute() {
        return Err("agents directory must be a folder name inside the project".into());
    }
    if name == "." || name.eq_ignore_ascii_case(".ax") || name.eq_ignore_ascii_case(".git") {
        return Err(format!(
            "agents directory cannot be {name:?}; pick a folder of its own such as {DEFAULT_AGENTS_DIR}"
        ));
    }
    Ok(name.to_string())
}

/// Save `policy.agentsDir` in the project `ax.json`.
pub fn write_project_agents_dir(project_root: &Path, raw: &str) -> Result<String, String> {
    let name = validate_agents_dir_name(raw)?;
    let path = config_path(project_root);
    update_config(&path, |root| {
        policy_section_mut(root)
            .insert("agentsDir".into(), serde_json::Value::String(name.clone()));
    })?;
    Ok(name)
}

/// Per-thread counters that let tests check how often config is loaded.
#[cfg(test)]
pub(crate) mod test_counters {
    thread_local! {
        pub(crate) static CONFIG_LOADS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_ides_round_trip_and_keep_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_project_ides(dir.path()), None);
        let path = dir.path().join(CONFIG_FILENAME);
        std::fs::write(
            &path,
            r#"{"policy":{"stacks":["rust"]},"agents":{"other":1}}"#,
        )
        .unwrap();
        assert_eq!(read_project_ides(dir.path()), None);

        write_project_ides(dir.path(), &["cursor".into(), "zed".into()]).unwrap();
        assert_eq!(
            read_project_ides(dir.path()),
            Some(vec!["cursor".to_string(), "zed".to_string()])
        );
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["policy"]["stacks"][0], "rust");
        assert_eq!(saved["agents"]["other"], 1);

        write_project_ides(dir.path(), &[]).unwrap();
        assert_eq!(read_project_ides(dir.path()), Some(Vec::new()));

        std::fs::write(&path, r#"{"agents":{"ides":"cursor"}}"#).unwrap();
        assert_eq!(read_project_ides(dir.path()), None);
    }

    #[test]
    fn dot_ax_json_alias_is_read_and_written_for_every_setting() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let alias = p.join(CONFIG_ALIAS);
        std::fs::write(
            &alias,
            r#"{"policySync":true,"policy":{"agentsDir":"team-policy","storage":"database","stacks":["rust"]}}"#,
        )
        .unwrap();
        assert_eq!(agents_dir_name(p), "team-policy");
        assert_eq!(
            policy_storage_status(p).project_value.as_deref(),
            Some("database")
        );
        assert_eq!(read_project_stack_ids(p), vec!["rust".to_string()]);
        assert!(policy_sync_enabled(p));

        let written = write_project_policy_sync(p, false).unwrap();
        assert_eq!(written, alias);
        assert!(
            !policy_sync_enabled(p),
            "the alias file holds the new value"
        );
        assert!(!p.join(CONFIG_FILENAME).exists(), "no second config file");
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&alias).unwrap()).unwrap();
        assert_eq!(saved["policy"]["agentsDir"], "team-policy");

        // ax.json wins when both exist.
        std::fs::write(p.join(CONFIG_FILENAME), r#"{"policySync":false}"#).unwrap();
        std::fs::write(&alias, r#"{"policySync":true}"#).unwrap();
        assert!(!policy_sync_enabled(p));
    }

    #[test]
    fn workspace_roots_with_a_member_filter_apply_to_that_member_only() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        for m in ["svc", "web"] {
            std::fs::create_dir_all(ws.join(m)).unwrap();
        }
        std::fs::create_dir_all(ws.join("team")).unwrap();
        std::fs::write(
            ws.join("ax.json"),
            r#"{"members":[{"path":"svc","name":"api"},{"path":"web"}],
                "policy":{"roots":[{"id":"api-only","path":"team","member":"api"},
                                   {"id":"all","path":"team"}]}}"#,
        )
        .unwrap();
        let ids =
            |p: &Path| -> Vec<String> { load_policy_roots(p).into_iter().map(|r| r.id).collect() };
        assert_eq!(ids(&ws.join("svc")), vec!["api-only", "all"]);
        assert_eq!(ids(&ws.join("web")), vec!["all"]);
    }

    #[test]
    fn default_storage_is_files() {
        let cfg = load_policy_config(Path::new("/nonexistent"));
        assert_eq!(cfg.storage, PolicyStorage::Files);
        assert!(!cfg.require_review);
        assert!(cfg.roots.is_empty());
    }

    #[test]
    fn write_project_storage_preserves_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILENAME);
        std::fs::write(
            &path,
            r#"{"index":{"exclude":["**/tmp/**"]},"policy":{"storage":"files"}}"#,
        )
        .unwrap();

        write_project_policy_storage(dir.path(), PolicyStorage::Database).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let root: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(root["policy"]["storage"], "database");
        assert_eq!(root["index"]["exclude"][0], "**/tmp/**");
    }

    #[test]
    fn policy_storage_parse_aliases() {
        assert_eq!(PolicyStorage::parse("db"), Some(PolicyStorage::Database));
        assert_eq!(PolicyStorage::parse("file"), Some(PolicyStorage::Files));
    }

    #[test]
    fn policy_sync_flag() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(CONFIG_FILENAME), r#"{"policySync":true}"#).unwrap();
        assert!(policy_sync_enabled(dir.path()));
    }

    #[test]
    fn effective_storage_respects_override() {
        assert_eq!(
            effective_storage(PolicyStorage::Files, Some("database")),
            PolicyStorage::Database
        );
        assert_eq!(
            effective_storage(PolicyStorage::Database, None),
            PolicyStorage::Database
        );
        assert_eq!(
            effective_storage(PolicyStorage::Database, Some("files")),
            PolicyStorage::Files
        );
    }

    #[test]
    fn loads_policy_roots_relative_and_absolute() {
        let dir = tempfile::tempdir().unwrap();
        let external = dir.path().join("external-policy");
        std::fs::create_dir_all(external.join("rules")).unwrap();
        let abs = external.display().to_string().replace('\\', "/");
        std::fs::write(
            dir.path().join(CONFIG_FILENAME),
            format!(
                r#"{{"policy":{{"storage":"files","roots":[{{"id":"shared","path":"{abs}","scope":"workspace"}},{{"id":"rel","path":"external-policy","scope":"project"}}]}}}}"#
            ),
        )
        .unwrap();
        let roots = load_policy_roots(dir.path());
        assert_eq!(roots.len(), 2);
        assert!(roots.iter().any(|r| r.id == "shared" && r.exists));
        assert!(roots.iter().any(|r| r.id == "rel" && r.exists));
        let status = policy_storage_status(dir.path());
        assert_eq!(status.roots.len(), 2);
    }

    #[test]
    fn agents_dir_defaults_until_saved() {
        let dir = tempfile::tempdir().unwrap();
        assert!(configured_agents_dir(dir.path()).is_none());
        assert_eq!(agents_dir_name(dir.path()), ".agents");
        assert!(validate_agents_dir_name("../x").is_err());
        assert_eq!(validate_agents_dir_name("").unwrap(), ".agents");
        let saved = write_project_agents_dir(dir.path(), "team-policy").unwrap();
        assert_eq!(saved, "team-policy");
        assert_eq!(
            configured_agents_dir(dir.path()).as_deref(),
            Some("team-policy")
        );
    }

    #[test]
    fn writers_refuse_an_unparseable_config_and_leave_it_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILENAME);
        let broken = "{\"members\":[\"a\"],\"index\":{\"exclude\":[\"x\"]},}\n";
        std::fs::write(&path, broken).unwrap();
        let attempts: Vec<(&str, Result<(), String>)> = vec![
            (
                "policySync",
                write_project_policy_sync(dir.path(), true).map(|_| ()),
            ),
            (
                "requireReview",
                write_project_require_review(dir.path(), true).map(|_| ()),
            ),
            (
                "storage",
                write_project_policy_storage(dir.path(), PolicyStorage::Database).map(|_| ()),
            ),
            (
                "stacks",
                write_project_stacks(dir.path(), &["rust".into()], "off"),
            ),
            ("ides", write_project_ides(dir.path(), &["cursor".into()])),
            (
                "agentsDir",
                write_project_agents_dir(dir.path(), "team").map(|_| ()),
            ),
        ];
        for (what, result) in attempts {
            let err = result.expect_err(what);
            assert!(err.contains(CONFIG_FILENAME), "{what}: {err}");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), broken, "{what}");
        }
    }

    type Job = Box<dyn FnOnce(&Path) + Send>;

    #[test]
    fn parallel_writers_of_different_keys_keep_every_key() {
        for round in 0..40 {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().to_path_buf();
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(6));
            let jobs: Vec<Job> = vec![
                Box::new(|p| {
                    write_project_policy_sync(p, true).unwrap();
                }),
                Box::new(|p| {
                    write_project_require_review(p, true).unwrap();
                }),
                Box::new(|p| {
                    write_project_policy_storage(p, PolicyStorage::Database).unwrap();
                }),
                Box::new(|p| write_project_stacks(p, &["rust".into()], "suggest").unwrap()),
                Box::new(|p| write_project_ides(p, &["cursor".into()]).unwrap()),
                Box::new(|p| {
                    write_project_agents_dir(p, "team").unwrap();
                }),
            ];
            let handles: Vec<_> = jobs
                .into_iter()
                .map(|job| {
                    let root = root.clone();
                    let barrier = barrier.clone();
                    std::thread::spawn(move || {
                        barrier.wait();
                        job(&root);
                    })
                })
                .collect();
            for h in handles {
                h.join().unwrap();
            }
            let text = std::fs::read_to_string(config_path(&root)).unwrap();
            let v: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(v["policySync"], true, "round {round}: {text}");
            assert_eq!(v["policy"]["requireReview"], true, "round {round}: {text}");
            assert_eq!(v["policy"]["storage"], "database", "round {round}: {text}");
            assert_eq!(
                v["policy"]["stackDetect"], "suggest",
                "round {round}: {text}"
            );
            assert_eq!(v["policy"]["agentsDir"], "team", "round {round}: {text}");
            assert_eq!(v["agents"]["ides"][0], "cursor", "round {round}: {text}");
        }
    }

    #[test]
    fn writers_create_a_missing_config() {
        let dir = tempfile::tempdir().unwrap();
        write_project_policy_sync(dir.path(), true).unwrap();
        assert!(policy_sync_enabled(dir.path()));
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }
}
