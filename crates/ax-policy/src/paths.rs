use std::path::{Path, PathBuf};

pub const POLICY_DIR: &str = "policy";
pub const RULES_DIR: &str = "rules";
pub const SKILLS_DIR: &str = "skills";
pub const SKILL_FILENAME: &str = "SKILL.md";

pub fn policy_root(ax_dir: &Path) -> PathBuf {
    ax_dir.join(POLICY_DIR)
}

pub fn rules_dir(ax_dir: &Path) -> PathBuf {
    policy_root(ax_dir).join(RULES_DIR)
}

pub fn skills_dir(ax_dir: &Path) -> PathBuf {
    policy_root(ax_dir).join(SKILLS_DIR)
}

pub fn rule_file(rules: &Path, id: &str) -> PathBuf {
    rules.join(format!("{id}.mdc"))
}

pub fn skill_file(skills: &Path, name: &str) -> PathBuf {
    skills.join(name).join(SKILL_FILENAME)
}

pub fn ensure_scaffold(ax_dir: &Path) -> std::io::Result<()> {
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
