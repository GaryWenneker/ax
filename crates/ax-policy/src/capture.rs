//! Deterministic directive detection — propose policy rules from user prompts.

use std::path::Path;

use crate::parse::serialize_rule;
use crate::types::RuleFrontmatter;

/// One question the agent should ask the user before saving a captured rule.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureInterviewQuestion {
    pub field: String,
    pub question: String,
    pub current: String,
    pub options: Vec<String>,
    pub required: bool,
}

/// Result of scanning a prompt for durable directive language.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureProposal {
    pub detected: bool,
    pub confidence: String,
    pub suggested_id: String,
    pub frontmatter: RuleFrontmatter,
    pub body: String,
    pub preview_path: String,
    pub preview: String,
    pub questions: Vec<CaptureInterviewQuestion>,
    pub interview_instruction: String,
}

const DIRECTIVE_PREFIXES: &[&str] = &[
    "je moet",
    "jij moet",
    "u moet",
    "gebruik altijd",
    "gebruik nooit",
    "altijd ",
    "nooit ",
    "voortaan ",
    "vanaf nu ",
    "you must",
    "you should always",
    "always use",
    "always ",
    "never use",
    "never ",
    "don't ",
    "do not ",
    "from now on",
];

const EXPLICIT_MARKERS: &[&str] = &["@rule", "#rule"];

const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "your", "have", "will", "when",
    "what", "how", "where", "which", "must", "moet", "altijd", "nooit", "always", "never",
    "gebruik", "use", "een", "het", "de", "van", "voor", "met", "die", "dat", "dit",
];

/// Returns true when the prompt contains directive language worth proposing as a rule.
pub fn detect_directive(prompt: &str) -> bool {
    propose_rule_from_prompt(prompt, &[]).detected
}

/// Build a rule proposal from prompt text (no disk write).
pub fn propose_rule_from_prompt(prompt: &str, open_files: &[String]) -> CaptureProposal {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return empty_proposal();
    }

    let (confidence, body) = match extract_directive_body(trimmed) {
        Some((c, b)) => (c, b),
        None => return empty_proposal(),
    };

    let body = body.trim().to_string();
    if body.len() < 8 {
        return empty_proposal();
    }

    let suggested_id = slug_from_text(&body);
    let triggers = extract_triggers(&body, trimmed);
    let globs = globs_from_files(open_files);

    let frontmatter = RuleFrontmatter {
        id: suggested_id.clone(),
        level: "WARNING".into(),
        always_apply: false,
        globs,
        triggers,
        tags: vec!["captured".into()],
        priority: 60,
    };

    let preview_path = format!(".ax/policy/rules/{suggested_id}.mdc");
    let preview = serialize_rule(&frontmatter, &format_rule_body(&body));
    let body = format_rule_body(&body);

    let mut proposal = CaptureProposal {
        detected: true,
        confidence: confidence.to_string(),
        suggested_id: suggested_id.clone(),
        frontmatter,
        body,
        preview_path,
        preview,
        questions: vec![],
        interview_instruction: String::new(),
    };
    proposal.questions = capture_interview_questions(&proposal);
    proposal.interview_instruction = interview_instruction_text();
    proposal
}

