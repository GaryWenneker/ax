//! Agent + workspace preferences in `~/.ax/config.json`.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentsConfig {
    #[serde(default)]
    pub preferred_external: Option<String>,
    /// Last agent picked in the Agent terminal dropdown (persists across reloads).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_terminal_agent: Option<String>,
    #[serde(default)]
    pub enabled_targets: Vec<String>,
    #[serde(default = "default_terminal_mode")]
    pub terminal_mode: String,
    #[serde(default)]
    pub active_profile: HashMap<String, String>,
    #[serde(default)]
    pub profiles: HashMap<String, Vec<super::profiles::ProfileEntry>>,
}

fn default_terminal_mode() -> String {
    "auto".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceConfig {
    #[serde(default)]
    pub recent: Vec<RecentProject>,
    #[serde(default)]
    pub browse_roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentProject {
    pub path: String,
    pub label: String,
    #[serde(default)]
    pub last_opened: u64,
    #[serde(default)]
    pub initialized: bool,
}

pub fn config_path() -> Option<PathBuf> {
    ax_utils::paths::home_dir().map(|h| h.join(".ax").join("config.json"))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalAxConfig {
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub workspace: WorkspaceConfig,
}

type ConfigRoot = serde_json::Map<String, serde_json::Value>;

/// Parses the bytes of `~/.ax/config.json`. A missing, empty or `null` file
/// is an empty object; anything else that is not a JSON object is an error,
/// so a hand-edited file with a typo is never replaced by a save.
fn parse_config_root(path: &Path, bytes: Option<&[u8]>) -> Result<ConfigRoot, String> {
    let Some(bytes) = bytes else {
        return Ok(ConfigRoot::new());
    };
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(ConfigRoot::new());
    }
    match serde_json::from_slice::<serde_json::Value>(bytes) {
        Ok(serde_json::Value::Object(map)) => Ok(map),
        Ok(serde_json::Value::Null) => Ok(ConfigRoot::new()),
        Ok(_) => Err(format!(
            "{} does not hold a JSON object; fix it before ax can save settings",
            path.display()
        )),
        Err(e) => Err(format!(
            "{} is not valid JSON ({e}); fix it before ax can save settings",
            path.display()
        )),
    }
}

fn read_config_root(path: &Path) -> Result<ConfigRoot, String> {
    match fs::read(path) {
        Ok(b) => parse_config_root(path, Some(&b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ConfigRoot::new()),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

/// Read-modify-write of `~/.ax/config.json` under a file lock with an atomic
/// replace, so parallel writers (CLI, Command Center) do not lose each
/// other's changes and a crash never leaves a half-written file. Every key
/// `edit` does not touch is kept. A file that does not parse is left alone
/// and reported as an error.
fn update_config_root(
    edit: impl FnOnce(&mut ConfigRoot) -> Result<(), String>,
) -> Result<(), String> {
    let path = config_path().ok_or("no home dir")?;
    let mut failure: Option<String> = None;
    let result = ax_utils::update_locked(&path, |current| {
        let mut root = match parse_config_root(&path, current.as_deref()) {
            Ok(r) => r,
            Err(e) => {
                failure = Some(e);
                return Ok(None);
            }
        };
        if let Err(e) = edit(&mut root) {
            failure = Some(e);
            return Ok(None);
        }
        let body = serde_json::to_string_pretty(&root)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Some((body + "\n").into_bytes()))
    });
    if let Some(e) = failure {
        return Err(e);
    }
    result.map(|_| ()).map_err(|e| e.to_string())
}

fn section<T: serde::de::DeserializeOwned + Default>(root: &ConfigRoot, key: &str) -> T {
    root.get(key)
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

fn set_section<T: Serialize>(root: &mut ConfigRoot, key: &str, value: &T) -> Result<(), String> {
    root.insert(
        key.into(),
        serde_json::to_value(value).map_err(|e| e.to_string())?,
    );
    Ok(())
}

pub fn load_global_config() -> GlobalAxConfig {
    let Some(path) = config_path() else {
        return GlobalAxConfig::default();
    };
    match read_config_root(&path) {
        Ok(root) => GlobalAxConfig {
            agents: section(&root, "agents"),
            workspace: section(&root, "workspace"),
        },
        Err(e) => {
            tracing::warn!("{e}; using default agent and workspace settings");
            GlobalAxConfig::default()
        }
    }
}

pub fn save_global_config(cfg: &GlobalAxConfig) -> Result<(), String> {
    update_config_root(|root| {
        set_section(root, "agents", &cfg.agents)?;
        set_section(root, "workspace", &cfg.workspace)
    })
}

pub fn load_agents_config() -> AgentsConfig {
    load_global_config().agents
}

/// Replaces only the `agents` section; other sections stay as they are on disk.
pub fn save_agents_config(agents: &AgentsConfig) -> Result<(), String> {
    update_config_root(|root| set_section(root, "agents", agents))
}

pub fn load_workspace_config() -> WorkspaceConfig {
    load_global_config().workspace
}

/// Replaces only the `workspace` section; other sections stay as they are on disk.
pub fn save_workspace_config(workspace: &WorkspaceConfig) -> Result<(), String> {
    update_config_root(|root| set_section(root, "workspace", workspace))
}

/// Changes the `workspace` section in one locked read-modify-write.
fn update_workspace(edit: impl FnOnce(&mut WorkspaceConfig)) -> Result<(), String> {
    update_config_root(|root| {
        let mut ws: WorkspaceConfig = section(root, "workspace");
        edit(&mut ws);
        set_section(root, "workspace", &ws)
    })
}

pub fn touch_recent_project(path: &Path, initialized: bool) -> Result<(), String> {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let path_str = abs.to_string_lossy().into_owned();
    let label = abs
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    update_workspace(|ws| {
        ws.recent.retain(|p| p.path != path_str);
        ws.recent.insert(
            0,
            RecentProject {
                path: path_str,
                label,
                last_opened: now,
                initialized,
            },
        );
        ws.recent.truncate(20);
    })
}

/// Drop one project from the global recent list. Returns whether a row was removed.
pub fn forget_recent_project(path: &Path) -> Result<bool, String> {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let path_str = abs.to_string_lossy().into_owned();
    if !load_workspace_config()
        .recent
        .iter()
        .any(|p| recent_matches(p, &abs, &path_str))
    {
        return Ok(false);
    }
    let mut removed = false;
    update_workspace(|ws| {
        let before = ws.recent.len();
        ws.recent.retain(|p| !recent_matches(p, &abs, &path_str));
        removed = ws.recent.len() != before;
    })?;
    Ok(removed)
}

fn recent_matches(p: &RecentProject, abs: &Path, path_str: &str) -> bool {
    let other = PathBuf::from(&p.path);
    let other_abs = other.canonicalize().unwrap_or(other);
    other_abs == abs || p.path == path_str
}

pub fn default_browse_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = ax_utils::paths::home_dir() {
        roots.push(home.clone());
        for sub in ["projects", "dev", "code", "src", "gary"] {
            let p = home.join(sub);
            if p.is_dir() {
                roots.push(p);
            }
        }
    }
    // Drive / filesystem roots so the project picker can walk to any folder
    // (including empty ones) — e.g. C:\gary when the home is C:\Users\….
    #[cfg(windows)]
    {
        for letter in b'A'..=b'Z' {
            let root = PathBuf::from(format!("{}:\\", letter as char));
            if root.is_dir() {
                roots.push(root);
            }
        }
    }
    #[cfg(not(windows))]
    {
        let root = PathBuf::from("/");
        if root.is_dir() {
            roots.push(root);
        }
    }
    roots
}

/// How many directory levels below the home directory the project scan still enters.
const PROJECT_SCAN_DEPTH: usize = 4;

fn skip_scan_dir(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "node_modules"
                | "target"
                | "target-dev"
                | "dist"
                | "build"
                | "vendor"
                | "Library"
                | "Applications"
                | "Movies"
                | "Music"
                | "Pictures"
                | "Downloads"
                | "AppData"
                | "Caches"
                | "Pods"
                | "coverage"
        )
}

fn canonical_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn visit_ax_projects(dir: &Path, depth: usize, max_depth: usize, out: &mut Vec<PathBuf>) {
    if dir.join(".ax").join("ax.db").is_file() {
        out.push(canonical_path(dir));
    }
    if depth >= max_depth {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if skip_scan_dir(&name) {
            continue;
        }
        visit_ax_projects(&entry.path(), depth + 1, max_depth, out);
    }
}

/// Directories that contain `.ax/ax.db`, found by walking `root` up to `max_depth`.
///
/// Hidden directories and dependency or build folders are not entered. Symlinks are not followed.
fn scan_initialized_projects(root: &Path, max_depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if root.is_dir() {
        visit_ax_projects(root, 0, max_depth, &mut out);
    }
    out.sort();
    out.dedup();
    out
}

fn path_key(path: &Path) -> String {
    canonical_path(path).to_string_lossy().into_owned()
}

/// Recent projects first. Scanned projects that are not already listed are appended, sorted by label.
fn merge_scanned_projects(recent: Vec<RecentProject>, scanned: &[PathBuf]) -> Vec<RecentProject> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for project in recent {
        if seen.insert(path_key(Path::new(&project.path))) {
            out.push(project);
        }
    }
    let mut extra = Vec::new();
    for path in scanned {
        let canon = canonical_path(path);
        if !seen.insert(canon.to_string_lossy().into_owned()) {
            continue;
        }
        let label = canon
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();
        extra.push(RecentProject {
            path: canon.to_string_lossy().into_owned(),
            label,
            last_opened: 0,
            initialized: true,
        });
    }
    extra.sort_by_key(|a| a.label.to_lowercase());
    out.extend(extra);
    out
}

/// Recent projects, plus every initialized ax project the home scan finds.
pub fn projects_for_switcher() -> Vec<RecentProject> {
    let recent = load_workspace_config().recent;
    let Some(home) = ax_utils::paths::home_dir() else {
        return recent;
    };
    let scanned = scan_initialized_projects(&home, PROJECT_SCAN_DEPTH);
    merge_scanned_projects(recent, &scanned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ax-scan-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ax_project(dir: &Path) {
        let ax = dir.join(".ax");
        fs::create_dir_all(&ax).unwrap();
        fs::write(ax.join("ax.db"), b"x").unwrap();
    }

    fn labels(paths: &[PathBuf]) -> Vec<String> {
        let mut names: Vec<String> = paths
            .iter()
            .map(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn malformed_config_is_never_overwritten() {
        let home = crate::test_home::fresh("malformed-config");
        let path = home.dir.join(".ax").join("config.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let original = "{\n  \"offload\": { \"endpoint\": \"https://x\" },\n  \"index\": {},\n}\n";
        fs::write(&path, original).unwrap();
        let _ = save_agents_config(&AgentsConfig::default());
        let _ = touch_recent_project(&home.dir, true);
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert!(save_agents_config(&AgentsConfig::default()).is_err());
    }

    #[test]
    fn saving_keeps_every_other_top_level_key() {
        let home = crate::test_home::fresh("keep-keys");
        let path = home.dir.join(".ax").join("config.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{"offload":{"endpoint":"https://x"},"index":{"a":1}}"#,
        )
        .unwrap();
        save_agents_config(&AgentsConfig::default()).unwrap();
        touch_recent_project(&home.dir, true).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["offload"]["endpoint"], "https://x");
        assert_eq!(v["index"]["a"], 1);
        assert_eq!(v["workspace"]["recent"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn concurrent_recent_project_updates_are_not_lost() {
        let home = crate::test_home::fresh("concurrent-recent");
        let dirs: Vec<PathBuf> = (0..16)
            .map(|i| {
                let d = home.dir.join(format!("p{i}"));
                fs::create_dir_all(&d).unwrap();
                d
            })
            .collect();
        let threads: Vec<_> = dirs
            .iter()
            .cloned()
            .map(|d| std::thread::spawn(move || touch_recent_project(&d, true).unwrap()))
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert_eq!(load_workspace_config().recent.len(), 16);
    }

    #[test]
    fn read_config_root_treats_null_as_empty_object() {
        let path = std::env::temp_dir().join(format!("ax-config-test-{}.json", std::process::id()));
        std::fs::write(&path, "null").unwrap();
        let root = read_config_root(&path).unwrap();
        assert!(root.is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn scan_finds_initialized_project_and_skips_plain_folder() {
        let root = scratch("plain");
        ax_project(&root.join("alpha"));
        fs::create_dir_all(root.join("plain")).unwrap();
        let found = scan_initialized_projects(&root, 2);
        assert_eq!(labels(&found), vec!["alpha".to_string()]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scan_includes_the_root_when_it_is_a_project() {
        let root = scratch("rootproj");
        ax_project(&root);
        let found = scan_initialized_projects(&root, 1);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], root.canonicalize().unwrap());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scan_skips_projects_inside_node_modules() {
        let root = scratch("nm");
        ax_project(&root.join("node_modules").join("hidden-dep"));
        ax_project(&root.join("visible"));
        let found = scan_initialized_projects(&root, 3);
        assert_eq!(labels(&found), vec!["visible".to_string()]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scan_finds_parent_and_nested_child() {
        let root = scratch("nest");
        let parent = root.join("parent");
        ax_project(&parent);
        ax_project(&parent.join("child"));
        let found = scan_initialized_projects(&root, 3);
        assert_eq!(
            labels(&found),
            vec!["child".to_string(), "parent".to_string()]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scan_skips_hidden_directories() {
        let root = scratch("hidden");
        ax_project(&root.join(".secret").join("proj"));
        let found = scan_initialized_projects(&root, 3);
        assert!(found.is_empty(), "{found:?}");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scan_respects_max_depth() {
        let root = scratch("depth");
        ax_project(&root.join("a").join("b").join("deep"));
        let shallow = scan_initialized_projects(&root, 2);
        assert!(shallow.is_empty(), "{shallow:?}");
        let deep = scan_initialized_projects(&root, 3);
        assert_eq!(labels(&deep), vec!["deep".to_string()]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn merge_keeps_recent_order_and_appends_missing_projects() {
        let root = scratch("merge");
        let known = root.join("known");
        let bravo = root.join("bravo");
        let alpha = root.join("alpha");
        ax_project(&known);
        ax_project(&bravo);
        ax_project(&alpha);
        let known = known.canonicalize().unwrap();
        let bravo = bravo.canonicalize().unwrap();
        let alpha = alpha.canonicalize().unwrap();
        let recent = vec![RecentProject {
            path: known.display().to_string(),
            label: "known".into(),
            last_opened: 9,
            initialized: true,
        }];
        let merged = merge_scanned_projects(recent, &[bravo.clone(), known.clone(), alpha.clone()]);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].label, "known");
        assert_eq!(merged[0].last_opened, 9);
        assert_eq!(merged[1].label, "alpha");
        assert_eq!(merged[1].path, alpha.display().to_string());
        assert!(merged[1].initialized);
        assert_eq!(merged[1].last_opened, 0);
        assert_eq!(merged[2].label, "bravo");
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn scan_does_not_follow_symlinked_directories() {
        let root = scratch("link");
        let outside = scratch("outside");
        let linked = outside.join("linked-proj");
        ax_project(&linked);
        std::os::unix::fs::symlink(&linked, root.join("alias")).unwrap();
        let found = scan_initialized_projects(&root, 2);
        assert!(
            found
                .iter()
                .all(|p| p.file_name().and_then(|n| n.to_str()) != Some("linked-proj")),
            "{found:?}"
        );
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }
}
