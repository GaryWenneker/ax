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

fn read_config_root(path: &Path) -> serde_json::Value {
    if !path.exists() {
        return serde_json::json!({});
    }
    let text = fs::read_to_string(path).unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    if value.is_object() {
        value
    } else {
        serde_json::json!({})
    }
}

pub fn load_global_config() -> GlobalAxConfig {
    let Some(path) = config_path() else {
        return GlobalAxConfig::default();
    };
    if !path.exists() {
        return GlobalAxConfig::default();
    }
    let value = read_config_root(&path);
    serde_json::from_value(value).unwrap_or_default()
}

pub fn save_global_config(cfg: &GlobalAxConfig) -> Result<(), String> {
    let path = config_path().ok_or("no home dir")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Merge with existing to preserve index/policy keys
    let mut merged = read_config_root(&path);
    let agents = serde_json::to_value(&cfg.agents).map_err(|e| e.to_string())?;
    let workspace = serde_json::to_value(&cfg.workspace).map_err(|e| e.to_string())?;
    let obj = merged
        .as_object_mut()
        .expect("read_config_root always returns an object");
    obj.insert("agents".into(), agents);
    obj.insert("workspace".into(), workspace);
    fs::write(
        &path,
        serde_json::to_string_pretty(&merged).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| e.to_string())
}

pub fn load_agents_config() -> AgentsConfig {
    load_global_config().agents
}

pub fn save_agents_config(agents: &AgentsConfig) -> Result<(), String> {
    let mut cfg = load_global_config();
    cfg.agents = agents.clone();
    save_global_config(&cfg)
}

pub fn load_workspace_config() -> WorkspaceConfig {
    load_global_config().workspace
}

pub fn save_workspace_config(workspace: &WorkspaceConfig) -> Result<(), String> {
    let mut cfg = load_global_config();
    cfg.workspace = workspace.clone();
    save_global_config(&cfg)
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

    let mut cfg = load_global_config();
    cfg.workspace.recent.retain(|p| p.path != path_str);
    cfg.workspace.recent.insert(
        0,
        RecentProject {
            path: path_str,
            label,
            last_opened: now,
            initialized,
        },
    );
    cfg.workspace.recent.truncate(20);
    save_global_config(&cfg)
}

/// Drop one project from the global recent list. Returns whether a row was removed.
pub fn forget_recent_project(path: &Path) -> Result<bool, String> {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let path_str = abs.to_string_lossy().into_owned();
    let mut cfg = load_global_config();
    let before = cfg.workspace.recent.len();
    cfg.workspace.recent.retain(|p| {
        let other = PathBuf::from(&p.path);
        let other_abs = other.canonicalize().unwrap_or(other);
        other_abs != abs && p.path != path_str
    });
    let removed = cfg.workspace.recent.len() != before;
    if removed {
        save_global_config(&cfg)?;
    }
    Ok(removed)
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
    fn read_config_root_treats_null_as_empty_object() {
        let path = std::env::temp_dir().join(format!("ax-config-test-{}.json", std::process::id()));
        std::fs::write(&path, "null").unwrap();
        let root = read_config_root(&path);
        assert!(root.is_object());
        assert!(root.as_object().unwrap().is_empty());
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
