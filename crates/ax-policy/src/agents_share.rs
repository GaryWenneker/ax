//! Git-shared `.agents` layout, legacy `.ax/policy` migrate, inactive relocation, leak gate.

use std::path::{Path, PathBuf};

use crate::parse::{parse_rule_file, parse_skill_file};
use crate::paths::{RULES_DIR, SKILL_FILENAME, SKILLS_DIR};
use crate::types::PolicyScope;

pub const AGENTS_DIR: &str = ".agents";
pub const POLICY_INACTIVE_DIR: &str = "policy-inactive";
pub const LEGACY_POLICY_DIR: &str = "policy";

const GITIGNORE_MARKERS: &[&str] = &["policy-private/", "policy-inactive/"];

pub fn agents_dir(project_root: &Path) -> PathBuf {
    project_root.join(AGENTS_DIR)
}

pub fn legacy_policy_dir(project_root: &Path) -> PathBuf {
    project_root.join(".ax").join(LEGACY_POLICY_DIR)
}

pub fn inactive_dir(project_root: &Path) -> PathBuf {
    project_root.join(".ax").join(POLICY_INACTIVE_DIR)
}

pub fn is_git_export_candidate(scope: PolicyScope, enabled: bool) -> bool {
    enabled && scope.is_packable()
}

/// Directory that should receive a file-backed item for this scope + enabled flag.
pub fn resolve_shareable_write_dir(
    project_root: &Path,
    scope: PolicyScope,
    enabled: bool,
) -> PathBuf {
    if !enabled && scope.is_packable() {
        return inactive_dir(project_root);
    }
    crate::hierarchy::policy_dir_for_scope(project_root, scope)
}

pub fn ensure_ax_share_gitignore(project_root: &Path) -> std::io::Result<()> {
    let ax = project_root.join(".ax");
    std::fs::create_dir_all(&ax)?;
    let gi = ax.join(".gitignore");
    let mut content = if gi.is_file() {
        std::fs::read_to_string(&gi)?
    } else {
        String::new()
    };
    let mut changed = false;
    for marker in GITIGNORE_MARKERS {
        if content.lines().any(|l| l.trim() == *marker) {
            continue;
        }
        if !content.ends_with('\n') && !content.is_empty() {
            content.push('\n');
        }
        content.push_str(marker);
        content.push('\n');
        changed = true;
    }
    if changed || !gi.is_file() {
        std::fs::write(&gi, content.as_bytes())?;
    }
    Ok(())
}

/// Move leftover `.ax/policy/{rules,skills}` files into `.agents` without overwriting.
pub fn migrate_legacy_policy_to_agents(project_root: &Path) -> std::io::Result<Vec<String>> {
    let mut moved = Vec::new();
    let legacy = legacy_policy_dir(project_root);
    let dest_root = agents_dir(project_root);
    for sub in [RULES_DIR, SKILLS_DIR] {
        let from = legacy.join(sub);
        if !from.is_dir() {
            continue;
        }
        let to = dest_root.join(sub);
        std::fs::create_dir_all(&to)?;
        for entry in std::fs::read_dir(&from)? {
            let entry = entry?;
            let src = entry.path();
            let name = entry.file_name();
            let dest = if should_send_legacy_to_inactive(&src) {
                let inactive_sub = inactive_dir(project_root).join(sub);
                std::fs::create_dir_all(&inactive_sub)?;
                inactive_sub.join(&name)
            } else {
                to.join(&name)
            };
            if dest.exists() {
                continue;
            }
            std::fs::rename(&src, &dest)?;
            moved.push(format!("{sub}/{}", name.to_string_lossy()));
        }
    }
    Ok(moved)
}

fn should_send_legacy_to_inactive(path: &Path) -> bool {
    if path.is_dir() {
        let skill_md = path.join(SKILL_FILENAME);
        if !skill_md.is_file() {
            return false;
        }
        let Ok(raw) = std::fs::read_to_string(&skill_md) else {
            return false;
        };
        return parse_skill_file(&skill_md, &raw)
            .map(|d| !d.frontmatter.enabled || !packable_scope(&d.frontmatter.scope))
            .unwrap_or(false);
    }
    let Ok(raw) = std::fs::read_to_string(path) else {
        return false;
    };
    parse_rule_file(path, &raw)
        .map(|d| !d.frontmatter.enabled || !packable_scope(&d.frontmatter.scope))
        .unwrap_or(false)
}

