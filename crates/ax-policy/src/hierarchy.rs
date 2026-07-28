//! Hierarchical policy layers: company → workspace → project → private.
//!
//! Precedence (later wins on same rule/skill id via upsert):
//! 1. `~/.ax/global_policy/` (company)
//! 2. Workspace root `.ax/policy/` (when `ax.json` has `members`)
//! 3. Project / member `.ax/policy/`
//! 4. `~/.ax/private_policy/` (private user)
//! 5. `<project>/.ax/policy-private/` (private project)

use std::path::{Path, PathBuf};

use crate::types::PolicyScope;

const CONFIG_FILENAME: &str = "ax.json";
const CONFIG_ALIAS: &str = ".ax.json";

/// One merge layer: directory root + scope stamp for DB rows.
#[derive(Debug, Clone)]
pub struct PolicyLayer {
    pub dir: PathBuf,
    pub scope: PolicyScope,
}

/// Ordered policy directory roots to merge (lowest → highest precedence).
/// Only includes directories that already exist (private dirs are created on first save).
pub fn policy_layer_dirs(project_root: &Path) -> Vec<PathBuf> {
    policy_layers(project_root)
        .into_iter()
        .map(|l| l.dir)
        .collect()
}

/// Ordered layers with scope metadata (lowest → highest precedence).
pub fn policy_layers(project_root: &Path) -> Vec<PolicyLayer> {
    let mut layers = Vec::new();

    if let Some(home) = dirs::home_dir() {
        let global = home.join(".ax").join("global_policy");
        if global.is_dir() {
            layers.push(PolicyLayer {
                dir: global,
                scope: PolicyScope::Company,
            });
        }
    }

    if let Some(ws) = find_workspace_root(project_root) {
        let ws_policy = ws.join(".ax").join("policy");
        if ws_policy.is_dir() {
            let member_policy = project_root.join(".ax").join("policy");
            if ws_policy != member_policy {
                layers.push(PolicyLayer {
                    dir: ws_policy,
                    scope: PolicyScope::Workspace,
                });
            }
        }
    }

    let local = project_root.join(".ax").join("policy");
    if local.is_dir() {
        layers.push(PolicyLayer {
            dir: local,
            scope: PolicyScope::Project,
        });
    }

    if let Some(home) = dirs::home_dir() {
        let private_user = home.join(".ax").join("private_policy");
        if private_user.is_dir() {
            layers.push(PolicyLayer {
                dir: private_user,
                scope: PolicyScope::PrivateUser,
            });
        }
    }

    let private_project = project_root.join(".ax").join("policy-private");
    if private_project.is_dir() {
        layers.push(PolicyLayer {
            dir: private_project,
            scope: PolicyScope::PrivateProject,
        });
    }

    layers
}

/// Resolve the on-disk policy directory for a scope (may not exist yet).
pub fn policy_dir_for_scope(project_root: &Path, scope: PolicyScope) -> PathBuf {
    match scope {
        PolicyScope::Company => dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".ax")
            .join("global_policy"),
        PolicyScope::Workspace => find_workspace_root(project_root)
            .unwrap_or_else(|| project_root.to_path_buf())
            .join(".ax")
            .join("policy"),
        PolicyScope::Project => project_root.join(".ax").join("policy"),
        PolicyScope::PrivateUser => dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".ax")
            .join("private_policy"),
        PolicyScope::PrivateProject => project_root.join(".ax").join("policy-private"),
    }
}

/// Ensure the policy directory for a scope exists (rules/ + skills/).
pub fn ensure_scope_dirs(project_root: &Path, scope: PolicyScope) -> std::io::Result<PathBuf> {
    let dir = policy_dir_for_scope(project_root, scope);
    std::fs::create_dir_all(dir.join("rules"))?;
    std::fs::create_dir_all(dir.join("skills"))?;
    if scope == PolicyScope::PrivateProject {
        ensure_private_gitignore(project_root)?;
    }
    Ok(dir)
}

/// Ensure `.ax/.gitignore` ignores `policy-private/`.
pub fn ensure_private_gitignore(project_root: &Path) -> std::io::Result<()> {
    let ax = project_root.join(".ax");
    std::fs::create_dir_all(&ax)?;
    let gi = ax.join(".gitignore");
    let marker = "policy-private/";
    if gi.is_file() {
        let content = std::fs::read_to_string(&gi)?;
        if content.lines().any(|l| l.trim() == marker) {
            return Ok(());
        }
        let mut next = content;
        if !next.ends_with('\n') && !next.is_empty() {
            next.push('\n');
        }
        next.push_str(marker);
        next.push('\n');
        std::fs::write(&gi, next.as_bytes())?;
    } else {
        std::fs::write(&gi, format!("{marker}\n"))?;
    }
    Ok(())
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
        let layers = policy_layers(&member);
        assert!(
            layers.iter().any(|l| l.scope == PolicyScope::Project),
            "expected project policy in layers: {layers:?}"
        );
    }

    #[test]
    fn private_project_layer_and_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let private = ensure_scope_dirs(root, PolicyScope::PrivateProject).unwrap();
        assert!(private.join("rules").is_dir());
        let layers = policy_layers(root);
        assert!(layers.iter().any(|l| l.scope == PolicyScope::PrivateProject));
        let gi = std::fs::read_to_string(root.join(".ax/.gitignore")).unwrap();
        assert!(gi.contains("policy-private/"));
    }

    #[test]
    fn scope_parse_aliases() {
        assert_eq!(PolicyScope::parse("global"), Some(PolicyScope::Company));
        assert_eq!(PolicyScope::parse("private"), Some(PolicyScope::PrivateProject));
        assert_eq!(PolicyScope::parse("private_user"), Some(PolicyScope::PrivateUser));
    }

    #[test]
    fn layer_precedence_order() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".ax/policy/rules")).unwrap();
        ensure_scope_dirs(root, PolicyScope::PrivateProject).unwrap();
        let layers = policy_layers(root);
        let scopes: Vec<_> = layers.iter().map(|l| l.scope).collect();
        assert!(scopes.contains(&PolicyScope::Project));
        assert!(scopes.contains(&PolicyScope::PrivateProject));
        let project_i = scopes.iter().position(|s| *s == PolicyScope::Project).unwrap();
        let private_i = scopes
            .iter()
            .position(|s| *s == PolicyScope::PrivateProject)
            .unwrap();
        assert!(private_i > project_i, "private_project must win over project");
    }
}
