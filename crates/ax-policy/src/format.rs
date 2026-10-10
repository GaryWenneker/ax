use std::collections::{HashMap, HashSet};

use crate::types::{
    MatchResult, MatchedRule, MatchedSkill, PolicyProperties, PolicyStatus, PreflightMeta,
};

/// Per-call delivery context for [`format_inject_block_with`].
#[derive(Default, Clone, Copy)]
pub struct InjectOptions<'a> {
    /// Keys (`rule:<id>` / `skill:<name>`) already sent in this session, with the body hash sent.
    pub delivered: Option<&'a HashMap<String, u64>>,
    /// Keys whose identical body the client already loads itself (IDE rule or skill files).
    pub client_loaded: Option<&'a HashSet<String>>,
    /// Always-apply skills longer than this are sent as a summary with an `ax_skill` pointer.
    pub skill_inline_chars: Option<usize>,
    /// Always-apply rules longer than this are sent as their directive sections with an `ax_rules` pointer.
    pub rule_inline_chars: Option<usize>,
}

pub struct InjectOutput {
    pub text: String,
    /// Keys and hashes whose body (or summary) this block carried.
    pub delivered: Vec<(String, u64)>,
}

pub fn rule_key(id: &str) -> String {
    format!("rule:{id}")
}

pub fn skill_key(name: &str) -> String {
    format!("skill:{name}")
}

/// FNV-1a 64: stable across processes and Rust versions, unlike `DefaultHasher`.
pub fn content_hash(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Hash of the body plus extra properties, so a property-only edit is resent.
pub fn delivered_body_hash(body: &str, properties: &PolicyProperties) -> u64 {
    content_hash(delivery_text(body, properties).as_ref())
}

fn delivery_text<'a>(body: &'a str, properties: &PolicyProperties) -> std::borrow::Cow<'a, str> {
    let rendered = crate::parse::render_properties(properties);
    if rendered.is_empty() {
        std::borrow::Cow::Borrowed(body)
    } else {
        std::borrow::Cow::Owned(format!("{body}\n{rendered}"))
    }
}

fn with_properties(properties: &PolicyProperties, body: &str) -> String {
    let rendered = crate::parse::render_properties(properties);
    if rendered.is_empty() {
        body.to_string()
    } else {
        format!("{rendered}\n{body}")
    }
}

enum Delivery {
    Send,
    Unchanged,
    ClientLoaded,
}

struct Tracker<'a> {
    opts: InjectOptions<'a>,
    unchanged: Vec<String>,
    client_loaded: Vec<String>,
    delivered: Vec<(String, u64)>,
}

impl<'a> Tracker<'a> {
    fn new(opts: InjectOptions<'a>) -> Self {
        Self {
            opts,
            unchanged: Vec::new(),
            client_loaded: Vec::new(),
            delivered: Vec::new(),
        }
    }

    /// Decide whether a body goes out, and record that decision.
    fn decide(&mut self, key: String, label: &str, body: &str) -> Delivery {
        let hash = content_hash(body);
        if self
            .opts
            .client_loaded
            .is_some_and(|set| set.contains(&key))
        {
            self.client_loaded.push(label.to_string());
            return Delivery::ClientLoaded;
        }
        if self.opts.delivered.and_then(|map| map.get(&key)) == Some(&hash) {
            self.unchanged.push(label.to_string());
            return Delivery::Unchanged;
        }
        self.delivered.push((key, hash));
        Delivery::Send
    }

    fn notes(&self) -> String {
        let mut out = String::new();
        if !self.unchanged.is_empty() {
            out.push_str(&format!(
                "_Unchanged since earlier in this session (bodies not resent): {}. If they are not in your context, call `ax_rules` / `ax_skill` by name._\n\n",
                self.unchanged.join(", ")
            ));
        }
        if !self.client_loaded.is_empty() {
            out.push_str(&format!(
                "_Loaded by your IDE with identical text (bodies not resent): {}._\n\n",
                self.client_loaded.join(", ")
            ));
        }
        out
    }
}

