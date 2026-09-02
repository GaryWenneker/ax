//! Recursive policy discovery for database migration — rules and skills across the repo.

use std::collections::HashSet;
use std::path::Path;

use sqlx::SqlitePool;

use crate::capture::CaptureInterviewQuestion;
use crate::index::{upsert_rule_doc, upsert_skill_doc};
use crate::parse::{parse_rule_file, parse_skill_file, serialize_rule, serialize_skill};
use crate::types::{PolicyRuleDoc, PolicySkillDoc};

const SKIP_DIR_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "target-dev",
    "dist",
    "build",
    ".next",
    ".nuxt",
    "vendor",
    "__pycache__",
    ".svn",
    ".hg",
    "coverage",
    ".turbo",
    ".cache",
    "playwright-report",
    ".gradle",
    ".idea",
];

const BOOTSTRAP_RULE_FILES: &[&str] = &["ax.mdc", "ax-agent-workflow.mdc"];

/// One discovered rule or skill candidate for database migration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrateCandidate {
    pub kind: String,
    pub source: String,
    pub source_path: String,
    pub key: String,
    pub preview: String,
    pub questions: Vec<CaptureInterviewQuestion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<PolicyRuleDoc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill: Option<PolicySkillDoc>,
}

/// File skipped during scan (bootstrap, invalid frontmatter, duplicate).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrateSkipped {
    pub source_path: String,
    pub reason: String,
}

/// Full migration plan — propose mode returns this for per-item interview.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigratePlan {
    pub rules_found: u32,
    pub skills_found: u32,
    pub candidates: Vec<MigrateCandidate>,
    pub skipped: Vec<MigrateSkipped>,
    pub interview_instruction: String,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrateApplyResult {
    pub rules_imported: u32,
    pub skills_imported: u32,
    pub skipped: u32,
}

/// Recursively scan the project for policy rules (`.mdc`) and skills (`SKILL.md`).
pub fn scan_policy_candidates(project_root: &Path) -> MigratePlan {
    let mut candidates = Vec::new();
    let mut skipped = Vec::new();
    let mut seen_keys: HashSet<String> = HashSet::new();

    for entry in walkdir::WalkDir::new(project_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !should_skip_entry(e.path(), project_root))
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let rel = match path.strip_prefix(project_root) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };

        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        if file_name == "SKILL.md" {
            match try_parse_skill(path, &rel) {
                Ok(Some(doc)) => {
                    let key = format!("skill:{}", doc.frontmatter.name);
                    if !seen_keys.insert(key.clone()) {
                        skipped.push(MigrateSkipped {
                            source_path: rel,
                            reason: format!("duplicate skill name `{}`", doc.frontmatter.name),
                        });
                        continue;
                    }
                    let source = classify_source(&rel);
                    let preview = serialize_skill(&doc.frontmatter, &doc.body);
                    let questions = migrate_skill_questions(&doc, &source);
                    candidates.push(MigrateCandidate {
                        kind: "skill".into(),
                        source,
                        source_path: rel,
                        key,
                        preview,
                        questions,
                        rule: None,
                        skill: Some(doc),
                    });
                }
                Ok(None) => {}
                Err(reason) => skipped.push(MigrateSkipped {
                    source_path: rel,
                    reason,
                }),
            }
            continue;
        }

        if path.extension().and_then(|e| e.to_str()) != Some("mdc") {
            continue;
        }

        if is_bootstrap_rule(&rel, file_name) {
            skipped.push(MigrateSkipped {
                source_path: rel,
                reason: "IDE bootstrap rule — not team policy".into(),
            });
            continue;
        }

        match try_parse_rule(path, &rel) {
            Ok(Some(doc)) => {
                let key = format!("rule:{}", doc.frontmatter.id);
                if !seen_keys.insert(key.clone()) {
                    skipped.push(MigrateSkipped {
                        source_path: rel,
                        reason: format!("duplicate rule id `{}`", doc.frontmatter.id),
                    });
                    continue;
                }
                let source = classify_source(&rel);
                let preview = serialize_rule(&doc.frontmatter, &doc.body);
                let questions = migrate_rule_questions(&doc, &source);
                candidates.push(MigrateCandidate {
                    kind: "rule".into(),
                    source,
                    source_path: rel,
                    key,
                    preview,
                    questions,
                    rule: Some(doc),
                    skill: None,
                });
            }
            Ok(None) => {}
            Err(reason) => skipped.push(MigrateSkipped {
                source_path: rel,
                reason,
            }),
        }
    }

    candidates.sort_by(|a, b| a.source_path.cmp(&b.source_path));

    let rules_found = candidates.iter().filter(|c| c.kind == "rule").count() as u32;
    let skills_found = candidates.iter().filter(|c| c.kind == "skill").count() as u32;

    MigratePlan {
        rules_found,
        skills_found,
        candidates,
        skipped,
        interview_instruction: migrate_interview_instruction(),
    }
}

