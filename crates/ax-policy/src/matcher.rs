use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use globset::{Glob, GlobSet, GlobSetBuilder};
use sqlx::SqlitePool;

use ax_utils::errors::AxError;

use crate::format::format_inject_block;
use crate::index::list_rules;
use crate::types::{
    MatchInput, MatchResult, MatchedRule, MatchedSkill, PolicyLevel, PolicyRuleRow, PolicySkillRow,
};

/// (max updated_at, count) per table — cheap staleness fingerprint.
type PolicyGeneration = (i64, i64, i64, i64);

struct PolicyCacheEntry {
    generation: PolicyGeneration,
    rules: Arc<Vec<PolicyRuleRow>>,
    skills: Arc<Vec<PolicySkillRow>>,
}

static POLICY_CACHE: OnceLock<Mutex<HashMap<String, PolicyCacheEntry>>> = OnceLock::new();

fn policy_cache() -> &'static Mutex<HashMap<String, PolicyCacheEntry>> {
    POLICY_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn policy_generation(pool: &SqlitePool) -> Result<PolicyGeneration, AxError> {
    sqlx::query_as(
        "SELECT (SELECT COALESCE(MAX(updated_at), 0) FROM policy_rules),
                (SELECT COUNT(*) FROM policy_rules),
                (SELECT COALESCE(MAX(updated_at), 0) FROM policy_skills),
                (SELECT COUNT(*) FROM policy_skills)",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| AxError::Database(ax_utils::errors::DatabaseError::new(e.to_string())))
}

/// Load rules + skills through a per-database cache. A one-row generation
/// query decides staleness so full policy bodies are not re-read from SQLite
/// on every preflight call.
pub async fn cached_rules_and_skills(
    pool: &SqlitePool,
) -> Result<(Arc<Vec<PolicyRuleRow>>, Arc<Vec<PolicySkillRow>>), AxError> {
    let key = pool.connect_options().get_filename().display().to_string();
    let generation = policy_generation(pool).await?;

    if let Ok(cache) = policy_cache().lock() {
        if let Some(entry) = cache.get(&key) {
            if entry.generation == generation {
                return Ok((Arc::clone(&entry.rules), Arc::clone(&entry.skills)));
            }
        }
    }

    let rules = Arc::new(list_rules(pool).await?);
    let skills = Arc::new(crate::index::list_skills(pool).await?);
    if let Ok(mut cache) = policy_cache().lock() {
        cache.insert(
            key,
            PolicyCacheEntry {
                generation,
                rules: Arc::clone(&rules),
                skills: Arc::clone(&skills),
            },
        );
    }
    Ok((rules, skills))
}

pub async fn match_policy(pool: &SqlitePool, input: &MatchInput) -> Result<MatchResult, AxError> {
    let (rules, skills) = cached_rules_and_skills(pool).await?;
    let prompt_lc = input.prompt.to_lowercase();
    let files = collect_relative_files(&input.cwd, &input.open_files, &input.changed_files);

    let mut matched_rules: Vec<(i32, MatchedRule)> = Vec::new();
    for rule in rules.iter() {
        if !rule.enabled || !is_approved_status(&rule.status) {
            continue;
        }
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
    for skill in skills.iter() {
        if !skill.enabled || !is_approved_status(&skill.status) {
            continue;
        }
        if let Some(m) = score_skill(skill, &prompt_lc) {
            matched_skills.push((skill.priority, m));
        }
    }
    let skills_out = select_matched_skills(matched_skills);

    let max_chars = max_inject_chars();
    let inject = format_inject_block(&rules_out, &skills_out, max_chars);

    Ok(MatchResult {
        rules: rules_out,
        skills: skills_out,
        inject,
    })
}

fn is_approved_status(status: &str) -> bool {
    status.is_empty()
        || status.eq_ignore_ascii_case("approved")
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
        always_apply: rule.always_apply,
        body: rule.body.clone(),
    })
}

fn select_matched_skills(matched: Vec<(i32, MatchedSkill)>) -> Vec<MatchedSkill> {
    let (mut always, mut rest): (Vec<_>, Vec<_>) =
        matched.into_iter().partition(|(_, s)| s.always_apply);
    always.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.name.cmp(&b.1.name)));
    rest.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.name.cmp(&b.1.name)));
    let mut out: Vec<MatchedSkill> = always.into_iter().map(|(_, s)| s).collect();
    out.extend(rest.into_iter().take(2).map(|(_, s)| s));
    out
}

fn score_skill(skill: &crate::types::PolicySkillRow, prompt_lc: &str) -> Option<MatchedSkill> {
    let mut score = 0i32;
    let mut reasons = Vec::new();

    if skill.always_apply {
        score += 100;
        reasons.push("alwaysApply".into());
    }

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
        always_apply: skill.always_apply,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::PolicySkillRow;

    fn skill_row(name: &str, always: bool, triggers: &[&str], priority: i32) -> PolicySkillRow {
        PolicySkillRow {
            name: name.into(),
            description: "evidence-first implementation workflow".into(),
            always_apply: always,
            triggers: triggers.iter().map(|s| (*s).to_string()).collect(),
            tags: vec![],
            priority,
            context_task: None,
            body: "BODY".into(),
            source_path: String::new(),
            enabled: true,
            status: "approved".into(),
            scope: "company".into(),
            storage: None,
            source: None,
            root_id: None,
            stub_path: None,
            effective_storage: String::new(),
            storage_is_override: false,
        }
    }

    #[test]
    fn empty_prompt_matches_always_apply_skill() {
        let skill = skill_row("old-coder", true, &["implement"], 90);
        let matched = score_skill(&skill, "").expect("alwaysApply must match empty prompt");
        assert_eq!(matched.name, "old-coder");
        assert!(matched.always_apply);
        assert!(matched.reason.contains("alwaysApply"));
    }

    #[test]
    fn empty_prompt_skips_trigger_only_skill() {
        let skill = skill_row("startup", false, &["preflight"], 100);
        assert!(score_skill(&skill, "").is_none());
    }

    #[test]
    fn select_keeps_all_always_apply_plus_two_contextual() {
        let matched = vec![
            (10, MatchedSkill {
                name: "a".into(),
                score: 100,
                reason: "alwaysApply".into(),
                description: String::new(),
                body: String::new(),
                always_apply: true,
            }),
            (90, MatchedSkill {
                name: "b".into(),
                score: 100,
                reason: "alwaysApply".into(),
                description: String::new(),
                body: String::new(),
                always_apply: true,
            }),
            (50, MatchedSkill {
                name: "c".into(),
                score: 25,
                reason: "trigger:x".into(),
                description: String::new(),
                body: String::new(),
                always_apply: false,
            }),
            (40, MatchedSkill {
                name: "d".into(),
                score: 25,
                reason: "trigger:x".into(),
                description: String::new(),
                body: String::new(),
                always_apply: false,
            }),
            (30, MatchedSkill {
                name: "e".into(),
                score: 25,
                reason: "trigger:x".into(),
                description: String::new(),
                body: String::new(),
                always_apply: false,
            }),
        ];
        let out = select_matched_skills(matched);
        let names: Vec<&str> = out.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["b", "a", "c", "d"]);
        assert!(out.iter().filter(|s| s.always_apply).count() == 2);
    }
}
