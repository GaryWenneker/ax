use crate::types::{MatchedRule, MatchedSkill};

pub fn format_inject_block(rules: &[MatchedRule], skills: &[MatchedSkill], max_chars: usize) -> String {
    if rules.is_empty() && skills.is_empty() {
        return String::new();
    }

    let mut body = String::from("<ax_policy note=\"Team rules and skills matched for this prompt — apply before editing.\">\n");

    if !rules.is_empty() {
        body.push_str("## Rules\n\n");
        for r in rules {
            body.push_str(&format!("### [{}] {}\n\n", r.level, r.id));
            body.push_str(&r.body);
            body.push_str("\n\n");
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
            &body[..max_chars]
        )
    } else {
        body
    }
}