/// Upsert all candidates into the database (apply after interview / `--yes`).
pub async fn import_migrate_candidates(
    pool: &SqlitePool,
    candidates: &[MigrateCandidate],
) -> Result<MigrateApplyResult, String> {
    let mut rules_imported = 0u32;
    let mut skills_imported = 0u32;

    for candidate in candidates {
        if let Some(rule) = &candidate.rule {
            upsert_rule_doc(pool, rule)
                .await
                .map_err(|e| e.to_string())?;
            rules_imported += 1;
        } else if let Some(skill) = &candidate.skill {
            upsert_skill_doc(pool, skill)
                .await
                .map_err(|e| e.to_string())?;
            skills_imported += 1;
        }
    }

    Ok(MigrateApplyResult {
        rules_imported,
        skills_imported,
        skipped: 0,
    })
}

/// Scan + import in one step (used when `--migrate --yes`).
pub async fn migrate_to_database(
    pool: &SqlitePool,
    project_root: &Path,
) -> Result<(MigratePlan, MigrateApplyResult), String> {
    let plan = scan_policy_candidates(project_root);
    let apply = import_migrate_candidates(pool, &plan.candidates).await?;
    Ok((plan, apply))
}

pub fn migrate_interview_instruction() -> String {
    "For each candidate in order: ask every question in questions[], apply answers to frontmatter/body, then import only items the user confirms (yes). Database mode stores rows in ax.db — source files outside .ax/policy/ are not deleted. Run `ax policy storage database --migrate --yes` to import all with parsed defaults.".into()
}

pub fn migrate_rule_questions(doc: &PolicyRuleDoc, source: &str) -> Vec<CaptureInterviewQuestion> {
    let fm = &doc.frontmatter;
    vec![
        CaptureInterviewQuestion {
            field: "import".into(),
            question: format!(
                "Import rule `{}` from {source} ({}) into ax.db?",
                fm.id,
                doc.source_path
            ),
            current: "pending".into(),
            options: vec!["yes".into(), "no".into(), "skip".into()],
            required: true,
        },
        CaptureInterviewQuestion {
            field: "storage".into(),
            question: "Storage destination for this rule".into(),
            current: "database".into(),
            options: vec!["database".into(), "skip".into()],
            required: true,
        },
        CaptureInterviewQuestion {
            field: "id".into(),
            question: format!("Rule id in ax.db — keep `{}` or rename?", fm.id),
            current: fm.id.clone(),
            options: vec![],
            required: true,
        },
        CaptureInterviewQuestion {
            field: "level".into(),
            question: "Severity: INFO, WARNING, or CRITICAL?".into(),
            current: fm.level.clone(),
            options: vec!["INFO".into(), "WARNING".into(), "CRITICAL".into()],
            required: true,
        },
        CaptureInterviewQuestion {
            field: "alwaysApply".into(),
            question: "Apply on every turn (alwaysApply) or only when triggers/globs match?".into(),
            current: fm.always_apply.to_string(),
            options: vec!["false".into(), "true".into()],
            required: true,
        },
        CaptureInterviewQuestion {
            field: "triggers".into(),
            question: "Activation keywords (comma-separated)".into(),
            current: fm.triggers.join(", "),
            options: vec![],
            required: false,
        },
        CaptureInterviewQuestion {
            field: "globs".into(),
            question: "File globs (empty = all files, e.g. **/*.tsx)".into(),
            current: fm.globs.join(", "),
            options: vec![],
            required: false,
        },
        CaptureInterviewQuestion {
            field: "priority".into(),
            question: "Priority 0–100 (higher = earlier in inject)".into(),
            current: fm.priority.to_string(),
            options: vec!["40".into(), "60".into(), "80".into(), "100".into()],
            required: false,
        },
        CaptureInterviewQuestion {
            field: "tags".into(),
            question: "Tags (comma-separated; add `migrated`?)".into(),
            current: fm.tags.join(", "),
            options: vec![],
            required: false,
        },
        CaptureInterviewQuestion {
            field: "sourceAction".into(),
            question: format!(
                "After import, keep original file at {} or note for removal?",
                doc.source_path
            ),
            current: "keep".into(),
            options: vec!["keep".into(), "remove-later".into()],
            required: false,
        },
    ]
}

