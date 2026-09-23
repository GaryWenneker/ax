//! Default policy templates — embedded at compile time, written on `ax init`.
//! Team policy → `.agents/` (MCP). IDE bootstrap → per-IDE instructions via `ide_seed`.

use std::path::{Path, PathBuf};

use crate::paths::{rule_file, skill_file};

/// Relative path under `.agents/` and file body (UTF-8, no BOM).
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
    /// Also stored as a machine skill in `~/.ax/global.db` on install / init.
    global_db: bool,
}

/// Global company-scope rules seeded to `~/.ax/global_policy/rules/`.
const GLOBAL_RULE_TEMPLATES: &[Template] = &[
    Template {
        rel: "rules/old-coder-mandatory.mdc",
        body: include_str!("../templates/rules/old-coder-mandatory.mdc"),
    },
    Template {
        rel: "rules/frontend-production-build.mdc",
        body: include_str!("../templates/rules/frontend-production-build.mdc"),
    },
];

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
        global_db: false,
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
        global_db: false,
    },
    SkillBundle {
        name: "review-loop",
        files: &[SkillBundleFile {
            rel: "SKILL.md",
            body: include_str!("../templates/skills/review-loop/SKILL.md"),
        }],
        global_db: true,
    },
    SkillBundle {
        name: "pr-review-comments",
        files: &[SkillBundleFile {
            rel: "SKILL.md",
            body: include_str!("../templates/skills/pr-review-comments/SKILL.md"),
        }],
        global_db: true,
    },
];

