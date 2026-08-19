//! Default policy templates — embedded at compile time, written on `ax init`.
//! Team policy → `.ax/policy/` (MCP). IDE bootstrap → per-IDE instructions via `ide_seed`.

use std::path::{Path, PathBuf};

use crate::paths::{rule_file, rules_dir, skill_file, skills_dir};

/// Relative path under `.ax/policy/` and file body (UTF-8, no BOM).
struct Template {
    rel: &'static str,
    body: &'static str,
}

const TEMPLATES: &[Template] = &[
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
        rel: "rules/install-version-resolution.mdc",
        body: include_str!("../templates/rules/install-version-resolution.mdc"),
    },
    Template {
        rel: "rules/codegraph-parity.mdc",
        body: include_str!("../templates/rules/codegraph-parity.mdc"),
    },
    Template {
        rel: "rules/policy-capture.mdc",
        body: include_str!("../templates/rules/policy-capture.mdc"),
    },
    Template {
        rel: "rules/web-ui-rebuild.mdc",
        body: include_str!("../templates/rules/web-ui-rebuild.mdc"),
    },
    Template {
        rel: "rules/docs-with-features.mdc",
        body: include_str!("../templates/rules/docs-with-features.mdc"),
    },
    Template {
        rel: "rules/modal-forms.mdc",
        body: include_str!("../templates/rules/modal-forms.mdc"),
    },
    Template {
        rel: "rules/explore-before-grep.mdc",
        body: include_str!("../templates/rules/explore-before-grep.mdc"),
    },
    Template {
        rel: "rules/mcp-callmcp-shape.mdc",
        body: include_str!("../templates/rules/mcp-callmcp-shape.mdc"),
    },
    Template {
        rel: "rules/prefer-mcp-ops.mdc",
        body: include_str!("../templates/rules/prefer-mcp-ops.mdc"),
    },
    Template {
        rel: "skills/startup/SKILL.md",
        body: include_str!("../templates/skills/startup/SKILL.md"),
    },
    Template {
        rel: "skills/subagents/SKILL.md",
        body: include_str!("../templates/skills/subagents/SKILL.md"),
    },
    Template {
        rel: "skills/systematic-debugging/SKILL.md",
        body: include_str!("../templates/skills/systematic-debugging/SKILL.md"),
    },
    Template {
        rel: "skills/tdd/SKILL.md",
        body: include_str!("../templates/skills/tdd/SKILL.md"),
    },
    Template {
        rel: "skills/design-first/SKILL.md",
        body: include_str!("../templates/skills/design-first/SKILL.md"),
    },
    Template {
        rel: "skills/auti/SKILL.md",
        body: include_str!("../templates/skills/auti/SKILL.md"),
    },
    Template {
        rel: "skills/deploy/SKILL.md",
        body: include_str!("../templates/skills/deploy/SKILL.md"),
    },
    Template {
        rel: "skills/feature-information/SKILL.md",
        body: include_str!("../templates/skills/feature-information/SKILL.md"),
    },
    Template {
        rel: "skills/no-ab-prefix/SKILL.md",
        body: include_str!("../templates/skills/no-ab-prefix/SKILL.md"),
    },
    Template {
        rel: "skills/noti/SKILL.md",
        body: include_str!("../templates/skills/noti/SKILL.md"),
    },
    Template {
        rel: "skills/pr/SKILL.md",
        body: include_str!("../templates/skills/pr/SKILL.md"),
    },
    Template {
        rel: "skills/pre-pr-check/SKILL.md",
        body: include_str!("../templates/skills/pre-pr-check/SKILL.md"),
    },
    Template {
        rel: "skills/preq/SKILL.md",
        body: include_str!("../templates/skills/preq/SKILL.md"),
    },
    Template {
        rel: "skills/ship/SKILL.md",
        body: include_str!("../templates/skills/ship/SKILL.md"),
    },
];