/// Questions the agent should ask the user to refine rule options before save.
pub fn capture_interview_questions(proposal: &CaptureProposal) -> Vec<CaptureInterviewQuestion> {
    let fm = &proposal.frontmatter;
    vec![
        CaptureInterviewQuestion {
            field: "confirm".into(),
            question: "Save this as a durable team rule? (yes/no)".into(),
            current: "pending".into(),
            options: vec!["yes".into(), "no".into()],
            required: true,
        },
        CaptureInterviewQuestion {
            field: "id".into(),
            question: format!(
                "Rule id (slug, used in DB): keep \"{}\" or choose another?",
                proposal.suggested_id
            ),
            current: proposal.suggested_id.clone(),
            options: vec![],
            required: true,
        },
        CaptureInterviewQuestion {
            field: "level".into(),
            question: "Severity level: when should agents treat this as binding?".into(),
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
            question: "Which keywords should activate this rule? (comma-separated)".into(),
            current: fm.triggers.join(", "),
            options: vec![],
            required: false,
        },
        CaptureInterviewQuestion {
            field: "globs".into(),
            question: "Limit to specific file patterns? (empty = all files, e.g. **/*.tsx)".into(),
            current: fm.globs.join(", "),
            options: vec![],
            required: false,
        },
        CaptureInterviewQuestion {
            field: "priority".into(),
            question: "Priority 0–100 (higher = earlier in inject). Default 60.".into(),
            current: fm.priority.to_string(),
            options: vec!["40".into(), "60".into(), "80".into(), "100".into()],
            required: false,
        },
        CaptureInterviewQuestion {
            field: "tags".into(),
            question: "Tags for categorization (comma-separated, e.g. captured, frontend)".into(),
            current: fm.tags.join(", "),
            options: vec![],
            required: false,
        },
        CaptureInterviewQuestion {
            field: "body".into(),
            question: "Refine the rule body text? (show preview; user may edit wording)".into(),
            current: "(see preview)".into(),
            options: vec![],
            required: false,
        },
    ]
}

pub fn interview_instruction_text() -> String {
    "Ask the user each question in plain language before save. Apply their answers to rule.frontmatter and rule.body. Save only after explicit yes — persisted to ax.db in database mode (not a disk-only file).".into()
}

/// Apply unique id resolution and refresh preview fields.
pub fn finalize_proposal(mut proposal: CaptureProposal, existing_ids: &[String]) -> CaptureProposal {
    if !proposal.detected {
        return proposal;
    }
    let unique_id = resolve_unique_id(&proposal.suggested_id, existing_ids);
    if unique_id != proposal.suggested_id {
        proposal.frontmatter.id = unique_id.clone();
        proposal.suggested_id = unique_id.clone();
        proposal.preview_path = format!(".ax/policy/rules/{unique_id}.mdc");
        proposal.preview = crate::parse::serialize_rule(&proposal.frontmatter, &proposal.body);
    }
    proposal.questions = capture_interview_questions(&proposal);
    proposal
}

/// Resolve a unique rule id against existing ids (suffix -2, -3, …).
pub fn resolve_unique_id(base_id: &str, existing_ids: &[String]) -> String {
    if !existing_ids.iter().any(|id| id == base_id) {
        return base_id.to_string();
    }
    for n in 2..100 {
        let candidate = format!("{base_id}-{n}");
        if !existing_ids.iter().any(|id| id == &candidate) {
            return candidate;
        }
    }
    format!("{base_id}-{}", existing_ids.len() + 1)
}

fn empty_proposal() -> CaptureProposal {
    CaptureProposal {
        detected: false,
        confidence: String::new(),
        suggested_id: String::new(),
        frontmatter: RuleFrontmatter {
            id: String::new(),
            level: "WARNING".into(),
            always_apply: false,
            globs: vec![],
            triggers: vec![],
            tags: vec![],
            priority: 60,
        },
        body: String::new(),
        preview_path: String::new(),
        preview: String::new(),
        questions: vec![],
        interview_instruction: String::new(),
    }
}

fn extract_directive_body(prompt: &str) -> Option<(&'static str, String)> {
    let lower = prompt.to_lowercase();

    for marker in EXPLICIT_MARKERS {
        if let Some(pos) = lower.find(marker) {
            let rest = prompt[pos + marker.len()..].trim_start_matches([':', ' ', '-']);
            if rest.len() >= 8 {
                return Some(("high", rest.to_string()));
            }
        }
    }

    for prefix in DIRECTIVE_PREFIXES {
        if let Some(pos) = lower.find(prefix) {
            let rest = prompt[pos + prefix.len()..].trim();
            if rest.len() >= 8 {
                let conf = if prefix.contains("moet") || prefix.contains("must") {
                    "high"
                } else {
                    "medium"
                };
                return Some((conf, rest.to_string()));
            }
        }
    }

    None
}

fn format_rule_body(text: &str) -> String {
    let title = title_from_body(text);
    format!("# {title}\n\nCaptured from user directive.\n\n{text}")
}

