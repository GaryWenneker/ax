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

/// Sorted metadata fingerprints also detect changes within one timestamp tick.
type PolicyGeneration = (String, String);

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
        "SELECT
        (SELECT COALESCE(group_concat(id || ':' || content_hash || ':' || updated_at || ':' || enabled || ':' || status || ':' || scope || ':' || COALESCE(storage,'') || ':' || COALESCE(properties,'{}'), '|'), '') FROM (SELECT * FROM policy_rules ORDER BY id)),
        (SELECT COALESCE(group_concat(name || ':' || content_hash || ':' || updated_at || ':' || enabled || ':' || status || ':' || scope || ':' || COALESCE(storage,'') || ':' || COALESCE(properties,'{}'), '|'), '') FROM (SELECT * FROM policy_skills ORDER BY name))"
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

/// Explicit project policy overrides global defaults on the same name.
pub fn merge_skills(
    local: Vec<PolicySkillRow>,
    extras: Vec<PolicySkillRow>,
) -> Vec<PolicySkillRow> {
    let mut by_name = HashMap::new();
    for skill in extras {
        by_name.insert(skill.name.clone(), skill);
    }
    for skill in local {
        by_name.insert(skill.name.clone(), skill);
    }
    let mut out: Vec<PolicySkillRow> = by_name.into_values().collect();
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

pub fn find_skill<'a>(skills: &'a [PolicySkillRow], name: &str) -> Option<&'a PolicySkillRow> {
    skills.iter().find(|s| s.name == name)
}

pub async fn match_policy(pool: &SqlitePool, input: &MatchInput) -> Result<MatchResult, AxError> {
    match_policy_with_extra_skills(pool, input, Vec::new()).await
}

pub async fn match_policy_with_extra_skills(
    pool: &SqlitePool,
    input: &MatchInput,
    extras: Vec<PolicySkillRow>,
) -> Result<MatchResult, AxError> {
    let (rules, skills) = cached_rules_and_skills(pool).await?;
    let skills = if extras.is_empty() {
        skills
    } else {
        Arc::new(merge_skills((*skills).clone(), extras))
    };
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

pub fn is_approved_status(status: &str) -> bool {
    status.is_empty() || status.eq_ignore_ascii_case("approved")
}

/// `needle` occurs in `haystack` with no letter or digit directly before or after it.
fn contains_phrase(haystack: &str, needle: &str) -> bool {
    let is_word = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    haystack.match_indices(needle).any(|(start, _)| {
        let before = haystack[..start].chars().next_back();
        let after = haystack[start + needle.len()..].chars().next();
        !is_word(before) && !is_word(after)
    })
}

/// Word endings a skill trigger of [`INFLECTED_TRIGGER_MIN`]+ characters may carry
/// ("test" matches "tests" and the Dutch "testen").
const TRIGGER_SUFFIXES: &[&str] = &["s", "es", "d", "ed", "ing", "er", "ers", "en"];
const INFLECTED_TRIGGER_MIN: usize = 4;

/// `trigger` occurs in `prompt_lc` as a whole word or phrase. A trigger inside
/// another word never counts ("go" in "cargo"). A long enough trigger also
/// matches an inflected form of itself.
fn contains_trigger_word(prompt_lc: &str, trigger: &str) -> bool {
    let is_word = |c: char| c.is_alphanumeric();
    let inflects = trigger.chars().count() >= INFLECTED_TRIGGER_MIN;
    prompt_lc.match_indices(trigger).any(|(start, _)| {
        if prompt_lc[..start].chars().next_back().is_some_and(is_word) {
            return false;
        }
        let rest = &prompt_lc[start + trigger.len()..];
        let tail_len = rest.find(|c: char| !is_word(c)).unwrap_or(rest.len());
        let tail = &rest[..tail_len];
        tail.is_empty() || (inflects && TRIGGER_SUFFIXES.contains(&tail))
    })
}

/// Match reason marker for an always-apply rule whose globs exclude every touched file.
pub const OUT_OF_SCOPE: &str = "outOfScope";

fn score_rule(rule: &PolicyRuleRow, prompt_lc: &str, files: &[String]) -> Option<MatchedRule> {
    let mut score = 0i32;
    let mut reasons = Vec::new();

    if rule.always_apply {
        score += 100;
        reasons.push("alwaysApply".into());
    }

    if !rule.globs.is_empty() && !files.is_empty() {
        if let Ok(set) = build_glob_set(&rule.globs) {
            let hit = files.iter().find(|f| set.is_match(f.as_str()));
            match hit {
                Some(f) => {
                    score += 30;
                    reasons.push(format!("glob:{f}"));
                }
                None if rule.always_apply => reasons.push(OUT_OF_SCOPE.into()),
                None => {}
            }
        }
    }

    for trigger in &rule.triggers {
        let t = trigger.to_lowercase();
        if !t.is_empty() && contains_phrase(prompt_lc, &t) {
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
        properties: rule.properties.clone(),
    })
}

fn select_matched_skills(matched: Vec<(i32, MatchedSkill)>) -> Vec<MatchedSkill> {
    let (mut always, mut rest): (Vec<_>, Vec<_>) =
        matched.into_iter().partition(|(_, s)| s.always_apply);
    always.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.name.cmp(&b.1.name)));
    // Contextual skills compete on how well they match the prompt; priority only
    // breaks a tie in score.
    rest.sort_by(|a, b| {
        b.1.score
            .cmp(&a.1.score)
            .then(b.0.cmp(&a.0))
            .then(a.1.name.cmp(&b.1.name))
    });
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
        if !t.is_empty() && contains_trigger_word(prompt_lc, &t) {
            score += 25;
            reasons.push(format!("trigger:{trigger}"));
        }
    }

    let desc_lc = skill.description.to_lowercase();
    let words: Vec<&str> = prompt_lc
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 3)
        .collect();
    let description_matches: std::collections::HashSet<_> = words
        .into_iter()
        .filter(|w| contains_phrase(&desc_lc, w))
        .collect();
    // One generic word (such as "feature") is not applicability evidence.
    if description_matches.len() >= 2 {
        score += 5 * description_matches.len() as i32;
        reasons.push("description:multiple-terms".into());
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
        properties: skill.properties.clone(),
    })
}

