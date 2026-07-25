//! Hierarchical policy layers: global → workspace → member.
//!
//! Precedence (later wins on same rule/skill id via upsert):
//! 1. `~/.ax/global_policy/`
//! 2. Workspace root `.ax/policy/` (when `ax.json` has `members`)
//! 3. Project / member `.ax/policy/`

use std::path::{Path, PathBuf};

const CONFIG_FILENAME: &str = "ax.json";
const CONFIG_ALIAS: &str = ".ax.json";

/// Ordered policy directory roots to merge (lowest → highest precedence).
pub fn policy_layer_dirs(project_root: &Path) -> Vec<PathBuf> {
    let mut layers = Vec::new();

    if let Some(home) = dirs::home_dir() {
        let global = home.join(".ax").join("global_policy");
        if global.is_dir() {
            layers.push(global);
        }
    }

    if let Some(ws) = find_workspace_root(project_root) {
        let ws_policy = ws.join(".ax").join("policy");
        if ws_policy.is_dir() {
            // Avoid duplicating the member's own policy when project_root == workspace.
            let member_policy = project_root.join(".ax").join("policy");
            if ws_policy != member_policy {
                layers.push(ws_policy);
            }
        }
    }

    let local = project_root.join(".ax").join("policy");
    if local.is_dir() {
        layers.push(local);
    }

    layers
}

/// Walk upward for `ax.json` / `.ax.json` containing a non-empty `members` array.
pub fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut cur = start.to_path_buf();
    loop {
        if workspace_has_members(&cur) {
            return Some(cur);
        }
        if !cur.pop() {
            break;
        }
    }
    None
}

fn workspace_has_members(root: &Path) -> bool {
    for name in [CONFIG_FILENAME, CONFIG_ALIAS] {
        let path = root.join(name);
        if !path.is_file() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };
        if let Some(members) = v.get("members").and_then(|m| m.as_array()) {
            if !members.is_empty() {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_workspace_with_members() {
        let dir = tempfile::tempdir().unwrap();
        let member = dir.path().join("svc");
        std::fs::create_dir_all(member.join(".ax/policy")).unwrap();
        std::fs::write(
            dir.path().join("ax.json"),
            r#"{"members":[{"path":"svc"}]}"#,
        )
        .unwrap();
        assert_eq!(
            find_workspace_root(&member).as_deref(),
            Some(dir.path())
        );
        let layers = policy_layer_dirs(&member);
        assert!(
            layers.iter().any(|p| p.join("").starts_with(member.join(".ax").as_path()) || p == &member.join(".ax").join("policy")),
            "expected member policy in layers: {layers:?}"
        );
    }
}