/// Baseline rollout skills — also copied to `.cursor/skills/` on init/install.
const ROLLOUT_SKILL_RELS: &[&str] = &[
    "skills/auti/SKILL.md",
    "skills/deploy/SKILL.md",
    "skills/feature-information/SKILL.md",
    "skills/no-ab-prefix/SKILL.md",
    "skills/noti/SKILL.md",
    "skills/pr/SKILL.md",
    "skills/pre-pr-check/SKILL.md",
    "skills/preq/SKILL.md",
    "skills/ship/SKILL.md",
];

/// Relative path + embedded body within a skill directory (e.g. `SKILL.md`, `references/gauntlet.md`).
struct SkillBundleFile {
    rel: &'static str,
    body: &'static str,
}

/// Multi-file skill bundle (SKILL.md + references/). Vendored from upstream repos.
struct SkillBundle {
    name: &'static str,
    files: &'static [SkillBundleFile],
}

/// Global company-scope rules seeded to `~/.ax/global_policy/rules/`.
const GLOBAL_RULE_TEMPLATES: &[Template] = &[Template {
    rel: "rules/old-coder-mandatory.mdc",
    body: include_str!("../templates/rules/old-coder-mandatory.mdc"),
}];

/// Machine-wide skills seeded to `~/.ax/global_policy/skills/` and `~/.cursor/skills/`.
/// Source: https://github.com/AmazingAng/old-coder (MIT)
const GLOBAL_SKILL_BUNDLES: &[SkillBundle] = &[
    SkillBundle {
        name: "old-coder",
        files: &[
            SkillBundleFile {
                rel: "SKILL.md",
                body: include_str!("../templates/skills/old-coder/SKILL.md"),
            },
            SkillBundleFile {
                rel: "references/gauntlet.md",
                body: include_str!("../templates/skills/old-coder/references/gauntlet.md"),
            },
            SkillBundleFile {
                rel: "references/templates.md",
                body: include_str!("../templates/skills/old-coder/references/templates.md"),
            },
            SkillBundleFile {
                rel: "references/verifier-case-study.md",
                body: include_str!("../templates/skills/old-coder/references/verifier-case-study.md"),
            },
            SkillBundleFile {
                rel: "references/verifier.md",
                body: include_str!("../templates/skills/old-coder/references/verifier.md"),
            },
        ],
    },
    SkillBundle {
        name: "old-coder-api",
        files: &[
            SkillBundleFile {
                rel: "SKILL.md",
                body: include_str!("../templates/skills/old-coder-api/SKILL.md"),
            },
            SkillBundleFile {
                rel: "references/breaking-changes.md",
                body: include_str!("../templates/skills/old-coder-api/references/breaking-changes.md"),
            },
            SkillBundleFile {
                rel: "references/examples.md",
                body: include_str!("../templates/skills/old-coder-api/references/examples.md"),
            },
            SkillBundleFile {
                rel: "references/patterns.md",
                body: include_str!("../templates/skills/old-coder-api/references/patterns.md"),
            },
        ],
    },
];