pub fn format_inject_block_with(
    rules: &[MatchedRule],
    skills: &[MatchedSkill],
    max_chars: usize,
    opts: InjectOptions<'_>,
) -> InjectOutput {
    if rules.is_empty() && skills.is_empty() {
        return InjectOutput {
            text: String::new(),
            delivered: Vec::new(),
        };
    }
    let mut tracker = Tracker::new(opts);
    let (always, contextual): (Vec<_>, Vec<_>) = rules.iter().partition(|r| r.always_apply);

    let mut body = String::from(
        "<ax_policy note=\"Team rules and skills matched for this prompt — apply before editing.\">\n",
    );

    // Always-apply rules are the preflight contract — never hard-truncate them.
    // If they exceed max_chars, the inject grows rather than cutting mid-rule.
    let (out_of_scope, always): (Vec<_>, Vec<_>) = always
        .into_iter()
        .partition(|r| r.reason.contains(crate::matcher::OUT_OF_SCOPE));
    if !always.is_empty() {
        body.push_str("## Rules (always apply)\n\n");
        for r in &always {
            let label = format!("{} ({})", r.id, r.level);
            if let Delivery::Send = tracker.decide(
                rule_key(&r.id),
                &label,
                delivery_text(&r.body, &r.properties).as_ref(),
            ) {
                match opts.rule_inline_chars {
                    Some(limit) if r.body.len() > limit => body.push_str(&compact_rule_block(r)),
                    _ => body.push_str(&rule_block(r)),
                }
            }
        }
    }
    if !out_of_scope.is_empty() {
        let ids: Vec<&str> = out_of_scope.iter().map(|r| r.id.as_str()).collect();
        body.push_str(&format!(
            "_Always-apply rules scoped to other files (not sent; `ax_rules` if you touch their files): {}._\n\n",
            ids.join(", ")
        ));
    }

    const FOOTER: &str = "</ax_policy>\n";
    let budget = |current: usize, extra: usize| current + extra + FOOTER.len() + 160;

    if !contextual.is_empty() {
        body.push_str("## Rules (matched for this prompt)\n\n");
        let mut omitted = Vec::new();
        for r in &contextual {
            let next = rule_block(r);
            if budget(body.len(), next.len()) > max_chars
                && !r.level.eq_ignore_ascii_case("CRITICAL")
            {
                omitted.push(r.id.as_str());
                continue;
            }
            let label = format!("{} ({})", r.id, r.level);
            if let Delivery::Send = tracker.decide(
                rule_key(&r.id),
                &label,
                delivery_text(&r.body, &r.properties).as_ref(),
            ) {
                body.push_str(&next);
            }
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

    // Always-apply skills are never omitted. Past `skill_inline_chars` they are
    // summarized with an `ax_skill` pointer rather than cut mid-body.
    if !always_skills.is_empty() {
        body.push_str("## Skills (always apply)\n\n");
        for s in &always_skills {
            let label = format!("skill {}", s.name);
            if let Delivery::Send = tracker.decide(
                skill_key(&s.name),
                &label,
                delivery_text(&s.body, &s.properties).as_ref(),
            ) {
                let summarize = opts
                    .skill_inline_chars
                    .is_some_and(|limit| s.body.len() > limit);
                body.push_str(&if summarize {
                    skill_summary_block(s)
                } else {
                    skill_block(s)
                });
            }
        }
    }

    if !contextual_skills.is_empty() {
        let mut skill_section = String::from("## Suggested skills\n\n");
        let mut omitted_skills = Vec::new();
        let mut listed = 0usize;
        for s in &contextual_skills {
            let block = skill_block(s);
            if budget(body.len() + skill_section.len(), block.len()) > max_chars {
                omitted_skills.push(s.name.as_str());
                continue;
            }
            listed += 1;
            let label = format!("skill {}", s.name);
            if let Delivery::Send = tracker.decide(
                skill_key(&s.name),
                &label,
                delivery_text(&s.body, &s.properties).as_ref(),
            ) {
                skill_section.push_str(&block);
            }
        }
        if listed == 0 {
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

    body.push_str(&tracker.notes());
    body.push_str(FOOTER);
    InjectOutput {
        text: body,
        delivered: tracker.delivered,
    }
}

pub fn format_inject_block(
    rules: &[MatchedRule],
    skills: &[MatchedSkill],
    max_chars: usize,
) -> String {
    format_inject_block_with(rules, skills, max_chars, InjectOptions::default()).text
}

fn skill_summary_block(s: &MatchedSkill) -> String {
    let headings: Vec<&str> = s
        .body
        .lines()
        .filter(|line| line.starts_with("## ") || line.starts_with("### "))
        .collect();
    let mut out = format!(
        "### skill: {} (summary)\n\n{}\n\n",
        s.name,
        with_properties(&s.properties, &s.description)
    );
    out.push_str("Sections:\n");
    for heading in headings {
        out.push_str(&format!("- {}\n", heading.trim_start_matches('#').trim()));
    }
    out.push_str(&format!(
        "\nThis summary is not the workflow. Call `ax_skill(\"{}\")` for the full body before work that this skill governs.\n\n",
        s.name
    ));
    out
}

fn rule_block(r: &MatchedRule) -> String {
    format!(
        "### [{}] {}\n\n{}\n\n",
        r.level,
        r.id,
        with_properties(&r.properties, &r.body)
    )
}

const DIRECTIVE_HEADINGS: &[&str] = &[
    "rules",
    "required",
    "required workflow",
    "required on write",
    "required mapping",
    "hard rules",
    "forbidden",
    "scope",
    "thresholds",
    "correct shape",
];

fn is_directive_heading(line: &str) -> bool {
    let Some(title) = line.strip_prefix("## ") else {
        return false;
    };
    let title = title.trim().trim_end_matches(':').to_ascii_lowercase();
    DIRECTIVE_HEADINGS.contains(&title.as_str())
}

/// The ABSOLUTE line plus directive sections; falls back to the first three non-empty lines.
fn compact_rule_body(body: &str) -> String {
    let mut kept: Vec<&str> = body
        .lines()
        .filter(|l| l.trim_start().starts_with("> **ABSOLUTE**"))
        .collect();
    let mut in_section = false;
    let mut found_section = false;
    for line in body.lines() {
        if line.starts_with("## ") {
            in_section = is_directive_heading(line);
            found_section |= in_section;
        } else if line.starts_with("# ") {
            in_section = false;
        }
        if in_section && !line.trim().is_empty() {
            kept.push(line);
        }
    }
    if !found_section && kept.is_empty() {
        kept = body
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(3)
            .collect();
    }
    kept.join("\n")
}

fn compact_rule_block(r: &MatchedRule) -> String {
    format!(
        "### [{}] {}\n\n{}\n\n_Full rule: `ax_rules` (\"{}\")._\n\n",
        r.level,
        r.id,
        with_properties(&r.properties, &compact_rule_body(&r.body)),
        r.id
    )
}

fn skill_block(s: &MatchedSkill) -> String {
    format!(
        "### skill: {}\n\n{}\n\n{}\n\n",
        s.name,
        s.description,
        with_properties(&s.properties, &s.body)
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
            reason: if always {
                "alwaysApply".into()
            } else {
                "trigger:x".into()
            },
            always_apply: always,
            body: body.into(),
            properties: PolicyProperties::new(),
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
        assert!(
            inject.contains(&"C".repeat(5_000)),
            "CRITICAL contextual rules must remain complete"
        );
        assert!(!inject.contains("Contextual rules truncated"));
    }

    fn skill(name: &str, body: &str) -> MatchedSkill {
        skill_with(name, body, false)
    }

    fn skill_with(name: &str, body: &str, always: bool) -> MatchedSkill {
        MatchedSkill {
            name: name.into(),
            score: if always { 100 } else { 25 },
            reason: if always {
                "alwaysApply".into()
            } else {
                "trigger:x".into()
            },
            description: "desc".into(),
            body: body.into(),
            always_apply: always,
            properties: PolicyProperties::new(),
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

    fn delivered_of(rules: &[MatchedRule], skills: &[MatchedSkill]) -> HashMap<String, u64> {
        format_inject_block_with(rules, skills, 16_000, InjectOptions::default())
            .delivered
            .into_iter()
            .collect()
    }

    #[test]
    fn first_delivery_sends_every_always_apply_body() {
        let rules = vec![rule("english-only", true, "WRITE ENGLISH")];
        let out = format_inject_block_with(&rules, &[], 16_000, InjectOptions::default());
        assert!(out.text.contains("WRITE ENGLISH"));
        assert_eq!(
            out.delivered,
            vec![(rule_key("english-only"), content_hash("WRITE ENGLISH"))]
        );
    }

    #[test]
    fn unchanged_always_rule_is_listed_not_resent() {
        let rules = vec![
            rule("english-only", true, "WRITE ENGLISH"),
            rule("utf8", true, "NO BOM"),
        ];
        let delivered = delivered_of(&rules, &[]);
        let opts = InjectOptions {
            delivered: Some(&delivered),
            ..Default::default()
        };
        let out = format_inject_block_with(&rules, &[], 16_000, opts);
        assert!(!out.text.contains("WRITE ENGLISH"), "{}", out.text);
        assert!(!out.text.contains("NO BOM"), "{}", out.text);
        assert!(out.text.contains("english-only"));
        assert!(out.text.contains("utf8"));
        assert!(
            out.text.contains("ax_rules"),
            "recovery pointer missing:\n{}",
            out.text
        );
        assert!(out.delivered.is_empty());
    }

    #[test]
    fn changed_rule_body_is_resent() {
        let delivered = delivered_of(&[rule("english-only", true, "OLD TEXT")], &[]);
        let opts = InjectOptions {
            delivered: Some(&delivered),
            ..Default::default()
        };
        let out =
            format_inject_block_with(&[rule("english-only", true, "NEW TEXT")], &[], 16_000, opts);
        assert!(out.text.contains("NEW TEXT"));
        assert_eq!(
            out.delivered,
            vec![(rule_key("english-only"), content_hash("NEW TEXT"))]
        );
    }

    #[test]
    fn client_loaded_rule_is_listed_not_sent() {
        let loaded: HashSet<String> = [rule_key("mcp-shape")].into_iter().collect();
        let opts = InjectOptions {
            client_loaded: Some(&loaded),
            ..Default::default()
        };
        let rules = vec![
            rule("mcp-shape", true, "SHAPE BODY"),
            rule("english-only", true, "WRITE ENGLISH"),
        ];
        let out = format_inject_block_with(&rules, &[], 16_000, opts);
        assert!(!out.text.contains("SHAPE BODY"));
        assert!(out.text.contains("mcp-shape"));
        assert!(out.text.contains("WRITE ENGLISH"));
    }

    #[test]
    fn large_always_skill_is_summarized_with_ax_skill_pointer() {
        let body = format!(
            "# Old Coder\n\n## The Loop\n\n{}\n\n### 1. SPEC\n\n{}",
            "L".repeat(3_000),
            "S".repeat(3_000)
        );
        let skills = vec![skill_with("old-coder", &body, true)];
        let opts = InjectOptions {
            skill_inline_chars: Some(1_000),
            ..Default::default()
        };
        let out = format_inject_block_with(&[], &skills, 200, opts);
        assert!(out.text.contains("old-coder"));
        assert!(out.text.contains("The Loop"));
        assert!(out.text.contains("1. SPEC"));
        assert!(out.text.contains("ax_skill"));
        assert!(
            !out.text.contains(&"L".repeat(80)),
            "full body leaked:\n{}",
            out.text
        );
        assert_eq!(
            out.delivered,
            vec![(skill_key("old-coder"), content_hash(&body))]
        );
    }

    #[test]
    fn small_always_skill_stays_inline() {
        let skills = vec![skill_with("tiny", "SMALL SKILL BODY", true)];
        let opts = InjectOptions {
            skill_inline_chars: Some(1_000),
            ..Default::default()
        };
        let out = format_inject_block_with(&[], &skills, 16_000, opts);
        assert!(out.text.contains("SMALL SKILL BODY"));
    }

    fn long_rule_body() -> String {
        format!(
            "# Title\n\n> **ABSOLUTE**: ALWAYS UTF-8.\n\nINTRO PARAGRAPH.\n\n## Why\n\n{}\n\n## Forbidden\n\n- NO UTF-16\n- NO BOM\n\n## Example\n\n```\nEXAMPLE CODE\n```\n",
            "WHY TEXT ".repeat(200)
        )
    }

    #[test]
    fn long_rule_is_sent_compact_with_directive_sections() {
        let body = long_rule_body();
        let opts = InjectOptions {
            rule_inline_chars: Some(600),
            ..Default::default()
        };
        let out = format_inject_block_with(&[rule("utf8", true, &body)], &[], 16_000, opts);
        assert!(out.text.contains("utf8"), "{}", out.text);
        assert!(out.text.contains("ALWAYS UTF-8"), "{}", out.text);
        assert!(
            out.text.contains("NO UTF-16") && out.text.contains("NO BOM"),
            "{}",
            out.text
        );
        assert!(out.text.contains("ax_rules"), "{}", out.text);
        assert!(!out.text.contains("WHY TEXT"), "{}", out.text);
        assert!(!out.text.contains("EXAMPLE CODE"), "{}", out.text);
        assert!(!out.text.contains("INTRO PARAGRAPH"), "{}", out.text);
        assert_eq!(out.delivered, vec![(rule_key("utf8"), content_hash(&body))]);
    }

    #[test]
    fn long_rule_without_directive_sections_keeps_first_lines() {
        let body = format!(
            "Line one.\nLine two.\nLine three.\nLine four.\n\n## Background\n\n{}",
            "FILLER ".repeat(200)
        );
        let opts = InjectOptions {
            rule_inline_chars: Some(600),
            ..Default::default()
        };
        let out = format_inject_block_with(&[rule("plain", true, &body)], &[], 16_000, opts);
        assert!(
            out.text.contains("Line one.") && out.text.contains("Line three."),
            "{}",
            out.text
        );
        assert!(
            !out.text.contains("Line four.") && !out.text.contains("FILLER"),
            "{}",
            out.text
        );
        assert!(out.text.contains("ax_rules"), "{}", out.text);
    }

    #[test]
    fn out_of_scope_always_rule_is_listed_by_id_only() {
        let mut ui = rule("wcag-contrast", true, "UI CONTRAST BODY");
        ui.reason = format!("alwaysApply, {}", crate::matcher::OUT_OF_SCOPE);
        let rules = vec![ui, rule("english-only", true, "WRITE ENGLISH")];
        let out = format_inject_block_with(&rules, &[], 16_000, InjectOptions::default());
        assert!(!out.text.contains("UI CONTRAST BODY"), "{}", out.text);
        assert!(out.text.contains("wcag-contrast"), "{}", out.text);
        assert!(out.text.contains("WRITE ENGLISH"), "{}", out.text);
        assert_eq!(
            out.delivered,
            vec![(rule_key("english-only"), content_hash("WRITE ENGLISH"))]
        );
    }

    #[test]
    fn short_rule_stays_full_with_rule_limit() {
        let opts = InjectOptions {
            rule_inline_chars: Some(600),
            ..Default::default()
        };
        let out = format_inject_block_with(
            &[rule("english-only", true, "WRITE ENGLISH")],
            &[],
            16_000,
            opts,
        );
        assert!(out.text.contains("WRITE ENGLISH"));
        assert!(!out.text.contains("Full rule"), "{}", out.text);
    }

    #[test]
    fn unchanged_skill_is_listed_not_resent() {
        let skills = vec![skill_with("tiny", "SMALL SKILL BODY", true)];
        let delivered = delivered_of(&[], &skills);
        let opts = InjectOptions {
            delivered: Some(&delivered),
            ..Default::default()
        };
        let out = format_inject_block_with(&[], &skills, 16_000, opts);
        assert!(!out.text.contains("SMALL SKILL BODY"));
        assert!(out.text.contains("tiny"));
    }

    #[test]
    fn default_options_match_legacy_formatter() {
        let rules = vec![rule("always-a", true, "AAAA"), rule("ctx-b", false, "BBBB")];
        let skills = vec![
            skill_with("old-coder", &"G".repeat(8_000), true),
            skill("ctx", "CCCC"),
        ];
        let legacy = format_inject_block(&rules, &skills, 16_000);
        let with = format_inject_block_with(&rules, &skills, 16_000, InjectOptions::default()).text;
        assert_eq!(legacy, with);
    }

    #[test]
    fn extra_properties_are_sent_for_rules_and_skills() {
        let mut rule = rule("owner-rule", true, "Do the thing.");
        rule.properties
            .insert("owner".into(), serde_json::json!("platform"));
        rule.properties
            .insert("files".into(), serde_json::json!(["src/a.rs"]));
        let mut skill = skill("astro-review", "Review Astro.");
        skill
            .properties
            .insert("kind".into(), serde_json::json!("review"));
        let inject = format_inject_block(&[rule], &[skill], 20_000);
        assert!(inject.contains("owner: platform"), "{inject}");
        assert!(inject.contains("src/a.rs"), "{inject}");
        assert!(inject.contains("kind: review"), "{inject}");
        assert!(inject.contains("Do the thing."), "{inject}");
        assert!(inject.contains("Review Astro."), "{inject}");
        assert_ne!(
            delivered_body_hash("Do the thing.", &PolicyProperties::new()),
            delivered_body_hash(
                "Do the thing.",
                &PolicyProperties::from([("owner".into(), serde_json::json!("platform"))])
            )
        );
    }

    #[test]
    fn content_hash_is_stable() {
        assert_eq!(content_hash(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(content_hash("a"), 0xaf63_dc4c_8601_ec8c);
        assert_ne!(content_hash("rule A"), content_hash("rule B"));
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