const MANAGED: &[(&str, &str, bool)] = &[
    (
        ".agents/skills/startup/SKILL.md",
        "skills/startup/SKILL.md",
        false,
    ),
    (
        ".agents/rules/explore-before-grep.mdc",
        "rules/explore-before-grep.mdc",
        false,
    ),
    (
        ".agents/rules/mcp-callmcp-shape.mdc",
        "rules/mcp-callmcp-shape.mdc",
        false,
    ),
    (
        ".agents/rules/prefer-mcp-ops.mdc",
        "rules/prefer-mcp-ops.mdc",
        true, // ops mapping — not a preflight instruction file
    ),
    (
        ".agents/rules/subagents.mdc",
        "rules/subagents.mdc",
        true,
    ),
    (
        ".agents/skills/subagents/SKILL.md",
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

fn shareable_policy_root(ax_dir: &Path) -> PathBuf {
    let project = ax_dir.parent().unwrap_or(ax_dir);
    crate::agents_share::agents_dir(project)
}

/// Write embedded default policy files when missing. Never overwrites existing files.
pub fn seed_default_policy(ax_dir: &Path) -> std::io::Result<SeedResult> {
    let project = ax_dir.parent().unwrap_or(ax_dir);
    let _ = crate::agents_share::migrate_legacy_policy_to_agents(project);
    let policy = shareable_policy_root(ax_dir);
    std::fs::create_dir_all(policy.join("rules"))?;
    std::fs::create_dir_all(policy.join("skills"))?;
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

fn write_skill_bundle(skills_root: &Path, bundle: &SkillBundle) -> std::io::Result<bool> {
    let skill_dir = skills_root.join(bundle.name);
    let skill_md = skill_dir.join(crate::paths::SKILL_FILENAME);
    let template_skill = bundle
        .files
        .iter()
        .find(|f| f.rel == crate::paths::SKILL_FILENAME)
        .map(|f| f.body)
        .unwrap_or("");
    if skill_md.exists() {
        let existing = std::fs::read_to_string(&skill_md).unwrap_or_default();
        let missing_triggers = !existing.contains("triggers:");
        if !missing_triggers && !seeded_content_needs_upgrade(&existing, template_skill) {
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

/// `seedVersion: N` from the frontmatter; 0 when absent or unreadable.
fn seed_version(content: &str) -> u32 {
    let Some(frontmatter) = content
        .strip_prefix("---")
        .and_then(|rest| rest.split("\n---").next())
    else {
        return 0;
    };
    frontmatter
        .lines()
        .find_map(|line| line.trim().strip_prefix("seedVersion:"))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0)
}

/// Skills that `ax install` / `ax init` also store as machine skills in `~/.ax/global.db`.
pub fn global_db_seed_skills() -> Vec<(&'static str, &'static str)> {
    GLOBAL_SKILL_BUNDLES
        .iter()
        .filter(|b| b.global_db)
        .filter_map(|b| {
            b.files
                .iter()
                .find(|f| f.rel == crate::paths::SKILL_FILENAME)
                .map(|f| (b.name, f.body))
        })
        .collect()
}

/// A seeded file is rewritten when the template carries a newer `seedVersion`
/// (hand edits included), or when it lacks a guard the template requires.
fn seeded_content_needs_upgrade(existing: &str, template: &str) -> bool {
    if seed_version(template) > seed_version(existing) {
        return true;
    }
    let ex = existing.to_ascii_lowercase();
    let tmpl = template.to_ascii_lowercase();
    if tmpl.contains("alwaysapply: true") && !ex.contains("alwaysapply: true") {
        return true;
    }
    if tmpl.contains("require-skill:") && !ex.contains("require-skill:") {
        return true;
    }
    false
}

fn seed_skill_bundles(skills_root: &Path, label_prefix: &str) -> std::io::Result<SeedResult> {
    seed_bundles(skills_root, label_prefix, GLOBAL_SKILL_BUNDLES.iter())
}

fn seed_bundles<'a>(
    skills_root: &Path,
    label_prefix: &str,
    bundles: impl Iterator<Item = &'a SkillBundle>,
) -> std::io::Result<SeedResult> {
    std::fs::create_dir_all(skills_root)?;
    let mut result = SeedResult::default();
    for bundle in bundles {
        let label = format!("{label_prefix}/{}/{}", bundle.name, crate::paths::SKILL_FILENAME);
        if write_skill_bundle(skills_root, bundle)? {
            result.created.push(label);
        } else {
            result.skipped.push(label);
        }
    }
    Ok(result)
}

/// Write machine-wide bundled skills to a Cursor skills directory.
/// Project-specific skills are installed only through stack packs.
pub fn seed_cursor_skills(skills_root: &Path) -> std::io::Result<SeedResult> {
    std::fs::create_dir_all(skills_root)?;
    let mut result = SeedResult::default();
    let bundles = seed_skill_bundles(skills_root, ".cursor/skills")?;
    result.created.extend(bundles.created);
    result.skipped.extend(bundles.skipped);
    Ok(result)
}

fn seed_project_bundles(skills_root: &Path) -> std::io::Result<SeedResult> {
    seed_bundles(skills_root, ".cursor/skills", GLOBAL_SKILL_BUNDLES.iter().filter(|b| !b.global_db))
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
            let existing = std::fs::read_to_string(&dest).unwrap_or_default();
            if !seeded_content_needs_upgrade(&existing, t.body) {
                result.skipped.push(label);
                continue;
            }
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
/// Skills stored in `~/.ax/global.db` are left out: a project copy would only duplicate them.
pub fn seed_project_cursor_skills(project_root: &Path) -> std::io::Result<SeedResult> {
    let agents_skills = crate::agents_share::agents_dir(project_root).join("skills");
    let mut result = seed_project_bundles(&agents_skills)?;
    match crate::agents_share::link_cursor_skills_to_agents(project_root) {
        Ok(linked) => {
            for name in linked {
                result
                    .created
                    .push(format!(".cursor/skills/{name} -> .agents/skills/{name}"));
            }
        }
        Err(_) => {
            let copied = seed_project_bundles(&project_root.join(".cursor").join("skills"))?;
            result.created.extend(copied.created);
            result.skipped.extend(copied.skipped);
        }
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
    let policy = shareable_policy_root(ax_dir);
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
    let policy = shareable_policy_root(ax_dir);
    std::fs::create_dir_all(policy.join("rules"))?;
    std::fs::create_dir_all(policy.join("skills"))?;
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
                    "`.cursor/rules/{cursor_name}.mdc` duplicates ax policy rule `{policy_id}` — remove it; use `.agents/rules/{policy_id}.mdc` + ax_preflight MCP instead"
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
        let startup = dir.path().join(".agents/skills/startup/SKILL.md");
        std::fs::remove_file(&startup).unwrap();
        let synced = sync_instructions(&ax, true).unwrap();
        assert!(!synced.fixed.is_empty());
        assert_eq!(synced.fail_count, 0);
    }

    #[test]
    fn seed_cursor_skills_writes_once() {
        let dir = tempdir().unwrap();
        let skills = dir.path().join(".cursor").join("skills");
        let first = seed_cursor_skills(&skills).unwrap();
        assert_eq!(first.created.len(), GLOBAL_SKILL_BUNDLES.len());
        assert!(!skills.join("noti").exists());
        assert!(!skills.join("dotnet-code-review").exists());
        assert!(!skills.join("deploy").exists());
        let second = seed_cursor_skills(&skills).unwrap();
        assert!(second.created.is_empty());
        assert_eq!(second.skipped.len(), GLOBAL_SKILL_BUNDLES.len());
    }

    #[test]
    fn project_seed_leaves_global_db_skills_out() {
        let dir = tempdir().unwrap();
        seed_project_cursor_skills(dir.path()).unwrap();
        let skills = dir.path().join(".agents/skills");
        assert!(skills.join("old-coder/SKILL.md").is_file());
        for bundle in GLOBAL_SKILL_BUNDLES.iter().filter(|b| b.global_db) {
            assert!(!skills.join(bundle.name).exists(), "{} seeded into the project", bundle.name);
        }
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
    fn write_skill_bundle_upgrades_missing_always_apply() {
        let dir = tempdir().unwrap();
        let skills = dir.path();
        let dest_dir = skills.join("old-coder");
        std::fs::create_dir_all(&dest_dir).unwrap();
        std::fs::write(
            dest_dir.join("SKILL.md"),
            "---\nname: old-coder\ndescription: x\ntriggers: [\"a\"]\n---\n\nold\n",
        )
        .unwrap();
        let bundle = GLOBAL_SKILL_BUNDLES
            .iter()
            .find(|b| b.name == "old-coder")
            .unwrap();
        assert!(write_skill_bundle(skills, bundle).unwrap());
        let body = std::fs::read_to_string(dest_dir.join("SKILL.md")).unwrap();
        assert!(body.to_ascii_lowercase().contains("alwaysapply: true"));
        assert!(!write_skill_bundle(skills, bundle).unwrap());
    }

    #[test]
    fn seed_global_policy_rules_upgrades_require_skill() {
        let dir = tempdir().unwrap();
        let rules = dir.path().join("rules");
        std::fs::create_dir_all(&rules).unwrap();
        std::fs::write(
            rules.join("old-coder-mandatory.mdc"),
            "---\nid: old-coder-mandatory\nlevel: CRITICAL\nalwaysApply: true\n---\n\nNo guard yet.\n",
        )
        .unwrap();
        let result = seed_global_policy_rules(&rules).unwrap();
        assert!(
            result
                .created
                .iter()
                .any(|s| s.contains("old-coder-mandatory")),
            "{:?}",
            result
        );
        let body = std::fs::read_to_string(rules.join("old-coder-mandatory.mdc")).unwrap();
        assert!(body.to_ascii_lowercase().contains("require-skill"));
        let second = seed_global_policy_rules(&rules).unwrap();
        assert!(second.created.is_empty());
        assert!(second.skipped.iter().any(|s| s.contains("old-coder-mandatory")));
    }

    fn review_loop_bundle() -> &'static SkillBundle {
        GLOBAL_SKILL_BUNDLES
            .iter()
            .find(|b| b.name == "review-loop")
            .expect("review-loop is a global skill bundle")
    }

    #[test]
    fn global_rules_include_frontend_production_build() {
        let body = GLOBAL_RULE_TEMPLATES
            .iter()
            .find(|t| t.rel == "rules/frontend-production-build.mdc")
            .map(|t| t.body)
            .expect("frontend-production-build is a seeded global rule");
        assert!(body.contains("pnpm run build"));
        assert!(body.contains("UNRESOLVED_IMPORT"));
        assert!(seed_version(body) >= 1);
    }

    fn old_coder_rule() -> &'static str {
        GLOBAL_RULE_TEMPLATES
            .iter()
            .find(|t| t.rel == "rules/old-coder-mandatory.mdc")
            .map(|t| t.body)
            .unwrap()
    }

    #[test]
    fn old_coder_rule_requires_the_review_loop() {
        let rule = old_coder_rule();
        assert!(rule.contains("level: CRITICAL"), "stays CRITICAL");
        assert!(rule.contains("ax_skill({ name: \"review-loop\" })"), "{rule}");
        assert!(rule.contains("GAUNTLET → REVIEW LOOP → EVIDENCE"));
        assert!(seed_version(rule) >= 2, "the rule change must bump seedVersion");
    }

    #[test]
    fn review_loop_skill_pins_every_step() {
        let body = review_loop_bundle().files[0].body;
        for phrase in [
            "name: review-loop",
            "## 1. Skill check",
            "`usable`",
            "`missing`",
            "`empty`",
            "## 2. Stack review",
            "## 3. Process findings",
            "RED test",
            "## 4. Repeat",
            "The loop ends after the first round with **zero findings**",
            "There is no round cap",
            "## Production build",
            "UNRESOLVED_IMPORT",
            "Review rounds",
        ] {
            assert!(body.contains(phrase), "review-loop skill lost {phrase:?}");
        }
        assert!(seed_version(body) >= 1);
    }

    #[test]
    fn global_seed_writes_review_loop_everywhere() {
        let dir = tempdir().unwrap();
        seed_skill_bundles(&dir.path().join("global"), "g").unwrap();
        seed_cursor_skills(&dir.path().join("cursor")).unwrap();
        assert!(dir.path().join("global/review-loop/SKILL.md").is_file());
        assert!(dir.path().join("cursor/review-loop/SKILL.md").is_file());
    }

    #[test]
    fn seed_version_decides_upgrades() {
        let v2 = "---\nid: x\nseedVersion: 2\n---\nbody\n";
        assert!(seeded_content_needs_upgrade("---\nid: x\n---\nold\n", v2), "missing version upgrades");
        assert!(seeded_content_needs_upgrade("---\nid: x\nseedVersion: 1\n---\nold\n", v2));
        assert!(!seeded_content_needs_upgrade("---\nid: x\nseedVersion: 2\n---\nhand edit\n", v2));
        assert!(!seeded_content_needs_upgrade("---\nid: x\nseedVersion: 3\n---\nnewer\n", v2));
        assert!(
            !seeded_content_needs_upgrade("---\nid: x\n---\nold\n", "---\nid: x\n---\nnew\n"),
            "unversioned templates never force an overwrite"
        );
    }

    #[test]
    fn seed_version_only_counts_the_frontmatter() {
        assert_eq!(seed_version("---\nid: x\n---\nExample:\nseedVersion: 9\n"), 0);
        assert_eq!(seed_version("seedVersion: 9\n"), 0, "no frontmatter, no version");
        assert_eq!(seed_version("---\nseedVersion: 4\n---\nbody\n"), 4);
    }

    #[test]
    fn older_seeded_old_coder_rule_is_upgraded_once() {
        let dir = tempdir().unwrap();
        let rules = dir.path().join("rules");
        std::fs::create_dir_all(&rules).unwrap();
        std::fs::write(
            rules.join("old-coder-mandatory.mdc"),
            "---\nid: old-coder-mandatory\nlevel: CRITICAL\nalwaysApply: true\n---\n\nguard: require-skill: \"old-coder\"\nhand edited\n",
        )
        .unwrap();
        let first = seed_global_policy_rules(&rules).unwrap();
        assert!(first.created.iter().any(|s| s.contains("old-coder-mandatory")), "{first:?}");
        let body = std::fs::read_to_string(rules.join("old-coder-mandatory.mdc")).unwrap();
        assert!(body.contains("review-loop"));
        let second = seed_global_policy_rules(&rules).unwrap();
        assert!(second.created.is_empty(), "{second:?}");
    }

    #[test]
    fn current_review_loop_skill_is_not_overwritten() {
        let dir = tempdir().unwrap();
        let bundle = review_loop_bundle();
        assert!(write_skill_bundle(dir.path(), bundle).unwrap());
        let path = dir.path().join("review-loop/SKILL.md");
        let edited = format!("{}\nlocal note\n", std::fs::read_to_string(&path).unwrap());
        std::fs::write(&path, &edited).unwrap();
        assert!(!write_skill_bundle(dir.path(), bundle).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), edited);
    }

    #[test]
    fn global_db_skills_are_review_loop_and_pr_review_comments() {
        let skills = global_db_seed_skills();
        assert_eq!(
            skills.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            vec!["review-loop", "pr-review-comments"]
        );
        assert!(skills[0].1.contains("## 4. Repeat"));
        assert!(skills[1].1.contains("one question per comment"));
    }

    fn pr_review_comments_body() -> &'static str {
        GLOBAL_SKILL_BUNDLES
            .iter()
            .find(|b| b.name == "pr-review-comments")
            .expect("pr-review-comments is a global skill bundle")
            .files[0]
            .body
    }

    #[test]
    fn pr_review_comments_skill_pins_every_step() {
        let body = pr_review_comments_body();
        for phrase in [
            "name: pr-review-comments",
            "someone else's pull request",
            "Do not fix",
            "one question per comment",
            "`AskQuestion`",
            "`AskUserQuestion`",
            "`Post: <proposed text 1>`",
            "`Do not post`",
            "- free text: the user writes their own comment",
            "Never ask for a batch approval of several comments",
            "Never post a comment the user did not choose",
            "## 5. Summary",
        ] {
            assert!(body.contains(phrase), "pr-review-comments lost {phrase:?}");
        }
        assert!(seed_version(body) >= 1);
    }

    #[test]
    fn global_seed_writes_pr_review_comments() {
        let dir = tempdir().unwrap();
        seed_skill_bundles(&dir.path().join("global"), "g").unwrap();
        seed_cursor_skills(&dir.path().join("cursor")).unwrap();
        assert!(dir.path().join("global/pr-review-comments/SKILL.md").is_file());
        assert!(dir.path().join("cursor/pr-review-comments/SKILL.md").is_file());
    }

    #[test]
    fn review_loop_hands_colleague_prs_to_pr_review_comments() {
        let review_loop = review_loop_bundle().files[0].body;
        assert!(review_loop.contains("`pr-review-comments`"), "{review_loop}");
        assert!(review_loop.contains("colleague's pull request"));
        assert!(seed_version(review_loop) >= 3, "the review-loop change must bump seedVersion");
        let rule = old_coder_rule();
        assert!(rule.contains("ax_skill({ name: \"pr-review-comments\" })"), "{rule}");
        assert!(seed_version(rule) >= 3, "the rule change must bump seedVersion");
    }

    #[test]
    fn sync_fix_restores_drifted_startup_template() {
        let dir = tempdir().unwrap();
        let ax = dir.path().join(".ax");
        seed_default_policy(&ax).unwrap();
        let startup = dir.path().join(".agents/skills/startup/SKILL.md");
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