const MANAGED: &[(&str, &str, bool)] = &[
    (
        ".ax/policy/skills/startup/SKILL.md",
        "skills/startup/SKILL.md",
        false,
    ),
    (
        ".ax/policy/rules/explore-before-grep.mdc",
        "rules/explore-before-grep.mdc",
        false,
    ),
    (
        ".ax/policy/rules/mcp-callmcp-shape.mdc",
        "rules/mcp-callmcp-shape.mdc",
        false,
    ),
    (
        ".ax/policy/rules/prefer-mcp-ops.mdc",
        "rules/prefer-mcp-ops.mdc",
        true, // ops mapping — not a preflight instruction file
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

fn cursor_skill_rel(name: &str) -> String {
    format!(".cursor/skills/{name}/{}", crate::paths::SKILL_FILENAME)
}

fn write_skill_bundle(skills_root: &Path, bundle: &SkillBundle) -> std::io::Result<bool> {
    let skill_dir = skills_root.join(bundle.name);
    let skill_md = skill_dir.join(crate::paths::SKILL_FILENAME);
    if skill_md.exists() {
        let existing = std::fs::read_to_string(&skill_md).unwrap_or_default();
        // Bundles ship with triggers — upgrade seeded copies from before triggers were added.
        if existing.contains("triggers:") {
            return Ok(false);
        }
    }
    for file in bundle.files {
        let dest = skill_dir.join(file.rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, file.body.as_bytes())?;
    }
    Ok(true)
}

fn seed_skill_bundles(skills_root: &Path, label_prefix: &str) -> std::io::Result<SeedResult> {
    std::fs::create_dir_all(skills_root)?;
    let mut result = SeedResult::default();
    for bundle in GLOBAL_SKILL_BUNDLES {
        let label = format!("{label_prefix}/{}/{}", bundle.name, crate::paths::SKILL_FILENAME);
        if write_skill_bundle(skills_root, bundle)? {
            result.created.push(label);
        } else {
            result.skipped.push(label);
        }
    }
    Ok(result)
}

/// Write baseline rollout skills to a Cursor skills directory (never overwrites).
pub fn seed_cursor_skills(skills_root: &Path) -> std::io::Result<SeedResult> {
    std::fs::create_dir_all(skills_root)?;
    let mut result = SeedResult::default();
    for rel in ROLLOUT_SKILL_RELS {
        let t = template_by_rel(rel).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, format!("unknown rollout skill: {rel}"))
        })?;
        let name = rel
            .strip_prefix("skills/")
            .and_then(|s| s.strip_suffix("/SKILL.md"))
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("bad skill rel: {rel}"))
            })?;
        let dest = skills_root.join(name).join(crate::paths::SKILL_FILENAME);
        let label = cursor_skill_rel(name);
        if dest.exists() {
            result.skipped.push(label);
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, t.body.as_bytes())?;
        result.created.push(label);
    }
    let bundles = seed_skill_bundles(skills_root, ".cursor/skills")?;
    result.created.extend(bundles.created);
    result.skipped.extend(bundles.skipped);
    Ok(result)
}

fn seed_global_policy_rules(rules_root: &Path) -> std::io::Result<SeedResult> {
    std::fs::create_dir_all(rules_root)?;
    let mut result = SeedResult::default();
    for t in GLOBAL_RULE_TEMPLATES {
        let id = t
            .rel
            .strip_prefix("rules/")
            .and_then(|s| s.strip_suffix(".mdc"))
            .unwrap_or("rule");
        let dest = rules_root.join(format!("{id}.mdc"));
        let label = format!("~/.ax/global_policy/rules/{id}.mdc");
        if dest.exists() {
            result.skipped.push(label);
            continue;
        }
        std::fs::write(&dest, t.body.as_bytes())?;
        result.created.push(label);
    }
    Ok(result)
}

fn global_policy_root() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".ax").join("global_policy"))
}

/// Seed `~/.ax/global_policy/` with machine-wide rules and skills (company scope via MCP).
pub fn seed_global_policy() -> std::io::Result<SeedResult> {
    let Some(global) = global_policy_root() else {
        return Ok(SeedResult::default());
    };
    std::fs::create_dir_all(&global)?;
    let mut result = seed_global_policy_rules(&global.join("rules"))?;
    let skills = seed_skill_bundles(&global.join("skills"), "~/.ax/global_policy/skills")?;
    result.created.extend(skills.created);
    result.skipped.extend(skills.skipped);
    Ok(result)
}

/// Seed `~/.ax/global_policy/skills/` with machine-wide skills (company scope via MCP).
pub fn seed_global_policy_skills() -> std::io::Result<SeedResult> {
    seed_global_policy()
}

/// Seed `~/.cursor/skills/` with baseline rollout skills (machine-wide Cursor agents).
pub fn seed_global_cursor_skills() -> std::io::Result<SeedResult> {
    let Some(home) = dirs::home_dir() else {
        return Ok(SeedResult::default());
    };
    seed_cursor_skills(&home.join(".cursor").join("skills"))
}

