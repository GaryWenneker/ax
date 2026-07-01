//! Default policy templates — embedded at compile time, written on `ax init`.
//! Migrated from Recall OS `recall-instruction-sync` / `recall-push-skills`:
//! IDE-specific `.cursor/rules` + `.cursor/skills` → IDE-agnostic `.ax/policy/`.

use std::path::{Path, PathBuf};

use crate::paths::{rule_file, rules_dir, skill_file, skills_dir};

/// Relative path under `.ax/policy/` and file body (UTF-8, no BOM).
struct Template {
    rel: &'static str,
    body: &'static str,
}

const TEMPLATES: &[Template] = &[
    Template {
        rel: "rules/agent-workflow.mdc",
        body: include_str!("../templates/rules/agent-workflow.mdc"),
    },
    Template {
        rel: "rules/subagents.mdc",
        body: include_str!("../templates/rules/subagents.mdc"),
    },
    Template {
        rel: "rules/english-only.mdc",
        body: include_str!("../templates/rules/english-only.mdc"),
    },
    Template {
        rel: "rules/utf8-no-bom.mdc",
        body: include_str!("../templates/rules/utf8-no-bom.mdc"),
    },
    Template {
        rel: "rules/release-all-platforms.mdc",
        body: include_str!("../templates/rules/release-all-platforms.mdc"),
    },
    Template {
        rel: "rules/codegraph-parity.mdc",
        body: include_str!("../templates/rules/codegraph-parity.mdc"),
    },
    Template {
        rel: "skills/startup/SKILL.md",
        body: include_str!("../templates/skills/startup/SKILL.md"),
    },
    Template {
        rel: "skills/subagents/SKILL.md",
        body: include_str!("../templates/skills/subagents/SKILL.md"),
    },
];

const MANAGED: &[(&str, &str, bool)] = &[
    (
        ".ax/policy/rules/agent-workflow.mdc",
        "rules/agent-workflow.mdc",
        false,
    ),
    (
        ".ax/policy/skills/startup/SKILL.md",
        "skills/startup/SKILL.md",
        false,
    ),
    (
        ".ax/policy/rules/subagents.mdc",
        "rules/subagents.mdc",
        true,
    ),
    (
        ".ax/policy/skills/subagents/SKILL.md",
        "skills/subagents/SKILL.md",
        true,
    ),
];

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SeedResult {
    pub created: Vec<String>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionCheck {
    pub label: String,
    pub path: PathBuf,
    pub ok: bool,
    pub issues: Vec<String>,
    pub optional: bool,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncResult {
    pub checks: Vec<InstructionCheck>,
    pub fixed: Vec<String>,
    pub fail_count: usize,
}

fn policy_path(policy_root: &Path, rel: &str) -> PathBuf {
    if let Some(id) = rel.strip_prefix("rules/").and_then(|s| s.strip_suffix(".mdc")) {
        rule_file(&policy_root.join("rules"), id)
    } else if let Some(rest) = rel.strip_prefix("skills/") {
        let name = rest.strip_suffix("/SKILL.md").unwrap_or(rest);
        skill_file(&policy_root.join("skills"), name)
    } else {
        policy_root.join(rel)
    }
}

fn template_by_rel(rel: &str) -> Option<&'static Template> {
    TEMPLATES.iter().find(|t| t.rel == rel)
}

fn write_template(policy_root: &Path, rel: &str) -> std::io::Result<PathBuf> {
    let t = template_by_rel(rel).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, format!("unknown template: {rel}"))
    })?;
    let dest = policy_path(policy_root, rel);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&dest, t.body.as_bytes())?;
    Ok(dest)
}

