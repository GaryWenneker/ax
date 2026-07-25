//! Workspace federation — multi-member monorepo discovery and config.
//!
//! Root config lives in `ax.json` (alias: `.ax.json`) with a `members` array:
//!
//! ```json
//! {
//!   "members": [
//!     { "path": "services/api", "name": "api" },
//!     { "path": "services/billing" }
//!   ]
//! }
//! ```

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use ax_context::directory::CONFIG_FILENAME;

const CONFIG_ALIAS: &str = ".ax.json";
const SKIP_DIRS: &[&str] = &[
    "target",
    "target-dev",
    "node_modules",
    ".git",
    "dist",
    "build",
    ".ax",
];
const MAX_DISCOVERY_DEPTH: usize = 4;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMember {
    /// Path relative to the workspace root.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceConfig {
    #[serde(default)]
    pub members: Vec<WorkspaceMember>,
}

/// Load workspace members from `ax.json` or `.ax.json` at `root`.
pub fn load_workspace_config(root: &Path) -> WorkspaceConfig {
    for name in [CONFIG_FILENAME, CONFIG_ALIAS] {
        let path = root.join(name);
        if !path.is_file() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(cfg) = serde_json::from_str::<WorkspaceConfig>(&content) {
            if !cfg.members.is_empty() {
                return cfg;
            }
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(members) = v.get("members") {
                if let Ok(list) = serde_json::from_value::<Vec<WorkspaceMember>>(members.clone()) {
                    if !list.is_empty() {
                        return WorkspaceConfig { members: list };
                    }
                }
            }
        }
    }
    WorkspaceConfig::default()
}

/// Write (or merge) `members` into `ax.json` at `root`.
pub fn write_workspace_config(root: &Path, config: &WorkspaceConfig) -> Result<(), String> {
    let path = root.join(CONFIG_FILENAME);
    let mut root_val = if path.is_file() {
        let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str::<serde_json::Value>(&content).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    let obj = root_val
        .as_object_mut()
        .ok_or_else(|| "ax.json root must be a JSON object".to_string())?;
    obj.insert(
        "members".into(),
        serde_json::to_value(&config.members).map_err(|e| e.to_string())?,
    );
    let pretty = serde_json::to_string_pretty(&root_val).map_err(|e| e.to_string())?;
    std::fs::write(&path, pretty + "\n").map_err(|e| e.to_string())?;
    Ok(())
}

/// Resolve absolute member roots. Empty members → single-project `[root]`.
pub fn member_roots(workspace_root: &Path) -> Vec<PathBuf> {
    let cfg = load_workspace_config(workspace_root);
    if cfg.members.is_empty() {
        return vec![workspace_root.to_path_buf()];
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for m in &cfg.members {
        let abs = workspace_root.join(&m.path);
        let key = abs.to_string_lossy().to_string();
        if seen.insert(key) {
            out.push(abs);
        }
    }
    out
}

/// Discover candidate members: Cargo workspace crates + nested `.ax/` dirs.
pub fn discover_members(workspace_root: &Path) -> Vec<WorkspaceMember> {
    let mut by_path: HashMap<String, WorkspaceMember> = HashMap::new();

    for (name, rel) in ax_resolution::frameworks::cargo_workspace::load_crate_map(workspace_root) {
        let path = clean_rel(&rel);
        if path.is_empty() || path == "." {
            continue;
        }
        let prefer_name = !name.contains('_') || name.contains('-');
        match by_path.get_mut(&path) {
            Some(existing) => {
                if prefer_name {
                    existing.name = Some(name);
                }
            }
            None => {
                by_path.insert(
                    path.clone(),
                    WorkspaceMember {
                        path,
                        name: Some(name),
                    },
                );
            }
        }
    }

    let mut seen: HashSet<String> = by_path.keys().cloned().collect();
    let mut nested = Vec::new();
    discover_ax_dirs(workspace_root, workspace_root, 0, &mut nested, &mut seen);
    for m in nested {
        by_path.entry(m.path.clone()).or_insert(m);
    }

    let mut members: Vec<WorkspaceMember> = by_path.into_values().collect();
    members.sort_by(|a, b| a.path.cmp(&b.path));
    members
}

fn discover_ax_dirs(
    workspace_root: &Path,
    dir: &Path,
    depth: usize,
    out: &mut Vec<WorkspaceMember>,
    seen: &mut HashSet<String>,
) {
    if depth > MAX_DISCOVERY_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if SKIP_DIRS.contains(&name.as_str()) || name.starts_with('.') {
            continue;
        }
        if path.join(".ax").is_dir() {
            if let Ok(rel) = path.strip_prefix(workspace_root) {
                let rel_s = clean_rel(&rel.to_string_lossy());
                if !rel_s.is_empty() && seen.insert(rel_s.clone()) {
                    out.push(WorkspaceMember {
                        name: Some(name),
                        path: rel_s,
                    });
                }
            }
        }
        discover_ax_dirs(workspace_root, &path, depth + 1, out, seen);
    }
}

fn clean_rel(s: &str) -> String {
    s.replace('\\', "/").trim_matches('/').to_string()
}

/// Walk upward from `start` looking for a workspace config with members.
pub fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut cur = start.to_path_buf();
    loop {
        let cfg = load_workspace_config(&cur);
        if !cfg.members.is_empty() {
            return Some(cur);
        }
        if !cur.pop() {
            break;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = WorkspaceConfig {
            members: vec![
                WorkspaceMember {
                    path: "services/api".into(),
                    name: Some("api".into()),
                },
                WorkspaceMember {
                    path: "services/web".into(),
                    name: None,
                },
            ],
        };
        write_workspace_config(dir.path(), &cfg).unwrap();
        let loaded = load_workspace_config(dir.path());
        assert_eq!(loaded.members.len(), 2);
        assert_eq!(loaded.members[0].path, "services/api");
        assert_eq!(loaded.members[0].name.as_deref(), Some("api"));
    }

    #[test]
    fn member_roots_falls_back_to_root() {
        let dir = tempfile::tempdir().unwrap();
        let roots = member_roots(dir.path());
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0], dir.path());
    }

    #[test]
    fn discover_nested_ax() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("svc-a");
        let b = dir.path().join("svc-b");
        std::fs::create_dir_all(a.join(".ax")).unwrap();
        std::fs::create_dir_all(b.join(".ax")).unwrap();
        let found = discover_members(dir.path());
        assert!(found.iter().any(|m| m.path == "svc-a"));
        assert!(found.iter().any(|m| m.path == "svc-b"));
    }
}
