use crate::types::{MatchedRule, MatchedSkill, MatchResult, PolicyStatus, PreflightMeta};

pub fn format_inject_block(rules: &[MatchedRule], skills: &[MatchedSkill], max_chars: usize) -> String {
    if rules.is_empty() && skills.is_empty() {
        return String::new();
    }

    let (always, contextual): (Vec<_>, Vec<_>) = rules.iter().partition(|r| r.always_apply);

    let mut body = String::from(
        "<ax_policy note=\"Team rules and skills matched for this prompt — apply before editing.\">\n",
    );

    // Always-apply rules are the preflight contract — never hard-truncate them.
    // If they exceed max_chars, the inject grows rather than cutting mid-rule.
    if !always.is_empty() {
        body.push_str("## Rules (always apply)\n\n");
        for r in &always {
            append_rule(&mut body, r);
        }
    }

    const FOOTER: &str = "</ax_policy>\n";
    let budget = |current: usize, extra: usize| current + extra + FOOTER.len() + 160;

    if !contextual.is_empty() {
        body.push_str("## Rules (matched for this prompt)\n\n");
        let mut omitted = Vec::new();
        for r in &contextual {
            let next = rule_block(r);
            if budget(body.len(), next.len()) > max_chars {
                omitted.push(r.id.as_str());
                continue;
            }
            body.push_str(&next);
        }
        if !omitted.is_empty() {
            body.push_str(&format!(
                "_Contextual rules truncated: {}. Call `ax_rules` with your prompt for full bodies._\n\n",
                omitted.join(", ")
            ));
        }
    }

    let (always_skills, contextual_skills): (Vec<_>, Vec<_>) =
        skills.iter().partition(|s| s.always_apply);

    // Always-apply skills are part of the preflight contract — never omit or truncate them.
    if !always_skills.is_empty() {
        body.push_str("## Skills (always apply)\n\n");
        for s in &always_skills {
            body.push_str(&skill_block(s));
        }
    }

    if !contextual_skills.is_empty() {
        let mut skill_section = String::from("## Suggested skills\n\n");
        let mut omitted_skills = Vec::new();
        for s in &contextual_skills {
            let block = skill_block(s);
            if budget(body.len() + skill_section.len(), block.len()) > max_chars {
                omitted_skills.push(s.name.as_str());
                continue;
            }
            skill_section.push_str(&block);
        }
        if omitted_skills.len() == contextual_skills.len() {
            body.push_str(&format!(
                "_Skills omitted to keep always-apply rules and skills intact: {}. Call `ax_skill` by name._\n\n",
                omitted_skills.join(", ")
            ));
        } else {
            if !omitted_skills.is_empty() {
                skill_section.push_str(&format!(
                    "_Skills omitted: {}. Call `ax_skill` by name._\n\n",
                    omitted_skills.join(", ")
                ));
            }
            body.push_str(&skill_section);
        }
    }

    body.push_str(FOOTER);
    body
}

fn append_rule(body: &mut String, r: &MatchedRule) {
    body.push_str(&rule_block(r));
}

fn rule_block(r: &MatchedRule) -> String {
    format!("### [{}] {}\n\n{}\n\n", r.level, r.id, r.body)
}

fn skill_block(s: &MatchedSkill) -> String {
    format!(
        "### skill: {}\n\n{}\n\n{}\n\n",
        s.name, s.description, s.body
    )
}

pub fn build_preflight_meta(status: &PolicyStatus, result: &MatchResult) -> PreflightMeta {
    let guard_required = result
        .rules
        .iter()
        .any(|r| r.level.eq_ignore_ascii_case("CRITICAL"));
    PreflightMeta {
        mode: status.mode.clone(),
        policy_status: status.clone(),
        matched_rules: result.rules.len(),
        matched_skills: result.skills.len(),
        guard_required,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str, always: bool, body: &str) -> MatchedRule {
        MatchedRule {
            id: id.into(),
            level: "CRITICAL".into(),
            score: if always { 100 } else { 20 },
            reason: if always { "alwaysApply".into() } else { "trigger:x".into() },
            always_apply: always,
            body: body.into(),
        }
    }

    #[test]
    fn always_apply_rules_kept_when_truncating() {
        let rules = vec![
            rule("always-a", true, "AAAA"),
            rule("ctx-b", false, &"B".repeat(20_000)),
        ];
        let inject = format_inject_block(&rules, &[], 500);
        assert!(inject.contains("always-a"));
        assert!(inject.contains("Rules (always apply)"));
        assert!(inject.contains("</ax_policy>"));
        assert!(!inject.contains("...(truncated"));
    }

    #[test]
    fn always_apply_never_hard_truncated_when_over_budget() {
        let rules = vec![
            rule("always-a", true, &"A".repeat(400)),
            rule("always-b", true, &"B".repeat(400)),
            rule("ctx-c", false, &"C".repeat(5_000)),
        ];
        let inject = format_inject_block(&rules, &[], 500);
        assert!(inject.contains("always-a"), "missing always-a:\n{inject}");
        assert!(inject.contains("always-b"), "missing always-b:\n{inject}");
        assert!(inject.contains("</ax_policy>"));
        assert!(!inject.contains("...(truncated"));
        assert!(inject.contains("Contextual rules truncated"));
    }

    fn skill(name: &str, body: &str) -> MatchedSkill {
        skill_with(name, body, false)
    }

    fn skill_with(name: &str, body: &str, always: bool) -> MatchedSkill {
        MatchedSkill {
            name: name.into(),
            score: if always { 100 } else { 25 },
            reason: if always { "alwaysApply".into() } else { "trigger:x".into() },
            description: "desc".into(),
            body: body.into(),
            always_apply: always,
        }
    }

    #[test]
    fn skills_omitted_instead_of_cutting_always_apply() {
        let rules = vec![rule("always-keep", true, "KEEPME")];
        let skills = vec![skill("huge", &"S".repeat(10_000))];
        let inject = format_inject_block(&rules, &skills, 200);
        assert!(inject.contains("KEEPME"));
        assert!(inject.contains("</ax_policy>"));
        assert!(!inject.contains(&"S".repeat(80)));
        assert!(inject.contains("ax_skill"));
    }

    #[test]
    fn always_apply_skills_never_omitted_when_over_budget() {
        let skills = vec![skill_with("old-coder", &"G".repeat(8_000), true)];
        let inject = format_inject_block(&[], &skills, 200);
        assert!(inject.contains("Skills (always apply)"));
        assert!(inject.contains("old-coder"));
        assert!(inject.contains(&"G".repeat(80)));
        assert!(!inject.contains("...(truncated"));
        assert!(inject.contains("</ax_policy>"));
    }
}