fn build_glob_set(globs: &[String]) -> Result<GlobSet, globset::Error> {
    let mut builder = GlobSetBuilder::new();
    for g in globs {
        builder.add(Glob::new(g)?);
    }
    builder.build()
}

/// Turns open and changed file paths into cwd-relative, `/`-separated paths
/// for glob matching. A relative path is taken relative to `cwd`, not to the
/// process working directory, and a file that does not exist yet still
/// matches: the nearest existing ancestor is resolved and the rest appended.
fn collect_relative_files(cwd: &Path, open: &[PathBuf], changed: &[PathBuf]) -> Vec<String> {
    let lexical_cwd = lexical_normalize(&absolute_from(cwd, cwd));
    let canon_cwd = resolve_existing_prefix(&lexical_cwd);
    let mut out = Vec::new();
    for p in open.iter().chain(changed.iter()) {
        let abs = lexical_normalize(&absolute_from(cwd, p));
        let canon = resolve_existing_prefix(&abs);
        let rel = canon
            .strip_prefix(&canon_cwd)
            .or_else(|_| abs.strip_prefix(&lexical_cwd));
        if let Ok(rel) = rel {
            let rel = rel.to_string_lossy().replace('\\', "/");
            if !rel.is_empty() {
                out.push(rel);
            }
        }
    }
    out
}

/// `path` when absolute, otherwise `cwd` joined with it (and `cwd` itself
/// made absolute against the process directory as a last resort).
fn absolute_from(cwd: &Path, path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };
    if joined.is_absolute() {
        joined
    } else {
        std::env::current_dir()
            .map(|d| d.join(&joined))
            .unwrap_or(joined)
    }
}

