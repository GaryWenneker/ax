use std::path::{Path, PathBuf};

pub const POLICY_DIR: &str = "policy";
pub const RULES_DIR: &str = "rules";
pub const SKILLS_DIR: &str = "skills";
pub const SHARED_DIR: &str = "shared";
pub const PENDING_DIR: &str = "pending";
pub const SKILL_FILENAME: &str = "SKILL.md";
/// Body placeholder written into stub files that point at an external `source`.
pub const STUB_BODY_MARKER: &str = "<!-- ax:stub — body loaded from source -->";

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
    let canon_base = base
        .canonicalize()
        .unwrap_or_else(|_| base.to_path_buf());
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
/// - absolute paths (`D:/shared/rules/foo.mdc`)
/// - `root:<id>/rules/foo.mdc` (relative to a configured policy root)
/// - project-relative paths (must stay under project or a configured root)
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
        let rel = rel.trim_start_matches('/');
        if rel.contains("..") {
            return Err("invalid path".into());
        }
        let full = root.path.join(rel);
        return Ok(full.canonicalize().unwrap_or(full));
    }

    let path = PathBuf::from(&src);
    if path.is_absolute() {
        return Ok(path.canonicalize().unwrap_or(path));
    }

    // Relative: allow under project root or any configured root.
    if src.contains("..") {
        return Err("invalid path".into());
    }
    let under_project = project_root.join(&src);
    if under_project.exists() {
        return Ok(under_project.canonicalize().unwrap_or(under_project));
    }
    for root in crate::config::load_policy_roots(project_root) {
        let candidate = root.path.join(&src);
        if candidate.exists() {
            return Ok(candidate.canonicalize().unwrap_or(candidate));
        }
    }
    Ok(under_project)
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
}
