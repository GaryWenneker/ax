//! Optional authored presentation. Canonical policy and source text are never rewritten.
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum PreflightTextStyle {
    #[default]
    Standard,
    SteInspired,
}

impl PreflightTextStyle {
    pub(crate) fn from_params(params: &Value) -> Result<Self, String> {
        match params.get("textStyle") {
            None => Ok(Self::Standard),
            Some(Value::String(style)) if style == "standard" => Ok(Self::Standard),
            Some(Value::String(style)) if style == "ste-inspired" => Ok(Self::SteInspired),
            Some(_) => Err("textStyle must be \"standard\" or \"ste-inspired\"".into()),
        }
    }

    /// Validate before a request can reset or acknowledge delivery state.
    pub(crate) fn validate_tool(name: &str, params: &Value) -> Result<(), String> {
        if name == "ax_preflight" {
            Self::from_params(params)?;
        }
        Ok(())
    }

    pub(crate) fn add_metadata(self, value: &mut Value) {
        if self == Self::SteInspired {
            value["presentation"] = json!({
                "textStyle": "ste-inspired", "profile": "ax-ste-inspired-1"
            });
        }
    }

    pub(crate) fn guidance(self) -> &'static str {
        match self {
            Self::Standard => "",
            Self::SteInspired => "Write explanations in clear technical English. Use one instruction per sentence. State conditions and exceptions. Keep technical names, code, quotations and uncertainty unchanged. Apply this style to the current turn only.\n\n",
        }
    }

    pub(crate) fn policy_markdown(self, block: &str) -> String {
        crate::smart_output::policy_markdown_with_instruction(
            block,
            match self {
                Self::Standard => "Apply matched rules and skills before editing.",
                Self::SteInspired => "Apply matched rules before you edit files. Follow each matched skill's workflow before you edit files.",
            },
        )
    }

    pub(crate) fn cache_help(self, standard: &str) -> &str {
        match self {
            Self::Standard => standard,
            Self::SteInspired => "## Context tools\n\nUse `ax_expand` with a cache ID and optional character offset for large replies. Use `ax_node` for one symbol. After context loss, set `fresh: true` to repeat graph queries. Record decisions with `ax_session`. Pass `known_context` only if you retain that snapshot. After compaction or reset, load missing rules with `ax_rules` and skills with `ax_skill`. Check that you still hold earlier content before you use it.",
        }
    }

    pub(crate) fn history_hint(self) -> &'static str {
        match self {
            Self::Standard => "<ax_history_hint>This asks when something changed: call ax_history with the file, symbol, or topic. It lists dated agent turns (prompt, outcome, files, commits) and git commits.</ax_history_hint>",
            Self::SteInspired => "<ax_history_hint>To find when something changed, call ax_history with its file, symbol or topic. The result lists dated agent turns and git commits. Each turn includes its prompt, outcome, files and commits.</ax_history_hint>",
        }
    }

    pub(crate) fn capture_hint(self) -> &'static str {
        match self {
            Self::Standard => "<ax_capture_hint>Directive detected — a ready rule proposal is in captureProposal. Ask the user captureProposal.questions, then call ax_policy_capture({ action: \"save\", rule }) after they confirm.</ax_capture_hint>",
            Self::SteInspired => "<ax_capture_hint>A directive was detected. Read the proposed rule in captureProposal. Ask the user captureProposal.questions. After the user confirms, call ax_policy_capture({ action: \"save\", rule }).</ax_capture_hint>",
        }
    }

    pub(crate) fn instruction(self, directive: bool) -> String {
        let mut text = match self {
            Self::Standard => "Apply CRITICAL rules before editing. If a skill matched, follow its workflow.",
            Self::SteInspired => "Apply CRITICAL rules before you edit files. If a skill matches, follow its workflow.",
        }.to_string();
        if directive {
            text.push_str(match self {
                Self::Standard => " DIRECTIVE DETECTED — captureProposal holds a ready rule. Ask the user the questions in captureProposal.questions, then call ax_policy_capture(action=\"save\", rule) after they say yes. This persists even in a project with no prior policy.",
                Self::SteInspired => " A directive was detected. Read the proposed rule in captureProposal. Ask the user captureProposal.questions. After the user confirms, call ax_policy_capture(action=\"save\", rule). The rule is saved even if the project has no prior policy.",
            });
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arbitrary_utf8_policy_bodies_survive_the_authored_envelope() {
        for body in [
            "",
            "unless",
            "café Δ 🪓",
            "<ax_policy>quoted</ax_policy>",
            "\n\n",
            "```rust\nlet x = \"must not\";\n```",
        ] {
            for repeat in 1..=16 {
                let body = body.repeat(repeat);
                let block = format!("<ax_policy note=\"matched\">\n{body}</ax_policy>\n");
                for style in [
                    PreflightTextStyle::Standard,
                    PreflightTextStyle::SteInspired,
                ] {
                    let rendered = style.policy_markdown(&block);
                    assert!(rendered.ends_with(&body));
                    assert!(!rendered.is_empty());
                    assert_eq!(style.policy_markdown(&block), rendered);
                }
            }
        }
    }

    #[test]
    fn unrecognized_envelopes_are_preserved() {
        for body in [
            "plain source",
            "<ax_policy>source</ax_policy>",
            "<ax_policy note=\"unfinished\">\nsource",
        ] {
            assert_eq!(PreflightTextStyle::SteInspired.policy_markdown(body), body);
        }
    }

    #[test]
    fn style_validation_is_strict_and_local_to_preflight() {
        assert_eq!(
            PreflightTextStyle::from_params(&json!({})).unwrap(),
            PreflightTextStyle::Standard
        );
        assert_eq!(
            PreflightTextStyle::from_params(&json!({"textStyle":"standard"})).unwrap(),
            PreflightTextStyle::Standard
        );
        assert_eq!(
            PreflightTextStyle::from_params(&json!({"textStyle":"ste-inspired"})).unwrap(),
            PreflightTextStyle::SteInspired
        );
        for invalid in [
            json!(null),
            json!(true),
            json!(0),
            json!([]),
            json!({}),
            json!("STE"),
            json!(" standard "),
        ] {
            let args = json!({"textStyle":invalid});
            assert!(PreflightTextStyle::validate_tool("ax_preflight", &args).is_err());
            assert!(PreflightTextStyle::validate_tool("ax_status", &args).is_ok());
        }
    }
}
