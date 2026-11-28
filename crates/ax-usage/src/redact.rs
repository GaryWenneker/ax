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
/// an AWS access key id, or a long run of key characters. UUIDs and hex
/// commit ids (40 or 64 hex digits) are identifiers, not secrets.
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
    if is_uuid(token) || is_commit_id(token) {
        return false;
    }
    token.len() >= 32
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// `8-4-4-4-12` hex digits.
fn is_uuid(token: &str) -> bool {
    let groups: Vec<&str> = token.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(group, len)| group.len() == len && group.chars().all(|c| c.is_ascii_hexdigit()))
}

/// A lowercase SHA-1 or SHA-256 commit id.
fn is_commit_id(token: &str) -> bool {
    matches!(token.len(), 40 | 64)
        && token
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// A word that announces a credential in the next word or after a colon:
/// `Authorization:`, `Bearer`, `password:`, `api-key:`, `token:`.
fn secret_marker(word: &str) -> bool {
    let lower = word.to_ascii_lowercase().replace('-', "_");
    lower == "bearer" || (secret_key(&lower) && !lower.ends_with("tokens"))
}

/// A value after a marker that is a count, not a credential (`token: 1200`).
fn plain_number(value: &str) -> bool {
    value
        .chars()
        .all(|c| c.is_ascii_digit() || c == ',' || c == '.')
}

/// Redact one token. Returns true when the token is a marker whose value is
/// the next token (`password:` or `Bearer`).
fn redact_token(token: &str, after_marker: bool, out: &mut String) -> bool {
    if after_marker {
        // `Authorization: Bearer <value>`: the scheme stays, its value goes.
        if matches!(token.to_ascii_lowercase().as_str(), "bearer" | "basic") {
            out.push_str(token);
            return true;
        }
        if !plain_number(token) {
            out.push_str(REDACTED);
            return false;
        }
    }
    if let Some((key, value)) = token.split_once('=') {
        if !value.is_empty() && secret_key(key) {
            out.push_str(key);
            out.push('=');
            out.push_str(REDACTED);
            return false;
        }
    }
    if let Some((key, value)) = token.split_once(':') {
        if secret_marker(key) {
            if value.is_empty() {
                out.push_str(token);
                return true;
            }
            if !plain_number(value) {
                out.push_str(key);
                out.push(':');
                out.push_str(REDACTED);
                return false;
            }
        }
    }
    if looks_like_secret(token) {
        out.push_str(REDACTED);
        return false;
    }
    out.push_str(token);
    token.eq_ignore_ascii_case("bearer")
}

/// Replace secrets in `text` with `[redacted]`. Every separator between
/// tokens (spaces, tabs, line breaks) is kept as it was.
pub(crate) fn redact_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut after_marker = false;
    while !rest.is_empty() {
        let start = rest
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(rest.len());
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        if end > 0 {
            after_marker = redact_token(&rest[..end], after_marker, &mut out);
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
    fn identifiers_stay_and_secrets_after_a_marker_go() {
        assert_eq!(
            redact_text("resumed session 3f2b9c1e-8d7a-4b6c-9e5f-1a2b3c4d5e6f"),
            "resumed session 3f2b9c1e-8d7a-4b6c-9e5f-1a2b3c4d5e6f"
        );
        let sha = "0f0bd4da9c1e8d7a4b6c9e5f1a2b3c4d5e6f7a8b";
        assert_eq!(redact_text(&format!("at {sha}")), format!("at {sha}"));
        assert_eq!(
            redact_text("curl -H Authorization: Bearer sk_live_short"),
            "curl -H Authorization: Bearer [redacted]"
        );
        assert_eq!(redact_text("password: hunter2"), "password: [redacted]");
        assert_eq!(redact_text("password:hunter2"), "password:[redacted]");
        assert_eq!(redact_text("api-key: abc"), "api-key: [redacted]");
        assert_eq!(redact_text("bearer abc123"), "bearer [redacted]");
        assert_eq!(redact_text("tokens: 1200 used"), "tokens: 1200 used");
        assert_eq!(redact_text("token: 1200"), "token: 1200");
        assert_eq!(redact_text("note: fine"), "note: fine");
        assert_eq!(
            redact_text("https://example.com/a"),
            "https://example.com/a"
        );
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