fn packable_scope(scope: &str) -> bool {
    PolicyScope::parse(scope)
        .unwrap_or(PolicyScope::Project)
        .is_packable()
}

fn rule_path(root: &Path, id: &str) -> PathBuf {
    root.join(RULES_DIR).join(format!("{id}.mdc"))
}

fn skill_dir(root: &Path, name: &str) -> PathBuf {
    root.join(SKILLS_DIR).join(name)
}

fn remove_path(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// Write `keep` and delete the same rule id from other shareable trees.
pub fn relocate_rule_file(project_root: &Path, id: &str, keep: &Path) -> std::io::Result<()> {
    let candidates = [
        rule_path(&agents_dir(project_root), id),
        rule_path(&legacy_policy_dir(project_root), id),
        rule_path(&inactive_dir(project_root), id),
    ];
    for p in candidates {
        if p != *keep {
            remove_path(&p)?;
        }
    }
    Ok(())
}

pub fn relocate_skill_dir(project_root: &Path, name: &str, keep: &Path) -> std::io::Result<()> {
    let candidates = [
        skill_dir(&agents_dir(project_root), name),
        skill_dir(&legacy_policy_dir(project_root), name),
        skill_dir(&inactive_dir(project_root), name),
    ];
    for p in candidates {
        if p != *keep {
            remove_path(&p)?;
        }
    }
    Ok(())
}

/// Fail-closed leak scan: private or disabled items must not sit under `.agents`.
pub fn agents_share_violations(project_root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let rules = agents_dir(project_root).join(RULES_DIR);
    if rules.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&rules) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("mdc") {
                    continue;
                }
                let Ok(raw) = std::fs::read_to_string(&path) else {
                    out.push(format!("unreadable {}", path.display()));
                    continue;
                };
                match parse_rule_file(&path, &raw) {
                    Ok(doc) => {
                        if !doc.frontmatter.enabled {
                            out.push(format!(
                                "disabled rule {} under .agents/rules",
                                doc.frontmatter.id
                            ));
                        }
                        if let Some(scope) = PolicyScope::parse(&doc.frontmatter.scope) {
                            if !scope.is_packable() {
                                out.push(format!(
                                    "non-packable scope {} on {} under .agents/rules",
                                    scope.as_str(),
                                    doc.frontmatter.id
                                ));
                            }
                        }
                    }
                    Err(e) => out.push(format!("invalid rule {}: {}", path.display(), e.error)),
                }
            }
        }
    }
    let skills = agents_dir(project_root).join(SKILLS_DIR);
    if skills.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&skills) {
            for entry in entries.flatten() {
                if !entry.path().is_dir() {
                    continue;
                }
                let skill_md = entry.path().join(SKILL_FILENAME);
                if !skill_md.is_file() {
                    continue;
                }
                let Ok(raw) = std::fs::read_to_string(&skill_md) else {
                    out.push(format!("unreadable {}", skill_md.display()));
                    continue;
                };
                match parse_skill_file(&skill_md, &raw) {
                    Ok(doc) => {
                        if !doc.frontmatter.enabled {
                            out.push(format!(
                                "disabled skill {} under .agents/skills",
                                doc.frontmatter.name
                            ));
                        }
                        if let Some(scope) = PolicyScope::parse(&doc.frontmatter.scope) {
                            if !scope.is_packable() {
                                out.push(format!(
                                    "non-packable scope {} on skill {} under .agents/skills",
                                    scope.as_str(),
                                    doc.frontmatter.name
                                ));
                            }
                        }
                    }
                    Err(e) => out.push(format!("invalid skill {}: {}", skill_md.display(), e.error)),
                }
            }
        }
    }
    out
}