pub fn migrate_skill_questions(doc: &PolicySkillDoc, source: &str) -> Vec<CaptureInterviewQuestion> {
    let fm = &doc.frontmatter;
    vec![
        CaptureInterviewQuestion {
            field: "import".into(),
            question: format!(
                "Import skill `{}` from {source} ({}) into ax.db?",
                fm.name,
                doc.source_path
            ),
            current: "pending".into(),
            options: vec!["yes".into(), "no".into(), "skip".into()],
            required: true,
        },
        CaptureInterviewQuestion {
            field: "storage".into(),
            question: "Storage destination for this skill".into(),
            current: "database".into(),
            options: vec!["database".into(), "skip".into()],
            required: true,
        },
        CaptureInterviewQuestion {
            field: "name".into(),
            question: format!("Skill name in ax.db — keep `{}` or rename?", fm.name),
            current: fm.name.clone(),
            options: vec![],
            required: true,
        },
        CaptureInterviewQuestion {
            field: "description".into(),
            question: "One-line description for matcher / inject".into(),
            current: fm.description.clone(),
            options: vec![],
            required: true,
        },
        CaptureInterviewQuestion {
            field: "triggers".into(),
            question: "Activation keywords (comma-separated)".into(),
            current: fm.triggers.join(", "),
            options: vec![],
            required: false,
        },
        CaptureInterviewQuestion {
            field: "priority".into(),
            question: "Priority 0–100".into(),
            current: fm.priority.to_string(),
            options: vec!["40".into(), "60".into(), "80".into(), "100".into()],
            required: false,
        },
        CaptureInterviewQuestion {
            field: "contextTask".into(),
            question: "Optional contextTask hint for ax_context routing".into(),
            current: fm.context_task.clone().unwrap_or_default(),
            options: vec![],
            required: false,
        },
        CaptureInterviewQuestion {
            field: "tags".into(),
            question: "Tags (comma-separated; add `migrated`?)".into(),
            current: fm.tags.join(", "),
            options: vec![],
            required: false,
        },
        CaptureInterviewQuestion {
            field: "sourceAction".into(),
            question: format!(
                "After import, keep original file at {} or note for removal?",
                doc.source_path
            ),
            current: "keep".into(),
            options: vec!["keep".into(), "remove-later".into()],
            required: false,
        },
    ]
}