/// Seed `<project>/.cursor/skills/` with baseline rollout skills.
pub fn seed_project_cursor_skills(project_root: &Path) -> std::io::Result<SeedResult> {
    seed_cursor_skills(&project_root.join(".cursor").join("skills"))
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

/// Issues for a managed instruction file, including drift from the embedded init template
/// when the file is required (not optional).
fn managed_file_issues(rel: &str, content: &str, optional: bool) -> Vec<String> {
    let mut issues = verify_content(content);
    if !optional {
        if let Some(t) = template_by_rel(rel) {
            if content.trim() != t.body.trim() {
                issues.push("drifted from embedded init template".into());
            }
        }
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
            let issues = managed_file_issues(rel, &content, *optional);
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
            managed_file_issues(rel, &content, *optional)
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
/// `ax` is the IDE bootstrap rule — see `ide_seed`.
const KNOWN_POLICY_IDS: &[&str] = &[
    "subagents",
    "english-only",
    "utf8-no-bom",
    "release-all-platforms",
    "install-version-resolution",
    "codegraph-parity",
    "policy-capture",
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
        if stem == "ax" || stem == "ax-agent-workflow" {
            continue;
        }
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
    fn rollout_skill_templates_parse() {
        use crate::parse::parse_skill_file;

        for rel in ROLLOUT_SKILL_RELS {
            let t = template_by_rel(rel).expect(rel);
            let tmp = tempdir().unwrap();
            let path = tmp.path().join("SKILL.md");
            std::fs::write(&path, t.body).unwrap();
            parse_skill_file(&path, t.body).expect(rel);
        }
    }

    #[test]
    fn seed_cursor_skills_writes_once() {
        let dir = tempdir().unwrap();
        let skills = dir.path().join(".cursor").join("skills");
        let first = seed_cursor_skills(&skills).unwrap();
        assert_eq!(
            first.created.len(),
            ROLLOUT_SKILL_RELS.len() + GLOBAL_SKILL_BUNDLES.len()
        );
        let second = seed_cursor_skills(&skills).unwrap();
        assert!(second.created.is_empty());
        assert_eq!(
            second.skipped.len(),
            ROLLOUT_SKILL_RELS.len() + GLOBAL_SKILL_BUNDLES.len()
        );
    }

    #[test]
    fn global_skill_bundles_include_references() {
        let dir = tempdir().unwrap();
        let skills = dir.path().join("skills");
        let result = seed_skill_bundles(&skills, "test/skills").unwrap();
        assert_eq!(result.created.len(), GLOBAL_SKILL_BUNDLES.len());
        assert!(skills.join("old-coder/references/gauntlet.md").is_file());
        assert!(skills.join("old-coder-api/references/patterns.md").is_file());
    }

    #[test]
    fn global_skill_bundle_templates_parse() {
        use crate::parse::parse_skill_file;

        for bundle in GLOBAL_SKILL_BUNDLES {
            let skill_md = bundle
                .files
                .iter()
                .find(|f| f.rel == "SKILL.md")
                .expect("bundle has SKILL.md");
            let tmp = tempdir().unwrap();
            let path = tmp.path().join("SKILL.md");
            std::fs::write(&path, skill_md.body).unwrap();
            parse_skill_file(&path, skill_md.body).expect(bundle.name);
        }
    }

    #[test]
    fn sync_fix_restores_drifted_startup_template() {
        let dir = tempdir().unwrap();
        let ax = dir.path().join(".ax");
        seed_default_policy(&ax).unwrap();
        let startup = skill_file(&skills_dir(&ax), "startup");
        let mut body = std::fs::read_to_string(&startup).unwrap();
        body.push_str("\n<!-- drifted -->\n");
        std::fs::write(&startup, body).unwrap();
        let synced = sync_instructions(&ax, true).unwrap();
        assert!(
            synced.fixed.iter().any(|r| r.contains("startup")),
            "expected drifted startup skill to be restored from init template"
        );
        let restored = std::fs::read_to_string(&startup).unwrap();
        assert!(restored.contains("paths"));
        assert!(!restored.contains("<!-- drifted -->"));
    }

    #[test]
    fn startup_template_documents_guard_aliases() {
        let body = include_str!("../templates/skills/startup/SKILL.md");
        assert!(body.contains("ax_guard"));
        assert!(body.contains("\"path\""));
        assert!(body.contains("\"operation\""));
        assert!(body.contains("paths"));
        assert!(body.contains("action"));
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