/// Write embedded default policy files when missing. Never overwrites existing files.
pub fn seed_default_policy(ax_dir: &Path) -> std::io::Result<SeedResult> {
    let policy = ax_dir.join("policy");
    std::fs::create_dir_all(rules_dir(ax_dir))?;
    std::fs::create_dir_all(skills_dir(ax_dir))?;
    let mut result = SeedResult::default();
    for t in TEMPLATES {
        let dest = policy_path(&policy, t.rel);
        if dest.exists() {
            result.skipped.push(t.rel.to_string());
            continue;
        }
        write_template(&policy, t.rel)?;
        result.created.push(t.rel.to_string());
    }
    Ok(result)
}

pub fn verify_content(content: &str) -> Vec<String> {
    let mut issues = Vec::new();
    if !content.contains("ax_preflight") {
        issues.push("missing ax_preflight".into());
    }
    if !content.contains("once per turn") && !content.contains("exactly once per turn") {
        issues.push("missing once-per-turn dedup".into());
    }
    let lower = content.to_lowercase();
    if lower.contains("recall_context_preflight") || lower.contains("recall_context") {
        issues.push("stale Recall MCP references — run ax policy sync --fix".into());
    }
    if lower.contains("preflight")
        && lower.contains("recall_context_status")
        && lower.contains("recall_context")
    {
        issues.push("forbidden three-step Recall startup".into());
    }
    issues
}

/// Verify default instruction files match ax preflight workflow (Recall instruction-sync parity).
pub fn verify_instructions(ax_dir: &Path) -> Vec<InstructionCheck> {
    let policy = ax_dir.join("policy");
    MANAGED
        .iter()
        .map(|(label, rel, optional)| {
            let path = policy_path(&policy, rel);
            if *optional && !path.exists() {
                return InstructionCheck {
                    label: (*label).to_string(),
                    path,
                    ok: true,
                    issues: vec![],
                    optional: *optional,
                };
            }
            if !path.exists() {
                return InstructionCheck {
                    label: (*label).to_string(),
                    path,
                    ok: false,
                    issues: vec!["missing".into()],
                    optional: *optional,
                };
            }
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let issues = verify_content(&content);
            InstructionCheck {
                label: (*label).to_string(),
                path,
                ok: issues.is_empty(),
                issues,
                optional: *optional,
            }
        })
        .collect()
}

/// Verify instruction files; with `fix`, restore missing or drifted managed files from embedded templates.
pub fn sync_instructions(ax_dir: &Path, fix: bool) -> std::io::Result<SyncResult> {
    std::fs::create_dir_all(rules_dir(ax_dir))?;
    std::fs::create_dir_all(skills_dir(ax_dir))?;
    let policy = ax_dir.join("policy");
    let mut result = SyncResult::default();
    for (label, rel, optional) in MANAGED {
        let path = policy_path(&policy, rel);
        if *optional && !path.exists() {
            continue;
        }
        let content = if path.exists() {
            std::fs::read_to_string(&path).unwrap_or_default()
        } else {
            String::new()
        };
        let issues = if path.exists() {
            verify_content(&content)
        } else {
            vec!["missing".into()]
        };
        if issues.is_empty() {
            result.checks.push(InstructionCheck {
                label: (*label).to_string(),
                path,
                ok: true,
                issues: vec![],
                optional: *optional,
            });
            continue;
        }
        if fix {
            write_template(&policy, rel)?;
            result.fixed.push((*rel).to_string());
            result.checks.push(InstructionCheck {
                label: (*label).to_string(),
                path: policy_path(&policy, rel),
                ok: true,
                issues: vec![],
                optional: *optional,
            });
        } else {
            result.checks.push(InstructionCheck {
                label: (*label).to_string(),
                path,
                ok: false,
                issues,
                optional: *optional,
            });
            if !*optional {
                result.fail_count += 1;
            }
        }
    }
    Ok(result)
}

/// Known ax policy rule ids that must not be duplicated in `.cursor/rules/`.
const KNOWN_POLICY_IDS: &[&str] = &[
    "agent-workflow",
    "subagents",
    "english-only",
    "utf8-no-bom",
    "release-all-platforms",
    "codegraph-parity",
];