/// Removes `.` and folds `..` without touching the file system.
fn lexical_normalize(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Canonicalizes the deepest existing ancestor of `path` and appends the
/// components below it that do not exist yet.
fn resolve_existing_prefix(path: &Path) -> PathBuf {
    let mut probe = path.to_path_buf();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(real) = probe.canonicalize() {
            let mut out = real;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match probe.file_name() {
            Some(name) => {
                rest.push(name.to_os_string());
                if !probe.pop() {
                    return path.to_path_buf();
                }
            }
            None => return path.to_path_buf(),
        }
    }
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
            group: String::new(),
            properties: Default::default(),
        }
    }

    fn rule_row(id: &str, always: bool, globs: &[&str], triggers: &[&str]) -> PolicyRuleRow {
        PolicyRuleRow {
            id: id.into(),
            level: "CRITICAL".into(),
            always_apply: always,
            globs: globs.iter().map(|s| (*s).to_string()).collect(),
            triggers: triggers.iter().map(|s| (*s).to_string()).collect(),
            tags: vec![],
            priority: 50,
            body: "RULE BODY".into(),
            source_path: String::new(),
            enabled: true,
            status: "approved".into(),
            scope: "project".into(),
            storage: None,
            source: None,
            root_id: None,
            stub_path: None,
            effective_storage: String::new(),
            storage_is_override: false,
            group: String::new(),
            properties: Default::default(),
        }
    }

    #[test]
    fn a_relative_file_resolves_against_the_input_cwd() {
        let dir = tempfile::tempdir().unwrap();
        // The file does not exist and the process runs in another directory.
        let rel = collect_relative_files(dir.path(), &[PathBuf::from("src/App.tsx")], &[]);
        assert_eq!(rel, vec!["src/App.tsx".to_string()]);
        let dotted = collect_relative_files(dir.path(), &[PathBuf::from("./src/../lib/a.rs")], &[]);
        assert_eq!(dotted, vec!["lib/a.rs".to_string()]);
    }

    #[cfg(unix)]
    #[test]
    fn a_new_file_under_a_linked_cwd_keeps_its_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir_all(real.join("src")).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        // cwd given through the link, file not yet created.
        let new_file = link.join("src/new.rs");
        assert_eq!(
            collect_relative_files(&link, &[new_file], &[]),
            vec!["src/new.rs".to_string()]
        );
        // cwd given canonically, file path through the link.
        let via_link = link.join("src/other.rs");
        assert_eq!(
            collect_relative_files(&real, &[via_link], &[]),
            vec!["src/other.rs".to_string()]
        );
        // A file outside the cwd is still dropped.
        assert!(collect_relative_files(&real, &[dir.path().join("x.rs")], &[]).is_empty());
    }

    #[test]
    fn a_generic_feature_word_cannot_select_an_unrelated_email_skill() {
        let mut skill = skill_row("feature-information", false, &[], 50);
        skill.description =
            "Send feature announcement emails to Hoornaarpreventie members via Brevo".into();
        assert!(score_skill(&skill, "implement the ax feature").is_none());
        assert!(score_skill(&skill, "send announcement emails").is_some());
    }

    #[test]
    fn skill_trigger_needs_a_whole_word() {
        let skill = skill_row("pr", false, &["pr"], 50);
        assert!(score_skill(&skill, "improve the prompt parser").is_none());
        assert!(score_skill(&skill, "express route").is_none());
        assert!(score_skill(&skill, "open a pr for this").is_some());
        let c = skill_row("c-review", false, &["c"], 50);
        assert!(score_skill(&c, "edit crates/ax-db/src/queries.rs").is_none());
        assert!(score_skill(&c, "review the c code").is_some());
    }

    #[test]
    fn skill_description_ignores_punctuation_around_prompt_words() {
        let skill = skill_row("old-coder", false, &[], 50);
        let hit = score_skill(&skill, "start the implementation, evidence-first!");
        assert!(
            hit.is_some_and(|m| m.reason.contains("description:multiple-terms")),
            "punctuation must not hide description words"
        );
    }

    #[test]
    fn c1_skill_trigger_inside_another_word_does_not_match() {
        let go = skill_row("go-review", false, &["go"], 50);
        let pr = skill_row("review-loop", false, &["pr"], 50);
        let prompt = "run cargo fmt on the preflights";
        assert!(score_skill(&go, prompt).is_none());
        assert!(score_skill(&pr, prompt).is_none());
    }

    #[test]
    fn c2_whole_word_skill_trigger_matches_case_insensitively() {
        let go = skill_row("go-review", false, &["go"], 50);
        let pr = skill_row("review-loop", false, &["PR"], 50);
        let hit = score_skill(&go, &"Review this Go handler".to_lowercase()).unwrap();
        assert!(hit.reason.contains("trigger:go"), "{}", hit.reason);
        let hit = score_skill(&pr, "open a pr").unwrap();
        assert!(hit.reason.contains("trigger:PR"), "{}", hit.reason);
    }

    #[test]
    fn c3_multi_word_skill_trigger_matches_as_a_phrase() {
        let skill = skill_row("old-coder", false, &["prove it works"], 50);
        assert!(score_skill(&skill, "please prove it works.").is_some());
        assert!(score_skill(&skill, "prove it somehow works").is_none());
    }

    #[test]
    fn c4_inflected_form_of_a_long_trigger_matches() {
        let test = skill_row("tdd", false, &["test"], 50);
        let implement = skill_row("old-coder", false, &["implement"], 50);
        assert!(score_skill(&test, "add tests").is_some());
        assert!(score_skill(&test, "meerdere testen").is_some());
        assert!(score_skill(&implement, "implementing the cap").is_some());
        let pr = skill_row("review-loop", false, &["pr"], 50);
        assert!(
            score_skill(&pr, "two prs").is_none(),
            "short triggers need the exact word"
        );
    }

    #[test]
    fn c5_unrelated_longer_word_does_not_match() {
        let spec = skill_row("old-coder", false, &["spec"], 50);
        let test = skill_row("tdd", false, &["test"], 50);
        let prompt = "a specific contest about testament";
        assert!(score_skill(&spec, prompt).is_none());
        assert!(score_skill(&test, prompt).is_none());
    }

    #[test]
    fn single_letter_trigger_needs_a_whole_word() {
        let rule = rule_row("c-memory", false, &[], &["c"]);
        assert!(score_rule(
            &rule,
            "edit crates/ax-db/src/queries.rs. which rules apply?",
            &[]
        )
        .is_none());
        assert!(score_rule(&rule, "fix the c code in main", &[]).is_some());
    }

    #[test]
    fn multi_word_trigger_still_matches_as_phrase() {
        let rule = rule_row("badges", false, &[], &["command center"]);
        assert!(score_rule(&rule, "restyle the command center list", &[]).is_some());
    }

    #[test]
    fn always_rule_with_globs_is_out_of_scope_for_other_files() {
        let rule = rule_row("wcag", true, &["crates/ax-web/web-ui/**"], &[]);
        let rs = score_rule(&rule, "edit", &["crates/ax-db/src/queries.rs".into()]).unwrap();
        assert!(rs.reason.contains(OUT_OF_SCOPE), "{}", rs.reason);
        let ui = score_rule(
            &rule,
            "edit",
            &["crates/ax-web/web-ui/src/pages/Ship.tsx".into()],
        )
        .unwrap();
        assert!(!ui.reason.contains(OUT_OF_SCOPE), "{}", ui.reason);
        let none = score_rule(&rule, "edit", &[]).unwrap();
        assert!(!none.reason.contains(OUT_OF_SCOPE), "{}", none.reason);
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
            (
                10,
                MatchedSkill {
                    name: "a".into(),
                    score: 100,
                    reason: "alwaysApply".into(),
                    description: String::new(),
                    body: String::new(),
                    always_apply: true,
                    properties: Default::default(),
                },
            ),
            (
                90,
                MatchedSkill {
                    name: "b".into(),
                    score: 100,
                    reason: "alwaysApply".into(),
                    description: String::new(),
                    body: String::new(),
                    always_apply: true,
                    properties: Default::default(),
                },
            ),
            (
                50,
                MatchedSkill {
                    name: "c".into(),
                    score: 25,
                    reason: "trigger:x".into(),
                    description: String::new(),
                    body: String::new(),
                    always_apply: false,
                    properties: Default::default(),
                },
            ),
            (
                40,
                MatchedSkill {
                    name: "d".into(),
                    score: 25,
                    reason: "trigger:x".into(),
                    description: String::new(),
                    body: String::new(),
                    always_apply: false,
                    properties: Default::default(),
                },
            ),
            (
                30,
                MatchedSkill {
                    name: "e".into(),
                    score: 25,
                    reason: "trigger:x".into(),
                    description: String::new(),
                    body: String::new(),
                    always_apply: false,
                    properties: Default::default(),
                },
            ),
        ];
        let out = select_matched_skills(matched);
        let names: Vec<&str> = out.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["b", "a", "c", "d"]);
        assert!(out.iter().filter(|s| s.always_apply).count() == 2);
    }

    #[test]
    fn contextual_skills_rank_by_match_score_before_priority() {
        let skill = |name: &str, score: i32| MatchedSkill {
            name: name.into(),
            score,
            reason: String::new(),
            description: String::new(),
            body: String::new(),
            always_apply: false,
            properties: Default::default(),
        };
        // a and b match on two description words; c matches two triggers.
        let matched = vec![
            (80, skill("a", 10)),
            (80, skill("b", 10)),
            (50, skill("c", 50)),
        ];
        let out = select_matched_skills(matched);
        let names: Vec<&str> = out.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["c", "a"]);
    }

    #[test]
    fn merge_skills_project_override_wins() {
        let local = vec![skill_row("azdo-pr-review", false, &["local"], 1)];
        let mut extra = skill_row("azdo-pr-review", false, &["global"], 80);
        extra.body = "GLOBAL".into();
        extra.description = "from global.db".into();
        let merged = merge_skills(local, vec![extra]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].triggers, vec!["local".to_string()]);
        assert_eq!(merged[0].body, "BODY");
    }

    #[test]
    fn merge_skills_adds_global_only_name() {
        let local = vec![skill_row("startup", true, &[], 100)];
        let extra = skill_row("azdo-pr-review", false, &["azdo pr review"], 80);
        let merged = merge_skills(local, vec![extra]);
        let names: Vec<&str> = merged.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["azdo-pr-review", "startup"]);
        assert!(find_skill(&merged, "azdo-pr-review").is_some());
    }
}