/// Point `.cursor/skills/<name>` at `.agents/skills/<name>` via symlink.
pub fn link_cursor_skills_to_agents(project_root: &Path) -> Result<Vec<String>, String> {
    let agents_skills = agents_dir(project_root).join(SKILLS_DIR);
    if !agents_skills.is_dir() {
        return Ok(vec![]);
    }
    let cursor_skills = project_root.join(".cursor").join("skills");
    std::fs::create_dir_all(&cursor_skills).map_err(|e| e.to_string())?;
    let mut linked = Vec::new();
    let entries = std::fs::read_dir(&agents_skills).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name();
        let dest = cursor_skills.join(&name);
        let target = PathBuf::from("../../").join(AGENTS_DIR).join(SKILLS_DIR).join(&name);
        if dest.exists() || dest.symlink_metadata().is_ok() {
            if dest
                .symlink_metadata()
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
            {
                continue;
            }
            if dest.is_dir() {
                continue;
            }
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, &dest).map_err(|e| e.to_string())?;
            linked.push(name.to_string_lossy().into_owned());
        }
        #[cfg(not(unix))]
        {
            return Err(format!(
                "symlink {}.cursor/skills/{} -> {} failed: not unix",
                "",
                name.to_string_lossy(),
                target.display()
            ));
        }
    }
    Ok(linked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hierarchy::{ensure_scope_dirs, policy_dir_for_scope, policy_layers};
    use crate::types::PolicyScope;

    fn write_rule(path: &Path, id: &str, enabled: bool, scope: &str) {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).unwrap();
        }
        let enabled_s = if enabled { "true" } else { "false" };
        std::fs::write(
            path,
            format!(
                "---\nid: {id}\nlevel: INFO\nalwaysApply: true\nenabled: {enabled_s}\nscope: \"{scope}\"\n---\n\nbody\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn project_write_path_is_agents() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        assert_eq!(policy_dir_for_scope(p, PolicyScope::Project), agents_dir(p));
        let created = ensure_scope_dirs(p, PolicyScope::Project).unwrap();
        assert!(created.join("rules").is_dir());
        assert!(created.join("skills").is_dir());
        assert!(p.join(".agents/rules").is_dir());
    }

    #[test]
    fn workspace_write_path_is_workspace_agents() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path();
        let member = ws.join("svc");
        std::fs::create_dir_all(&member).unwrap();
        std::fs::write(ws.join("ax.json"), r#"{"members":[{"path":"svc"}]}"#).unwrap();
        assert_eq!(
            policy_dir_for_scope(&member, PolicyScope::Workspace),
            agents_dir(ws)
        );
    }

    #[test]
    fn private_and_company_paths_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        assert_eq!(
            policy_dir_for_scope(p, PolicyScope::PrivateProject),
            p.join(".ax/policy-private")
        );
        assert!(policy_dir_for_scope(p, PolicyScope::PrivateUser)
            .ends_with(std::path::Path::new(".ax/private_policy")));
        assert!(policy_dir_for_scope(p, PolicyScope::Company)
            .ends_with(std::path::Path::new(".ax/global_policy")));
    }

    #[test]
    fn gitignore_lists_private_and_inactive() {
        let dir = tempfile::tempdir().unwrap();
        ensure_ax_share_gitignore(dir.path()).unwrap();
        let gi = std::fs::read_to_string(dir.path().join(".ax/.gitignore")).unwrap();
        assert!(gi.contains("policy-private/"));
        assert!(gi.contains("policy-inactive/"));
    }

    #[test]
    fn layers_legacy_then_agents_then_inactive_then_private() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::create_dir_all(p.join(".ax/policy/rules")).unwrap();
        std::fs::create_dir_all(p.join(".agents/rules")).unwrap();
        std::fs::create_dir_all(p.join(".ax/policy-inactive/rules")).unwrap();
        ensure_scope_dirs(p, PolicyScope::PrivateProject).unwrap();
        let layers = policy_layers(p);
        let dirs: Vec<_> = layers.iter().map(|l| l.dir.clone()).collect();
        let i_legacy = dirs.iter().position(|d| d == &legacy_policy_dir(p)).unwrap();
        let i_agents = dirs.iter().position(|d| d == &agents_dir(p)).unwrap();
        let i_inactive = dirs.iter().position(|d| d == &inactive_dir(p)).unwrap();
        let i_private = dirs
            .iter()
            .position(|d| d.ends_with("policy-private"))
            .unwrap();
        assert!(i_legacy < i_agents);
        assert!(i_agents < i_inactive);
        assert!(i_inactive < i_private);
        assert!(layers[i_inactive].preserve_item_scope);
    }

    #[test]
    fn migrate_moves_rules_not_pending() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::create_dir_all(p.join(".ax/policy/rules")).unwrap();
        std::fs::create_dir_all(p.join(".ax/policy/pending/rules")).unwrap();
        write_rule(
            &p.join(".ax/policy/rules/foo.mdc"),
            "foo",
            true,
            "project",
        );
        std::fs::write(p.join(".ax/policy/pending/rules/bar.mdc"), "x").unwrap();
        let moved = migrate_legacy_policy_to_agents(p).unwrap();
        assert!(moved.iter().any(|m| m.contains("foo.mdc")));
        assert!(p.join(".agents/rules/foo.mdc").is_file());
        assert!(!p.join(".ax/policy/rules/foo.mdc").exists());
        assert!(p.join(".ax/policy/pending/rules/bar.mdc").is_file());
    }

    #[test]
    fn migrate_skips_when_destination_exists() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        std::fs::create_dir_all(p.join(".ax/policy/rules")).unwrap();
        std::fs::create_dir_all(p.join(".agents/rules")).unwrap();
        write_rule(&p.join(".ax/policy/rules/foo.mdc"), "foo", true, "project");
        write_rule(&p.join(".agents/rules/foo.mdc"), "foo", true, "project");
        let dest_before = std::fs::read_to_string(p.join(".agents/rules/foo.mdc")).unwrap();
        migrate_legacy_policy_to_agents(p).unwrap();
        let dest_after = std::fs::read_to_string(p.join(".agents/rules/foo.mdc")).unwrap();
        assert_eq!(dest_before, dest_after);
        assert!(p.join(".ax/policy/rules/foo.mdc").is_file());
    }

    #[test]
    fn disable_relocate_leaves_agents() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let agents_file = rule_path(&agents_dir(p), "demo");
        write_rule(&agents_file, "demo", true, "project");
        let keep = rule_path(&inactive_dir(p), "demo");
        write_rule(&keep, "demo", false, "project");
        relocate_rule_file(p, "demo", &keep).unwrap();
        assert!(!agents_file.exists());
        assert!(keep.is_file());
        let raw = std::fs::read_to_string(&keep).unwrap();
        assert!(raw.contains("enabled: false"));
    }

    #[test]
    fn enable_relocate_restores_agents() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let inactive = rule_path(&inactive_dir(p), "demo");
        write_rule(&inactive, "demo", false, "project");
        let keep = rule_path(&agents_dir(p), "demo");
        write_rule(&keep, "demo", true, "project");
        relocate_rule_file(p, "demo", &keep).unwrap();
        assert!(!inactive.exists());
        assert!(keep.is_file());
    }

    #[test]
    fn private_write_dir_never_agents() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        assert_eq!(
            resolve_shareable_write_dir(p, PolicyScope::PrivateProject, true),
            p.join(".ax/policy-private")
        );
        assert_eq!(
            resolve_shareable_write_dir(p, PolicyScope::PrivateProject, false),
            p.join(".ax/policy-private")
        );
        assert_eq!(
            resolve_shareable_write_dir(p, PolicyScope::Project, false),
            inactive_dir(p)
        );
    }

    #[test]
    fn git_export_candidate_filters() {
        assert!(is_git_export_candidate(PolicyScope::Project, true));
        assert!(is_git_export_candidate(PolicyScope::Workspace, true));
        assert!(!is_git_export_candidate(PolicyScope::Project, false));
        assert!(!is_git_export_candidate(PolicyScope::PrivateProject, true));
        assert!(!is_git_export_candidate(PolicyScope::Company, true));
        assert!(!is_git_export_candidate(PolicyScope::PrivateUser, true));
    }

    #[test]
    fn leak_gate_detects_disabled_and_private() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        assert!(agents_share_violations(p).is_empty());
        write_rule(
            &p.join(".agents/rules/bad.mdc"),
            "bad",
            false,
            "project",
        );
        assert!(!agents_share_violations(p).is_empty());
        std::fs::remove_file(p.join(".agents/rules/bad.mdc")).unwrap();
        write_rule(
            &p.join(".agents/rules/priv.mdc"),
            "priv",
            true,
            "private_project",
        );
        assert!(agents_share_violations(p)
            .iter()
            .any(|v| v.contains("non-packable")));
    }

    #[test]
    fn cursor_skill_symlink_points_at_agents() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let skill = p.join(".agents/skills/foo");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(skill.join("SKILL.md"), "---\nname: foo\n---\n\nx\n").unwrap();
        let linked = link_cursor_skills_to_agents(p).unwrap();
        assert!(linked.contains(&"foo".to_string()));
        let dest = p.join(".cursor/skills/foo");
        assert!(dest.symlink_metadata().unwrap().file_type().is_symlink());
    }
}
