use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use sqlx::SqlitePool;

use ax_utils::errors::AxError;

use crate::format::format_inject_block;
use crate::index::list_rules;
use crate::types::{MatchInput, MatchResult, MatchedRule, MatchedSkill, PolicyLevel, PolicyRuleRow};

pub async fn match_policy(pool: &SqlitePool, input: &MatchInput) -> Result<MatchResult, AxError> {
    let rules = list_rules(pool).await?;
    let skills = crate::index::list_skills(pool).await?;
    let prompt_lc = input.prompt.to_lowercase();
    let files = collect_relative_files(&input.cwd, &input.open_files, &input.changed_files);

    let mut matched_rules: Vec<(i32, MatchedRule)> = Vec::new();
    for rule in &rules {
        if let Some(m) = score_rule(rule, &prompt_lc, &files) {
            matched_rules.push((rule.priority, m));
        }
    }
    matched_rules.sort_by(|a, b| {
        level_ord(&b.1.level)
            .cmp(&level_ord(&a.1.level))
            .then(b.0.cmp(&a.0))
            .then(a.1.id.cmp(&b.1.id))
    });
    let rules_out: Vec<MatchedRule> = matched_rules.into_iter().map(|(_, r)| r).collect();

    let mut matched_skills: Vec<(i32, MatchedSkill)> = Vec::new();
    for skill in &skills {
        if let Some(m) = score_skill(skill, &prompt_lc) {
            matched_skills.push((skill.priority, m));
        }
    }
    matched_skills.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.name.cmp(&b.1.name)));
    let skills_out: Vec<MatchedSkill> = matched_skills.into_iter().take(2).map(|(_, s)| s).collect();

    let max_chars = max_inject_chars();
    let inject = format_inject_block(&rules_out, &skills_out, max_chars);

    Ok(MatchResult {
        rules: rules_out,
        skills: skills_out,
        inject,
    })
}

fn score_rule(rule: &PolicyRuleRow, prompt_lc: &str, files: &[String]) -> Option<MatchedRule> {
    let mut score = 0i32;
    let mut reasons = Vec::new();

    if rule.always_apply {
        score += 100;
        reasons.push("alwaysApply".into());
    }

    if !rule.globs.is_empty() && !files.is_empty() {
        if let Ok(set) = build_glob_set(&rule.globs) {
            for f in files {
                if set.is_match(f) {
                    score += 30;
                    reasons.push(format!("glob:{f}"));
                    break;
                }
            }
        }
    }

    for trigger in &rule.triggers {
        let t = trigger.to_lowercase();
        if !t.is_empty() && prompt_lc.contains(&t) {
            score += 20;
            reasons.push(format!("trigger:{trigger}"));
        }
    }

    if score == 0 {
        return None;
    }

    Some(MatchedRule {
        id: rule.id.clone(),
        level: rule.level.clone(),
        score,
        reason: reasons.join(", "),
        body: rule.body.clone(),
    })
}

fn score_skill(skill: &crate::types::PolicySkillRow, prompt_lc: &str) -> Option<MatchedSkill> {
    let mut score = 0i32;
    let mut reasons = Vec::new();

    for trigger in &skill.triggers {
        let t = trigger.to_lowercase();
        if !t.is_empty() && prompt_lc.contains(&t) {
            score += 25;
            reasons.push(format!("trigger:{trigger}"));
        }
    }

    let desc_lc = skill.description.to_lowercase();
    let words: Vec<&str> = prompt_lc.split_whitespace().filter(|w| w.len() > 3).collect();
    for w in words {
        if desc_lc.contains(w) {
            score += 5;
        }
    }

    if score == 0 {
        return None;
    }

    Some(MatchedSkill {
        name: skill.name.clone(),
        score,
        reason: reasons.join(", "),
        description: skill.description.clone(),
        body: skill.body.clone(),
    })
}

fn build_glob_set(globs: &[String]) -> Result<GlobSet, globset::Error> {
    let mut builder = GlobSetBuilder::new();
    for g in globs {
        builder.add(Glob::new(g)?);
    }
    builder.build()
}

fn collect_relative_files(cwd: &Path, open: &[PathBuf], changed: &[PathBuf]) -> Vec<String> {
    let mut out = Vec::new();
    for p in open.iter().chain(changed.iter()) {
        if let Ok(rel) = normalize_rel(cwd, p) {
            out.push(rel.replace('\\', "/"));
        }
    }
    out
}

fn normalize_rel(cwd: &Path, path: &Path) -> Result<String, ()> {
    let canon_cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canon
        .strip_prefix(&canon_cwd)
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|_| ())
}

fn level_ord(level: &str) -> i32 {
    match PolicyLevel::parse(level) {
        Some(PolicyLevel::Critical) => 3,
        Some(PolicyLevel::Warning) => 2,
        Some(PolicyLevel::Info) => 1,
        None => 0,
    }
}

pub fn max_inject_chars() -> usize {
    std::env::var("AX_POLICY_MAX_CHARS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(16_000)
}