fn title_from_body(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().take(8).collect();
    if words.is_empty() {
        return "Captured rule".into();
    }
    let mut title = words.join(" ");
    if title.len() > 72 {
        title.truncate(72);
        title = title.trim_end().to_string();
    }
    title
}

fn slug_from_text(text: &str) -> String {
    let slug: String = text
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else {
                '-'
            }
        })
        .collect();

    let mut parts: Vec<&str> = slug
        .split('-')
        .filter(|p| !p.is_empty() && p.len() > 1)
        .take(6)
        .collect();

    if parts.is_empty() {
        parts.push("captured-rule");
    }

    let mut id = parts.join("-");
    if id.len() > 48 {
        id.truncate(48);
        id = id.trim_end_matches('-').to_string();
    }
    id
}

fn extract_triggers(body: &str, full_prompt: &str) -> Vec<String> {
    let mut triggers = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for text in [body, full_prompt] {
        for word in text.split(|c: char| !c.is_alphanumeric() && c != '-') {
            let w = word.trim().to_lowercase();
            if w.len() <= 3 || STOP_WORDS.contains(&w.as_str()) {
                continue;
            }
            if seen.insert(w.clone()) {
                triggers.push(w);
            }
            if triggers.len() >= 8 {
                return triggers;
            }
        }
    }

    if triggers.len() < 3 {
        for word in body.split_whitespace().take(12) {
            let w = word
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            if w.len() > 3 && !STOP_WORDS.contains(&w.as_str()) && seen.insert(w.clone()) {
                triggers.push(w);
            }
            if triggers.len() >= 3 {
                break;
            }
        }
    }

    triggers
}

fn globs_from_files(open_files: &[String]) -> Vec<String> {
    if open_files.is_empty() {
        return vec![];
    }
    let mut globs = Vec::new();
    for f in open_files.iter().take(3) {
        let p = f.replace('\\', "/");
        if p.contains('*') {
            globs.push(p);
        } else if let Some(ext) = Path::new(&p).extension().and_then(|e| e.to_str()) {
            globs.push(format!("**/*.{ext}"));
        }
    }
    globs.sort();
    globs.dedup();
    globs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_dutch_directive() {
        let p = propose_rule_from_prompt("je moet altijd Tailwind gebruiken voor de UI", &[]);
        assert!(p.detected);
        assert_eq!(p.confidence, "high");
        assert!(p.frontmatter.triggers.iter().any(|t| t.contains("tailwind")));
        assert!(p.preview.contains("---"));
    }

    #[test]
    fn detects_english_directive() {
        let p = propose_rule_from_prompt("You must always use dark mode in the web UI", &[]);
        assert!(p.detected);
        assert!(p.suggested_id.contains("dark"));
    }

    #[test]
    fn ignores_question_prompt() {
        let p = propose_rule_from_prompt("Hoe werkt de policy matcher?", &[]);
        assert!(!p.detected);
    }

    #[test]
    fn explicit_rule_marker() {
        let p = propose_rule_from_prompt("@rule: gebruik alleen Engels in commit messages", &[]);
        assert!(p.detected);
        assert_eq!(p.confidence, "high");
    }

    #[test]
    fn resolve_unique_id_suffix() {
        let ids = vec!["foo".into(), "foo-2".into()];
        assert_eq!(resolve_unique_id("foo", &ids), "foo-3");
    }

    #[test]
    fn propose_includes_interview_questions() {
        let p = propose_rule_from_prompt("je moet altijd Tailwind gebruiken", &[]);
        assert!(p.detected);
        assert!(!p.questions.is_empty());
        assert!(p.questions.iter().any(|q| q.field == "level"));
        assert!(p.questions.iter().any(|q| q.field == "alwaysApply"));
        assert!(!p.interview_instruction.is_empty());
    }

    #[test]
    fn globs_from_open_files() {
        let p = propose_rule_from_prompt(
            "je moet altijd types gebruiken",
            &["src/App.tsx".into()],
        );
        assert!(p.frontmatter.globs.iter().any(|g| g.contains("tsx")));
    }
}
