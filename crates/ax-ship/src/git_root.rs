//! Resolve git repositories for the quality gate (workspace may contain multiple repos).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ax_git::{diff_vs_base, ChangedFile, ChangedHunk, GitContext, GitError};
use ax_remote::ShipConfig;

/// Relative folder names under `workspace` that contain a `.git` directory (depth 1).
pub fn discover_git_repos(workspace: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(workspace) else {
        return Vec::new();
    };

    let mut repos: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !entry.file_type().ok()?.is_dir() {
                return None;
            }
            if !path.join(".git").exists() {
                return None;
            }
            path.file_name()?.to_str().map(str::to_string)
        })
        .collect();

    repos.sort();
    repos
}

/// Fill `ship.git_roots` from discovery and merge newly cloned repos into the list.
/// Persists to `.ax/ship.toml` when roots change.
pub fn sync_discovered_git_roots(workspace: &Path, config: &mut ShipConfig) -> Vec<String> {
    let discovered = discover_git_repos(workspace);

    if workspace.join(".git").exists() {
        return discovered;
    }

    let has_explicit = !config.ship.git_roots.is_empty()
        || config
            .ship
            .git_root
            .as_ref()
            .is_some_and(|s| !s.trim().is_empty());

    if !has_explicit && !discovered.is_empty() {
        config.ship.git_roots = discovered.clone();
        let _ = ax_remote::save_ship_config(workspace, config);
        return discovered;
    }

    if !discovered.is_empty() {
        let existing: HashSet<String> = discovered.iter().cloned().collect();
        config.ship.git_roots.retain(|r| existing.contains(r));
        let mut changed = false;
        for repo in &discovered {
            if !config.ship.git_roots.contains(repo) {
                config.ship.git_roots.push(repo.clone());
                changed = true;
            }
        }
        if changed || config.ship.git_roots.len() != existing.len() {
            config.ship.git_roots.sort();
            let _ = ax_remote::save_ship_config(workspace, config);
        }
    }

    resolve_sonar_repo_names(workspace, config)
}

/// Git repo folder names for Sonar provisioning — configured roots plus any autodiscovered repos.
/// Repos listed in `sonar.exclude_repos` are filtered out.
pub fn resolve_sonar_repo_names(workspace: &Path, config: &ShipConfig) -> Vec<String> {
    let discovered = discover_git_repos(workspace);
    let existing: HashSet<String> = discovered.iter().cloned().collect();
    let mut names = if config.ship.git_roots.is_empty() {
        discovered
    } else {
        let mut merged: Vec<String> = config
            .ship
            .git_roots
            .iter()
            .filter(|r| existing.contains(*r))
            .cloned()
            .collect();
        for repo in discovered {
            if !merged.contains(&repo) {
                merged.push(repo);
            }
        }
        merged.sort();
        merged.dedup();
        merged
    };
    let excluded: HashSet<&str> = config.sonar.exclude_repos.iter().map(|s| s.as_str()).collect();
    if !excluded.is_empty() {
        names.retain(|name| !excluded.contains(name.as_str()));
    }
    names
}

/// All git repo paths used by the quality gate.
pub fn resolve_git_roots(workspace: &Path, config: &ShipConfig) -> Result<Vec<PathBuf>, String> {
    if !config.ship.git_roots.is_empty() {
        return resolve_paths(workspace, &config.ship.git_roots);
    }

    if let Some(single) = config.ship.git_root.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        return resolve_paths(workspace, &[single.to_string()]);
    }

    if workspace.join(".git").exists() {
        return Ok(vec![workspace.to_path_buf()]);
    }

    let discovered = discover_git_repos(workspace);
    if discovered.is_empty() {
        return Err(format!(
            "{} is not a git repository and contains no child folders with .git. \
             Clone repos into this workspace or set ship.git_roots in .ax/ship.toml.",
            workspace.display()
        ));
    }

    resolve_paths(workspace, &discovered)
}

/// Backward-compatible single-repo resolver (first of `resolve_git_roots`).
pub fn resolve_git_root(workspace: &Path, config: &ShipConfig) -> Result<PathBuf, String> {
    resolve_git_roots(workspace, config)?
        .into_iter()
        .next()
        .ok_or_else(|| "no git repositories configured".into())
}

pub fn resolve_git_root_from(workspace: &Path, configured: Option<&str>) -> Result<PathBuf, String> {
    if let Some(single) = configured.map(str::trim).filter(|s| !s.is_empty()) {
        return resolve_paths(workspace, &[single.to_string()])?
            .into_iter()
            .next()
            .ok_or_else(|| "no git repositories configured".into());
    }
    let mut config = ax_remote::load_ship_config(workspace);
    sync_discovered_git_roots(workspace, &mut config);
    resolve_git_root(workspace, &config)
}

fn resolve_paths(workspace: &Path, rel_paths: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut roots = Vec::with_capacity(rel_paths.len());
    for rel in rel_paths {
        let path = workspace.join(rel);
        if path.join(".git").exists() {
            roots.push(path);
        } else {
            return Err(format!(
                "ship.git_roots entry '{rel}' is not a git repository ({})",
                path.display()
            ));
        }
    }
    Ok(roots)
}

