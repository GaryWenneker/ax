use std::path::{Path, PathBuf};

pub const POLICY_DIR: &str = "policy";
pub const RULES_DIR: &str = "rules";
pub const SKILLS_DIR: &str = "skills";
pub const SHARED_DIR: &str = "shared";
pub const PENDING_DIR: &str = "pending";
pub const SKILL_FILENAME: &str = "SKILL.md";
/// Body placeholder written into stub files that point at an external `source`.
pub const STUB_BODY_MARKER: &str = "<!-- ax:stub — body loaded from source -->";

/// The user's home directory (`AX_HOME_DIR` when set). Unit tests of this
/// crate get a private empty folder for the whole test process instead, so
/// they never read or write the developer's real `~/.ax`.
pub(crate) fn home_dir() -> Option<PathBuf> {
    #[cfg(test)]
    {
        Some(test_home().to_path_buf())
    }
    #[cfg(not(test))]
    {
        ax_utils::paths::home_dir()
    }
}

#[cfg(test)]
fn test_home() -> &'static Path {
    static HOME: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    HOME.get_or_init(|| {
        tempfile::Builder::new()
            .prefix("ax-policy-test-home")
            .tempdir()
            .expect("create the test home folder")
    })
    .path()
}

pub fn policy_root(ax_dir: &Path) -> PathBuf {
    ax_dir.join(POLICY_DIR)
}

pub fn rules_dir(ax_dir: &Path) -> PathBuf {
    policy_root(ax_dir).join(RULES_DIR)
}

pub fn skills_dir(ax_dir: &Path) -> PathBuf {
    policy_root(ax_dir).join(SKILLS_DIR)
}

/// Per-project git pack directory: `.ax/policy/shared/`.
pub fn shared_dir(ax_dir: &Path) -> PathBuf {
    policy_root(ax_dir).join(SHARED_DIR)
}

/// Pending review queue: `.ax/policy/pending/`.
pub fn pending_dir(ax_dir: &Path) -> PathBuf {
    policy_root(ax_dir).join(PENDING_DIR)
}

pub fn pending_rules_dir(ax_dir: &Path) -> PathBuf {
    pending_dir(ax_dir).join(RULES_DIR)
}

pub fn pending_skills_dir(ax_dir: &Path) -> PathBuf {
    pending_dir(ax_dir).join(SKILLS_DIR)
}

pub fn rule_file(rules: &Path, id: &str) -> PathBuf {
    rules.join(format!("{id}.mdc"))
}

pub fn skill_file(skills: &Path, name: &str) -> PathBuf {
    skills.join(name).join(SKILL_FILENAME)
}

pub fn ensure_scaffold(ax_dir: &Path) -> std::io::Result<()> {
    ensure_policy_dirs(ax_dir)?;
    let _ = crate::seed::seed_default_policy(ax_dir)?;
    Ok(())
}

/// Create `.ax/policy/` directories only — no template seeding (used on import / ensure).
pub fn ensure_policy_dirs(ax_dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(rules_dir(ax_dir))?;
    std::fs::create_dir_all(skills_dir(ax_dir))?;
    Ok(())
}

pub fn ax_dir_from_project(project_root: &Path) -> PathBuf {
    project_root.join(".ax")
}

/// Resolve a user path under `.ax/policy/` — reject traversal.
pub fn resolve_policy_path(base: &Path, relative: &str) -> Result<PathBuf, String> {
    let rel = relative.trim().replace('\\', "/");
    if rel.contains("..") || rel.starts_with('/') {
        return Err("invalid path".into());
    }
    let full = base.join(rel);
    let canon_base = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
    let canon_full = full
        .canonicalize()
        .or_else(|_| {
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            full.canonicalize()
        })
        .map_err(|e| e.to_string())?;
    if !canon_full.starts_with(&canon_base) {
        return Err("path outside policy directory".into());
    }
    Ok(canon_full)
}

/// Whether a markdown body is a stub pointer (body loaded from `source`).
pub fn is_stub_body(body: &str) -> bool {
    let t = body.trim();
    t.is_empty() || t.starts_with("<!-- ax:stub")
}

