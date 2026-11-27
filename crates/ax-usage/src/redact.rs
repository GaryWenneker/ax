//! Secret redaction for text that ax-usage logs or stores: tool arguments and
//! previews in the verbose log, activity summaries, and turn prompts and
//! outcomes. Whitespace and line breaks are kept, so Markdown keeps its shape.

use serde_json::Value;

pub(crate) const REDACTED: &str = "[redacted]";

/// A key whose value is a credential: `password`, `token`, `secret`,
/// `api_key`, `apikey`, `*_key`, `authorization`.
pub(crate) fn secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("password")
        || lower.contains("passwd")
        || lower.contains("token")
        || lower.contains("secret")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.ends_with("_key")
        || lower == "authorization"
}

/// A single whitespace-free token that looks like a credential: an `sk-` key,
/// an AWS access key id, or a long run of key characters.
pub(crate) fn looks_like_secret(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    if lower.contains("password=") || lower.contains("token=") || lower.contains("api_key=") {
        return true;
    }
    if token.starts_with("sk-") && token.len() > 20 {
        return true;
    }
    if token.len() == 20
        && token.starts_with("AKIA")
        && token
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return true;
    }
    token.len() >= 32
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn redact_token(token: &str, out: &mut String) {
    if let Some((key, value)) = token.split_once('=') {
        if !value.is_empty() && secret_key(key) {
            out.push_str(key);
            out.push('=');
            out.push_str(REDACTED);
            return;
        }
    }
    if looks_like_secret(token) {
        out.push_str(REDACTED);
    } else {
        out.push_str(token);
    }
}

/// Replace secrets in `text` with `[redacted]`. Every separator between
/// tokens (spaces, tabs, line breaks) is kept as it was.
pub(crate) fn redact_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let start = rest
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(rest.len());
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if end > 0 {
            redact_token(&rest[..end], &mut out);
        }
        rest = &rest[end..];
    }
    out
}

/// Redact a JSON value: a string under a secret key becomes `[redacted]`, and
/// every other string is passed through [`redact_text`]. Numbers and booleans
/// stay, so a `max_tokens` limit is still visible.
pub(crate) fn redact_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| {
                    let redacted = if secret_key(key) && item.is_string() {
                        Value::String(REDACTED.into())
                    } else {
                        redact_json(item)
                    };
                    (key.clone(), redacted)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact_json).collect()),
        Value::String(text) => Value::String(redact_text(text)),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_keeps_its_line_breaks_and_loses_its_secrets() {
        let text = "# Plan\n\n- set TOKEN=abc123\n\tkey sk-abcdefghijklmnopqrstuvwxyz0123\n";
        assert_eq!(
            redact_text(text),
            "# Plan\n\n- set TOKEN=[redacted]\n\tkey [redacted]\n"
        );
        assert_eq!(redact_text("aws AKIAIOSFODNN7EXAMPLE"), "aws [redacted]");
        assert_eq!(
            redact_text("path crates/ax-usage/src/lib.rs"),
            "path crates/ax-usage/src/lib.rs"
        );
        assert_eq!(redact_text(""), "");
        assert_eq!(redact_text("  \n"), "  \n");
    }

    #[test]
    fn json_redacts_secret_keys_and_strings() {
        let value = json!({
            "api_key": "k",
            "nested": {"password": "p", "list": ["secret=s3", "fine"]},
            "max_tokens": 2
        });
        assert_eq!(
            redact_json(&value),
            json!({
                "api_key": "[redacted]",
                "nested": {"password": "[redacted]", "list": ["secret=[redacted]", "fine"]},
                "max_tokens": 2
            })
        );
    }
}