#[derive(Debug, Clone)]
pub struct MultiRepoDiff {
    pub context: Option<GitContext>,
    pub files: Vec<ChangedFile>,
    pub hunks: Vec<ChangedHunk>,
    pub repos_scanned: usize,
    pub repos_with_changes: usize,
}

/// Resolve the git diff base branch for a single repository.
pub fn resolve_repo_base_branch(
    repo_path: &Path,
    default_base: &str,
    overrides: &HashMap<String, String>,
) -> String {
    let repo_name = repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    if let Some(branch) = overrides.get(repo_name).filter(|b| !b.trim().is_empty()) {
        return branch.trim().to_string();
    }

    if let Some(branch) = detect_integration_branch(repo_path) {
        if git_ref_exists(repo_path, &branch) {
            return branch;
        }
    }

    let head = ax_git::current_branch(repo_path).ok().flatten();
    if head.as_deref().is_some_and(|h| h != "main" && h != "master") {
        if git_ref_exists(repo_path, "develop") {
            return "develop".into();
        }
    }

    if git_ref_exists(repo_path, default_base) {
        return default_base.to_string();
    }

    for candidate in ["develop", "main", "master"] {
        if git_ref_exists(repo_path, candidate) {
            return candidate.into();
        }
    }

    default_base.to_string()
}

fn git_ref_exists(repo_path: &Path, branch: &str) -> bool {
    std::process::Command::new("git")
        .current_dir(repo_path)
        .args(["rev-parse", "--verify", &format!("{branch}^{{commit}}")])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Integration branch from `origin/HEAD` (e.g. `develop`, `main`).
fn detect_integration_branch(repo_path: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .current_dir(repo_path)
        .args(["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let symref = String::from_utf8_lossy(&output.stdout).trim().to_string();
    symref.strip_prefix("origin/").map(str::to_string)
}

/// Run `git diff` against each repo's resolved base branch and merge paths as `{repo_name}/{file}`.
pub fn diff_all_repos(
    git_roots: &[PathBuf],
    default_base: &str,
    per_repo: &HashMap<String, String>,
) -> Result<MultiRepoDiff, String> {
    let mut all_files = Vec::new();
    let mut all_hunks = Vec::new();
    let mut primary_context = None;
    let mut repos_with_changes = 0;
    let mut errors = Vec::new();

    for root in git_roots {
        let prefix = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("repo")
            .to_string();
        let base = resolve_repo_base_branch(root, default_base, per_repo);

        match diff_vs_base(root, &base) {
            Ok(diff) => {
                if primary_context.is_none() {
                    primary_context = Some(diff.context);
                }
                if !diff.files.is_empty() {
                    repos_with_changes += 1;
                }
                for mut file in diff.files {
                    file.path = format!("{prefix}/{}", file.path);
                    all_files.push(file);
                }
                for mut hunk in diff.hunks {
                    hunk.path = format!("{prefix}/{}", hunk.path);
                    all_hunks.push(hunk);
                }
            }
            Err(GitError::RefNotFound(_)) => {
                errors.push(format!("{prefix}: base ref '{base}' not found"));
            }
            Err(e) => errors.push(format!("{prefix}: {e}")),
        }
    }

    if git_roots.is_empty() {
        return Err("no git repositories to diff".into());
    }

    if all_files.is_empty() && !errors.is_empty() && errors.len() == git_roots.len() {
        return Err(errors.join("; "));
    }

    Ok(MultiRepoDiff {
        context: primary_context,
        files: all_files,
        hunks: all_hunks,
        repos_scanned: git_roots.len(),
        repos_with_changes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_workspace() -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("ax-git-discover-{n}"))
    }

    #[test]
    fn resolve_sonar_repo_names_skips_missing_git_roots() {
        let ws = temp_workspace();
        fs::create_dir_all(ws.join("alpha")).unwrap();
        fs::create_dir_all(ws.join("alpha/.git")).unwrap();
        fs::create_dir_all(ws.join("ghost")).unwrap();

        let config = ShipConfig {
            ship: ax_remote::ShipSection {
                git_roots: vec!["alpha".into(), "ghost".into(), "missing".into()],
                ..Default::default()
            },
            ..Default::default()
        };

        let names = resolve_sonar_repo_names(&ws, &config);
        assert_eq!(names, vec!["alpha"]);

        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn discovers_immediate_child_git_repos() {
        let ws = temp_workspace();
        fs::create_dir_all(ws.join("alpha")).unwrap();
        fs::create_dir_all(ws.join("beta")).unwrap();
        fs::create_dir_all(ws.join("alpha/.git")).unwrap();
        fs::create_dir_all(ws.join("beta/.git")).unwrap();
        fs::create_dir_all(ws.join("no-git")).unwrap();

        let repos = discover_git_repos(&ws);
        assert_eq!(repos, vec!["alpha", "beta"]);

        let _ = fs::remove_dir_all(&ws);
    }
}
