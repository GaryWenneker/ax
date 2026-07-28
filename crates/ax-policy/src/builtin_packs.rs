//! Installable built-in policy packs embedded at compile time.

use std::path::{Path, PathBuf};

use ax_utils::errors::AxError;

use crate::hierarchy::ensure_scope_dirs;
use crate::paths::{rule_file, skill_file};
use crate::types::PolicyScope;

struct PackFile {
    /// Relative path under the pack root (e.g. `rules/foo.mdc`).
    rel: &'static str,
    body: &'static str,
}

struct BuiltinPack {
    name: &'static str,
    description: &'static str,
    files: &'static [PackFile],
}

const AZDO_FULLSTACK: BuiltinPack = BuiltinPack {
    name: "azdo-fullstack",
    description: "Full-stack Azure DevOps ticket-to-release skills and rules",
    files: &[
        PackFile {
            rel: "README.md",
            body: include_str!("../templates/packs/azdo-fullstack/README.md"),
        },
        PackFile {
            rel: "rules/azdo-no-spec-no-start.mdc",
            body: include_str!("../templates/packs/azdo-fullstack/rules/azdo-no-spec-no-start.mdc"),
        },
        PackFile {
            rel: "rules/azdo-traceability.mdc",
            body: include_str!("../templates/packs/azdo-fullstack/rules/azdo-traceability.mdc"),
        },
        PackFile {
            rel: "rules/azdo-shift-left-security.mdc",
            body: include_str!("../templates/packs/azdo-fullstack/rules/azdo-shift-left-security.mdc"),
        },
        PackFile {
            rel: "rules/azdo-dod-code.mdc",
            body: include_str!("../templates/packs/azdo-fullstack/rules/azdo-dod-code.mdc"),
        },
        PackFile {
            rel: "rules/azdo-tests-required.mdc",
            body: include_str!("../templates/packs/azdo-fullstack/rules/azdo-tests-required.mdc"),
        },
        PackFile {
            rel: "rules/azdo-pr-small-scope.mdc",
            body: include_str!("../templates/packs/azdo-fullstack/rules/azdo-pr-small-scope.mdc"),
        },
        PackFile {
            rel: "rules/azdo-pr-policies.mdc",
            body: include_str!("../templates/packs/azdo-fullstack/rules/azdo-pr-policies.mdc"),
        },
        PackFile {
            rel: "rules/azdo-build-once-deploy-many.mdc",
            body: include_str!("../templates/packs/azdo-fullstack/rules/azdo-build-once-deploy-many.mdc"),
        },
        PackFile {
            rel: "rules/azdo-prod-approval-gate.mdc",
            body: include_str!("../templates/packs/azdo-fullstack/rules/azdo-prod-approval-gate.mdc"),
        },
        PackFile {
            rel: "rules/azdo-release-verification.mdc",
            body: include_str!("../templates/packs/azdo-fullstack/rules/azdo-release-verification.mdc"),
        },
        PackFile {
            rel: "skills/azdo-refinement/SKILL.md",
            body: include_str!("../templates/packs/azdo-fullstack/skills/azdo-refinement/SKILL.md"),
        },
        PackFile {
            rel: "skills/azdo-development/SKILL.md",
            body: include_str!("../templates/packs/azdo-fullstack/skills/azdo-development/SKILL.md"),
        },
        PackFile {
            rel: "skills/azdo-testing/SKILL.md",
            body: include_str!("../templates/packs/azdo-fullstack/skills/azdo-testing/SKILL.md"),
        },
        PackFile {
            rel: "skills/azdo-code-review/SKILL.md",
            body: include_str!("../templates/packs/azdo-fullstack/skills/azdo-code-review/SKILL.md"),
        },
        PackFile {
            rel: "skills/azdo-pipelines/SKILL.md",
            body: include_str!("../templates/packs/azdo-fullstack/skills/azdo-pipelines/SKILL.md"),
        },
        PackFile {
            rel: "skills/azdo-release/SKILL.md",
            body: include_str!("../templates/packs/azdo-fullstack/skills/azdo-release/SKILL.md"),
        },
    ],
};

const PACKS: &[&BuiltinPack] = &[&AZDO_FULLSTACK];

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltinPackInstallResult {
    pub pack: String,
    pub created: Vec<String>,
    pub skipped: Vec<String>,
    pub overwritten: Vec<String>,
    pub policy_dir: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltinPackInfo {
    pub name: String,
    pub description: String,
    pub files: usize,
}

pub fn list_builtin_packs() -> Vec<BuiltinPackInfo> {
    PACKS
        .iter()
        .map(|p| BuiltinPackInfo {
            name: p.name.into(),
            description: p.description.into(),
            files: p.files.len(),
        })
        .collect()
}

fn find_pack(name: &str) -> Option<&'static BuiltinPack> {
    let key = name.trim().to_ascii_lowercase();
    PACKS.iter().copied().find(|p| p.name == key)
}

fn dest_for_rel(policy_dir: &Path, rel: &str) -> PathBuf {
    if let Some(id) = rel.strip_prefix("rules/").and_then(|s| s.strip_suffix(".mdc")) {
        return rule_file(&policy_dir.join("rules"), id);
    }
    if let Some(rest) = rel.strip_prefix("skills/") {
        if let Some(name) = rest.strip_suffix("/SKILL.md") {
            return skill_file(&policy_dir.join("skills"), name);
        }
    }
    policy_dir.join(rel)
}

/// Install a built-in pack into project-scope policy (files), then caller should index.
pub fn install_builtin_pack(
    project_root: &Path,
    name: &str,
    force: bool,
) -> Result<BuiltinPackInstallResult, AxError> {
    let pack = find_pack(name).ok_or_else(|| {
        let known = PACKS
            .iter()
            .map(|p| p.name)
            .collect::<Vec<_>>()
            .join(", ");
        AxError::Other(format!("unknown pack '{name}'. Available: {known}"))
    })?;

    let policy_dir = ensure_scope_dirs(project_root, PolicyScope::Project)
        .map_err(|e| AxError::Other(e.to_string()))?;

    let mut result = BuiltinPackInstallResult {
        pack: pack.name.into(),
        policy_dir: policy_dir.display().to_string(),
        ..Default::default()
    };

    for file in pack.files {
        let dest = dest_for_rel(&policy_dir, file.rel);
        if dest.exists() && !force {
            result.skipped.push(file.rel.into());
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| AxError::Other(e.to_string()))?;
        }
        let existed = dest.exists();
        // UTF-8 without BOM
        std::fs::write(&dest, file.body.as_bytes()).map_err(|e| AxError::Other(e.to_string()))?;
        if existed {
            result.overwritten.push(file.rel.into());
        } else {
            result.created.push(file.rel.into());
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_azdo_pack() {
        let packs = list_builtin_packs();
        assert!(packs.iter().any(|p| p.name == "azdo-fullstack"));
    }

    #[test]
    fn installs_without_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let r1 = install_builtin_pack(dir.path(), "azdo-fullstack", false).unwrap();
        assert!(!r1.created.is_empty());
        assert!(r1.skipped.is_empty());
        let r2 = install_builtin_pack(dir.path(), "azdo-fullstack", false).unwrap();
        assert!(r2.created.is_empty());
        assert!(!r2.skipped.is_empty());
        assert!(dir
            .path()
            .join(".ax/policy/rules/azdo-traceability.mdc")
            .is_file());
    }
}
