use crate::types::{MatchedRule, MatchedSkill, MatchResult, PolicyStatus, PreflightMeta};

pub fn format_inject_block(rules: &[MatchedRule], skills: &[MatchedSkill], max_chars: usize) -> String {
    if rules.is_empty() && skills.is_empty() {
        return String::new();
    }

    let (always, contextual): (Vec<_>, Vec<_>) = rules.iter().partition(|r| r.always_apply);

    let mut body = String::from(
        "<ax_policy note=\"Team rules and skills matched for this prompt — apply before editing.\">\n",
    );

    if !always.is_empty() {
        body.push_str("## Rules (always apply)\n\n");
        for r in &always {
            append_rule(&mut body, r);
        }
    }

    if !contextual.is_empty() {
        body.push_str("## Rules (matched for this prompt)\n\n");
        let mut omitted = Vec::new();
        for r in &contextual {
            let next = rule_block(r);
            if body.len() + next.len() + 80 > max_chars {
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

    if !skills.is_empty() {
        body.push_str("## Suggested skills\n\n");
        for s in skills {
            body.push_str(&format!("### skill: {}\n\n", s.name));
            body.push_str(&s.description);
            body.push_str("\n\n");
            body.push_str(&s.body);
            body.push_str("\n\n");
        }
    }

    body.push_str("</ax_policy>\n");

    if body.len() > max_chars {
        format!(
            "{}\n...(truncated; call ax_preflight or ax_rules for full policy)",
            truncate_at_char_boundary(&body, max_chars)
        )
    } else {
        body
    }
}

fn append_rule(body: &mut String, r: &MatchedRule) {
    body.push_str(&rule_block(r));
}

fn rule_block(r: &MatchedRule) -> String {
    format!("### [{}] {}\n\n{}\n\n", r.level, r.id, r.body)
}

fn truncate_at_char_boundary(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
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
    }
}