/// Resolve a `source:` frontmatter value.
///
/// Accepts:
/// - absolute paths (`D:/shared/rules/foo.mdc`) inside the project or a policy root
/// - `root:<id>/rules/foo.mdc` (relative to a configured policy root)
/// - project-relative paths (must stay under project or a configured root)
///
/// The result is both read (index) and written (save), so it is confined: it must
/// be a policy file (`.mdc` or `.md`), must not sit inside a `.git` folder, and must
/// resolve (following links) inside the project, a configured policy root or a
/// policy scope folder. A folder outside those is allowed by adding it as a root.
pub fn resolve_source_path(project_root: &Path, source: &str) -> Result<PathBuf, String> {
    let src = source.trim().replace('\\', "/");
    if src.is_empty() {
        return Err("empty source".into());
    }

    if let Some(rest) = src.strip_prefix("root:") {
        let (id, rel) = rest
            .split_once('/')
            .ok_or_else(|| "root: source must be root:<id>/<relative-path>".to_string())?;
        let root = crate::config::find_policy_root(project_root, id)
            .ok_or_else(|| format!("unknown policy root id '{id}'"))?;
        let full = ax_utils::contained_join(&root.path, rel.trim_start_matches('/'))
            .map_err(|e| e.to_string())?;
        return confine_source(project_root, source, full);
    }

    let path = PathBuf::from(&src);
    if path.is_absolute() {
        return confine_source(project_root, source, path);
    }

    // Relative: allow under project root or any configured root.
    let rel = ax_utils::safe_relative(&src).map_err(|e| e.to_string())?;
    let under_project = project_root.join(&rel);
    if under_project.exists() {
        return confine_source(project_root, source, under_project);
    }
    for root in crate::config::load_policy_roots(project_root) {
        let candidate = root.path.join(&rel);
        if candidate.exists() {
            return confine_source(project_root, source, candidate);
        }
    }
    confine_source(project_root, source, under_project)
}

/// Folders a `source:` may point into.
fn source_roots(project_root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![project_root.to_path_buf()];
    roots.extend(
        crate::config::load_policy_roots(project_root)
            .into_iter()
            .map(|r| r.path),
    );
    roots.extend(crate::hierarchy::policy_layer_dirs(project_root));
    roots
}

fn confine_source(project_root: &Path, source: &str, path: PathBuf) -> Result<PathBuf, String> {
    let refused = || {
        format!(
            "source '{source}' must be a .mdc or .md policy file inside the project or a configured policy root"
        )
    };
    let is_policy_file = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("mdc") || e.eq_ignore_ascii_case("md"));
    let in_git_dir = path
        .components()
        .any(|c| c.as_os_str().eq_ignore_ascii_case(".git"));
    if !is_policy_file || in_git_dir {
        return Err(refused());
    }
    // Resolve links on the deepest existing part, so a link out of a root is caught.
    let Some(resolved) = real_path(&path) else {
        return Err(refused());
    };
    let inside = source_roots(project_root).iter().any(|root| {
        let real_root = root.canonicalize().unwrap_or_else(|_| root.clone());
        resolved.starts_with(&real_root)
    });
    let resolved_in_git = resolved
        .components()
        .any(|c| c.as_os_str().eq_ignore_ascii_case(".git"));
    if !inside || resolved_in_git {
        return Err(refused());
    }
    Ok(resolved)
}

/// `path` with links resolved on its deepest existing ancestor; the missing tail
/// is appended as is. `None` for a dangling link or a `..` in the tail.
fn real_path(path: &Path) -> Option<PathBuf> {
    let mut probe = path.to_path_buf();
    let mut tail = Vec::new();
    loop {
        match probe.canonicalize() {
            Ok(real) => {
                let mut out = real;
                for part in tail.iter().rev() {
                    out.push(part);
                }
                return Some(out);
            }
            Err(_) => {
                if std::fs::symlink_metadata(&probe).is_ok() {
                    return None;
                }
                let name = probe.file_name()?.to_os_string();
                if name == ".." {
                    return None;
                }
                tail.push(name);
                if !probe.pop() {
                    return None;
                }
            }
        }
    }
}