/// Cursor rule filenames that alias ax policy ids.
const CURSOR_RULE_ALIASES: &[(&str, &str)] = &[
    ("no-mojibake", "utf8-no-bom"),
    ("ax-codegraph-parity", "codegraph-parity"),
];

/// Warn when `.cursor/rules/*.mdc` duplicates ax policy — delivery must be MCP-only.
pub fn check_cursor_rule_duplicates(project_root: &Path) -> Vec<String> {
    let cursor_rules = project_root.join(".cursor").join("rules");
    let Ok(entries) = std::fs::read_dir(&cursor_rules) else {
        return vec![];
    };
    let mut warnings = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("mdc") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let fname = path.file_name().and_then(|s| s.to_str()).unwrap_or(stem);
        let content = std::fs::read_to_string(&path).unwrap_or_default();

        for (cursor_name, policy_id) in CURSOR_RULE_ALIASES {
            if stem == *cursor_name {
                warnings.push(format!(
                    "`.cursor/rules/{cursor_name}.mdc` duplicates ax policy rule `{policy_id}` — remove it; use `.ax/policy/rules/{policy_id}.mdc` + ax_preflight MCP instead"
                ));
            }
        }

        if KNOWN_POLICY_IDS.contains(&stem) {
            warnings.push(format!(
                "`.cursor/rules/{stem}.mdc` mirrors ax policy rule `{stem}` — remove it; delivery is via ax_preflight MCP only"
            ));
        }

        for id in KNOWN_POLICY_IDS {
            if content.contains(&format!("id: {id}")) {
                let msg = format!(
                    "`.cursor/rules/{fname}` contains ax policy id `{id}` — remove it; use MCP inject instead"
                );
                if !warnings.iter().any(|w| w == &msg) {
                    warnings.push(msg);
                }
            }
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn seed_writes_defaults_once() {
        let dir = tempdir().unwrap();
        let ax = dir.path().join(".ax");
        let first = seed_default_policy(&ax).unwrap();
        assert_eq!(first.created.len(), TEMPLATES.len());
        assert!(first.skipped.is_empty());
        let second = seed_default_policy(&ax).unwrap();
        assert!(second.created.is_empty());
        assert_eq!(second.skipped.len(), TEMPLATES.len());
    }

    #[test]
    fn seeded_files_pass_verify() {
        let dir = tempdir().unwrap();
        let ax = dir.path().join(".ax");
        seed_default_policy(&ax).unwrap();
        let checks = verify_instructions(&ax);
        for c in checks.iter().filter(|c| !c.optional) {
            assert!(c.ok, "{:?}: {:?}", c.label, c.issues);
        }
    }

    #[test]
    fn sync_fix_restores_missing_startup() {
        let dir = tempdir().unwrap();
        let ax = dir.path().join(".ax");
        seed_default_policy(&ax).unwrap();
        let startup = skill_file(&skills_dir(&ax), "startup");
        std::fs::remove_file(&startup).unwrap();
        let synced = sync_instructions(&ax, true).unwrap();
        assert!(!synced.fixed.is_empty());
        assert_eq!(synced.fail_count, 0);
    }

    #[test]
    fn detect_stale_recall_references() {
        let issues = verify_content("call recall_context_preflight every turn");
        assert!(issues.iter().any(|i| i.contains("Recall")));
    }

    #[test]
    fn detect_cursor_rule_duplicate_by_alias() {
        let dir = tempdir().unwrap();
        let cursor_rules = dir.path().join(".cursor").join("rules");
        std::fs::create_dir_all(&cursor_rules).unwrap();
        std::fs::write(
            cursor_rules.join("no-mojibake.mdc"),
            b"---\nid: utf8\n---\nbody",
        )
        .unwrap();
        let warnings = check_cursor_rule_duplicates(dir.path());
        assert!(warnings.iter().any(|w| w.contains("no-mojibake")));
        assert!(warnings.iter().any(|w| w.contains("utf8-no-bom")));
    }
}