fn should_skip_entry(path: &Path, project_root: &Path) -> bool {
    if !path.is_dir() {
        return false;
    }
    let rel = path
        .strip_prefix(project_root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();

    if rel.is_empty() {
        return false;
    }

    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if SKIP_DIR_NAMES.contains(&name) {
        return true;
    }

    if name == ".ax" {
        return false;
    }

    if rel.starts_with(".ax/") && !rel.starts_with(".ax/policy") {
        return true;
    }

    false
}

fn classify_source(rel_path: &str) -> String {
    if rel_path.starts_with(".ax/policy/rules/")
        || rel_path.starts_with(".ax/policy/skills/")
        || rel_path.starts_with(".agents/rules/")
        || rel_path.starts_with(".agents/skills/")
    {
        "ax-policy".into()
    } else if rel_path.starts_with(".cursor/rules/") {
        "cursor-rules".into()
    } else if rel_path.contains(".cursor/skills/") {
        "cursor-skills".into()
    } else if rel_path.contains(".claude/skills/") {
        "claude-skills".into()
    } else if rel_path.ends_with(".mdc") {
        "discovered-rule".into()
    } else if rel_path.ends_with("SKILL.md") {
        "discovered-skill".into()
    } else {
        "discovered".into()
    }
}

fn is_bootstrap_rule(rel_path: &str, file_name: &str) -> bool {
    if !rel_path.starts_with(".cursor/rules/") {
        return false;
    }
    BOOTSTRAP_RULE_FILES.contains(&file_name)
}

fn try_parse_rule(path: &Path, rel: &str) -> Result<Option<PolicyRuleDoc>, String> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => return Err(format!("read error: {e}")),
    };
    if !raw.trim_start().starts_with("---") {
        return Ok(None);
    }
    let mut doc = parse_rule_file(path, &raw).map_err(|e| e.error)?;
    doc.source_path = rel.to_string();
    Ok(Some(doc))
}

fn try_parse_skill(path: &Path, rel: &str) -> Result<Option<PolicySkillDoc>, String> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => return Err(format!("read error: {e}")),
    };
    if !raw.trim_start().starts_with("---") {
        return Ok(None);
    }
    let mut doc = parse_skill_file(path, &raw).map_err(|e| e.error)?;
    doc.source_path = rel.to_string();
    Ok(Some(doc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_rule(dir: &Path, rel: &str, id: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let body = format!(
            r#"---
id: {id}
level: WARNING
triggers: [test]
---
# {id}
"#
        );
        fs::write(path, body).unwrap();
    }

    fn write_skill(dir: &Path, rel: &str, name: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let body = format!(
            r#"---
name: {name}
description: Test skill {name}
triggers: [test]
---
# {name}
"#
        );
        fs::write(path, body).unwrap();
    }

    #[test]
    fn scan_finds_ax_policy_and_cursor_rules() {
        let dir = tempfile::tempdir().unwrap();
        write_rule(dir.path(), ".ax/policy/rules/team.mdc", "team");
        write_rule(dir.path(), ".cursor/rules/custom.mdc", "custom");
        write_rule(dir.path(), ".cursor/rules/ax.mdc", "ax-bootstrap");

        let plan = scan_policy_candidates(dir.path());
        assert_eq!(plan.rules_found, 2);
        assert!(plan.skipped.iter().any(|s| s.source_path.contains("ax.mdc")));
        assert!(plan.candidates.iter().any(|c| c.key == "rule:team"));
        assert!(plan.candidates.iter().any(|c| c.key == "rule:custom"));
    }

    #[test]
    fn scan_finds_skills_recursively() {
        let dir = tempfile::tempdir().unwrap();
        write_skill(
            dir.path(),
            ".cursor/skills/deploy/SKILL.md",
            "deploy",
        );
        write_skill(
            dir.path(),
            "packages/app/.cursor/skills/review/SKILL.md",
            "review",
        );

        let plan = scan_policy_candidates(dir.path());
        assert_eq!(plan.skills_found, 2);
        assert!(plan.candidates.iter().all(|c| !c.questions.is_empty()));
    }

    #[test]
    fn scan_skips_node_modules() {
        let dir = tempfile::tempdir().unwrap();
        write_rule(dir.path(), "node_modules/pkg/rules/hidden.mdc", "hidden");
        let plan = scan_policy_candidates(dir.path());
        assert_eq!(plan.rules_found, 0);
    }

    #[test]
    fn migrate_rule_questions_include_storage() {
        let dir = tempfile::tempdir().unwrap();
        write_rule(dir.path(), ".ax/policy/rules/foo.mdc", "foo");
        let plan = scan_policy_candidates(dir.path());
        let c = plan.candidates.first().unwrap();
        assert!(c.questions.iter().any(|q| q.field == "storage"));
        assert!(c.questions.iter().any(|q| q.field == "import"));
    }
}