/// Resolve the on-disk write target for a file-backed item.
pub fn resolve_item_write_dir(
    project_root: &Path,
    scope: crate::types::PolicyScope,
    root_id: Option<&str>,
) -> Result<PathBuf, String> {
    if let Some(id) = root_id.filter(|s| !s.is_empty()) {
        let root = crate::config::find_policy_root(project_root, id)
            .ok_or_else(|| format!("unknown policy root id '{id}'"))?;
        std::fs::create_dir_all(root.path.join(RULES_DIR)).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(root.path.join(SKILLS_DIR)).map_err(|e| e.to_string())?;
        return Ok(root.path);
    }
    crate::hierarchy::ensure_scope_dirs(project_root, scope).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_body_detection() {
        assert!(is_stub_body(STUB_BODY_MARKER));
        assert!(is_stub_body(""));
        assert!(!is_stub_body("Real rule body"));
    }

    #[test]
    fn resolve_root_source() {
        let dir = tempfile::tempdir().unwrap();
        let external = dir.path().join("ext");
        std::fs::create_dir_all(external.join("rules")).unwrap();
        let target = external.join("rules/foo.mdc");
        std::fs::write(&target, "x").unwrap();
        let abs = external.display().to_string().replace('\\', "/");
        std::fs::write(
            dir.path().join("ax.json"),
            format!(r#"{{"policy":{{"roots":[{{"id":"shared","path":"{abs}"}}]}}}}"#),
        )
        .unwrap();
        let resolved = resolve_source_path(dir.path(), "root:shared/rules/foo.mdc").unwrap();
        assert!(resolved.ends_with("foo.mdc"));
    }
    #[test]
    fn an_absolute_source_outside_project_and_roots_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("config.mdc");
        std::fs::write(&target, "x").unwrap();
        let abs = target.display().to_string();
        assert!(resolve_source_path(dir.path(), &abs).is_err());
    }

    #[test]
    fn an_absolute_source_inside_a_configured_root_is_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(external.path().join("rules")).unwrap();
        let target = external.path().join("rules/foo.mdc");
        std::fs::write(&target, "x").unwrap();
        let root = external.path().display().to_string().replace('\\', "/");
        std::fs::write(
            dir.path().join("ax.json"),
            format!(r#"{{"policy":{{"roots":[{{"id":"shared","path":"{root}"}}]}}}}"#),
        )
        .unwrap();
        let abs = target.display().to_string();
        assert!(resolve_source_path(dir.path(), &abs).is_ok());
    }

    #[test]
    fn a_source_that_is_not_a_policy_file_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".git/hooks")).unwrap();
        std::fs::write(dir.path().join(".git/hooks/post-merge"), "#!/bin/sh\n").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.path().join(".git/hooks/x.mdc"), "x").unwrap();
        assert!(resolve_source_path(dir.path(), ".git/hooks/post-merge").is_err());
        assert!(resolve_source_path(dir.path(), "src/main.rs").is_err());
        assert!(resolve_source_path(dir.path(), ".git/hooks/x.mdc").is_err());
    }

    #[test]
    fn a_project_source_that_is_a_policy_file_is_allowed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("shared/rules")).unwrap();
        std::fs::write(dir.path().join("shared/rules/foo.mdc"), "x").unwrap();
        std::fs::create_dir_all(dir.path().join("shared/skills/bar")).unwrap();
        std::fs::write(dir.path().join("shared/skills/bar/SKILL.md"), "x").unwrap();
        assert!(resolve_source_path(dir.path(), "shared/rules/foo.mdc").is_ok());
        assert!(resolve_source_path(dir.path(), "shared/skills/bar/SKILL.md").is_ok());
        assert!(resolve_source_path(dir.path(), "shared/rules/new.mdc").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn a_project_source_linked_outside_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.mdc"), "x").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        assert!(resolve_source_path(dir.path(), "link/secret.mdc").is_err());
    }

    #[test]
    fn a_root_source_with_an_absolute_rest_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let external = dir.path().join("ext");
        std::fs::create_dir_all(&external).unwrap();
        let abs = external.display().to_string().replace('\\', "/");
        std::fs::write(
            dir.path().join("ax.json"),
            format!(r#"{{"policy":{{"roots":[{{"id":"shared","path":"{abs}"}}]}}}}"#),
        )
        .unwrap();
        assert!(resolve_source_path(dir.path(), "root:shared/C:/x.mdc").is_err());
    }
}
