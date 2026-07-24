//! Verbose MCP tracing for Cursor Output (stderr).
//!
//! When enabled via `AX_MCP_VERBOSE` or `.ax/ship.toml` `[ui].verbose_mcp`,
//! tool-call inbound/outbound/enrichment lines are collected and delivered to
//! the stdio proxy as `{"type":"ax_log",...}` side-channel lines (never mixed
//! into agent-facing MCP payloads).

use std::cell::RefCell;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

pub const TRACE_FIELD_MAX: usize = 3_072;
/// Keep outbound previews short so the log stays one-line-per-event (no dump of explore bodies).
const PREVIEW_LOG_MAX: usize = 160;
const PREFIX: &str = "[ax-mcp]";

tokio::task_local! {
    static VERBOSE_TRACE: RefCell<Vec<String>>;
}

#[derive(Debug, Deserialize, Default)]
struct ShipUiFile {
    #[serde(default)]
    ui: UiVerboseOnly,
}

#[derive(Debug, Deserialize, Default)]
struct UiVerboseOnly {
    #[serde(default)]
    verbose_mcp: bool,
}

/// True when env override or project `[ui].verbose_mcp` is set.
pub fn verbose_enabled(project_root: Option<&Path>) -> bool {
    if env_flag_truthy("AX_MCP_VERBOSE") {
        return true;
    }
    let Some(root) = project_root else {
        return false;
    };
    read_ship_verbose_mcp(root)
}

fn env_flag_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
}

fn read_ship_verbose_mcp(project_root: &Path) -> bool {
    let root = strip_verbatim_prefix(project_root);
    let path = root.join(".ax").join("ship.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return false;
    };
    toml::from_str::<ShipUiFile>(&text)
        .map(|c| c.ui.verbose_mcp)
        .unwrap_or(false)
}

/// Windows daemon paths often use `\\?\C:\...` — normalize before joining.
fn strip_verbatim_prefix(path: &Path) -> std::path::PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        return std::path::PathBuf::from(rest);
    }
    path.to_path_buf()
}

/// Run `fut` with an empty per-task verbose buffer; returns `(result, lines)`.
pub async fn with_trace_buffer<F, T>(fut: F) -> (T, Vec<String>)
where
    F: std::future::Future<Output = T>,
{
    VERBOSE_TRACE
        .scope(RefCell::new(Vec::new()), async move {
            let out = fut.await;
            let lines = VERBOSE_TRACE.with(|buf| std::mem::take(&mut *buf.borrow_mut()));
            (out, lines)
        })
        .await
}

/// Append a prefixed single-line entry when a trace buffer is active.
/// When a Cursor session id is known (`AX_CURSOR_SESSION_ID` or
/// `~/.ax/active-cursor-session`), append `session=<id>` for audit correlation.
pub fn push_line(msg: impl AsRef<str>) {
    let _ = VERBOSE_TRACE.try_with(|buf| {
        let clean = sanitize_one_line(msg.as_ref());
        let line = match active_session_id() {
            Some(sid) => format!("{PREFIX} {clean} session={sid}"),
            None => format!("{PREFIX} {clean}"),
        };
        buf.borrow_mut().push(line);
    });
}

/// Active Cursor agent session id for verbose correlation (optional).
pub fn active_session_id() -> Option<String> {
    if let Ok(v) = std::env::var("AX_CURSOR_SESSION_ID") {
        let v = v.trim();
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }
    let path = dirs::home_dir()?.join(".ax").join("active-cursor-session");
    let text = std::fs::read_to_string(path).ok()?;
    let id = text.lines().next()?.trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

/// Collapse newlines so each event stays one log line (tail -f friendly).
fn sanitize_one_line(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\n' | '\r' => '⏎',
            '\t' => ' ',
            other => other,
        })
        .collect()
}

pub fn push_inbound(tool: &str, args: &Value) {
    let redacted = redact_value(args);
    let rendered = truncate_utf8(&redacted.to_string(), TRACE_FIELD_MAX);
    push_line(format!("inbound tool={tool} args={rendered}"));
}

pub fn push_internal(tool: &str, value: &Value) {
    let keys = value
        .as_object()
        .map(|o| {
            o.keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_else(|| "(non-object)".into());
    let inject_len = value
        .get("inject")
        .and_then(|v| v.as_str())
        .map(|s| s.len())
        .unwrap_or(0);
    let text_len = value
        .get("text")
        .and_then(|v| v.as_str())
        .map(|s| s.len())
        .unwrap_or(0);
    push_line(format!(
        "internal tool={tool} keys=[{keys}] inject_chars={inject_len} text_chars={text_len}"
    ));
}

pub fn push_outbound(tool: &str, wrapped: &Value, full_structured: bool, duration_ms: i64) {
    let text_chars = wrapped
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|part| part.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.len())
        .unwrap_or(0);
    let has_structured = wrapped.get("structuredContent").is_some();
    let preview = wrapped
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|part| part.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| truncate_utf8(s, PREVIEW_LOG_MAX))
        .unwrap_or_default();
    let mode = if full_structured { "full" } else { "lean" };
    push_line(format!(
        "outbound tool={tool} mode={mode} text_chars={text_chars} structured={has_structured} duration_ms={duration_ms}"
    ));
    if !preview.is_empty() {
        push_line(format!("outbound preview tool={tool} text={preview}"));
    }
}

pub fn push_error(tool: &str, msg: &str) {
    push_line(format!(
        "error tool={tool} message={}",
        truncate_utf8(msg, TRACE_FIELD_MAX)
    ));
}

/// Daemon → proxy side-channel line (not JSON-RPC).
pub fn format_ax_log_line(text: &str) -> String {
    serde_json::json!({ "type": "ax_log", "text": text }).to_string() + "\n"
}

/// MCP logging notification — forwarded on stdout so Cursor's MCP Output shows it.
pub fn format_mcp_log_notification(text: &str) -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notifications/message",
        "params": {
            "level": "info",
            "logger": "ax-mcp",
            "data": text,
        }
    })
    .to_string()
        + "\n"
}

/// Parse a daemon side-channel log line; returns the text when it is `ax_log`.
pub fn parse_ax_log_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.contains("\"ax_log\"") {
        return None;
    }
    let value: Value = serde_json::from_str(trimmed).ok()?;
    if value.get("type").and_then(|t| t.as_str()) != Some("ax_log") {
        return None;
    }
    value
        .get("text")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
}

pub fn emit_stderr(lines: &[String]) {
    for line in lines {
        eprintln!("{line}");
    }
}

/// Append traces to the active daily log under `<project>/.ax/` (falls back to `~/.ax/`).
pub fn append_verbose_log_file(lines: &[String], project_root: Option<&Path>) {
    ax_usage::append_verbose_log(lines, project_root);
}

/// Active daily log path for the project.
pub fn verbose_log_path(project_root: Option<&Path>) -> Option<std::path::PathBuf> {
    Some(ax_usage::current_log_path(project_root))
}

/// Deliver verbose lines on every path: stderr, log file, and (for callers) MCP notifs.
pub fn deliver_local(lines: &[String], project_root: Option<&Path>) {
    emit_stderr(lines);
    append_verbose_log_file(lines, project_root);
}

pub fn truncate_utf8(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[{} more bytes]", &s[..end], s.len().saturating_sub(end))
}

fn redact_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if key_looks_secret(k) {
                    out.insert(k.clone(), Value::String("***".into()));
                } else {
                    out.insert(k.clone(), redact_value(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(redact_value).collect()),
        other => other.clone(),
    }
}

fn key_looks_secret(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("token")
        || lower.contains("password")
        || lower.contains("secret")
        || lower.ends_with("_key")
        || lower.ends_with("apikey")
        || lower.contains("api_key")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn env_flag_accepts_truthy() {
        // Direct unit coverage of parser helpers (no env mutation races).
        assert!(matches_truthy("1"));
        assert!(matches_truthy("true"));
        assert!(matches_truthy("YES"));
        assert!(!matches_truthy("0"));
        assert!(!matches_truthy("no"));
    }

    fn matches_truthy(v: &str) -> bool {
        let v = v.trim();
        v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes")
    }

    #[test]
    fn redact_hides_secret_keys() {
        let raw = json!({
            "prompt": "hello",
            "api_key": "sk-secret",
            "nested": { "authToken": "abc", "ok": 1 }
        });
        let redacted = redact_value(&raw);
        assert_eq!(redacted["api_key"], "***");
        assert_eq!(redacted["nested"]["authToken"], "***");
        assert_eq!(redacted["nested"]["ok"], 1);
        assert_eq!(redacted["prompt"], "hello");
    }

    #[test]
    fn truncate_marks_overflow() {
        let s = "a".repeat(100);
        let out = truncate_utf8(&s, 10);
        assert!(out.starts_with("aaaaaaaaaa"));
        assert!(out.contains("more bytes"));
    }

    #[test]
    fn sanitize_collapses_newlines() {
        assert_eq!(sanitize_one_line("a\nb\rc"), "a⏎b⏎c");
    }

    #[test]
    fn ax_log_roundtrip() {
        let line = format_ax_log_line("[ax-mcp] hello");
        assert_eq!(parse_ax_log_line(&line).as_deref(), Some("[ax-mcp] hello"));
        assert!(parse_ax_log_line(r#"{"jsonrpc":"2.0","id":1}"#).is_none());
        assert!(parse_ax_log_line(r#"{"type":"hello","pid":1}"#).is_none());
    }

    #[test]
    fn mcp_notification_contains_logger() {
        let line = format_mcp_log_notification("[ax-mcp] hello");
        assert!(line.contains("notifications/message"));
        assert!(line.contains("ax-mcp"));
        assert!(line.contains("[ax-mcp] hello"));
    }

    #[test]
    fn strip_verbatim_windows_prefix() {
        let p = strip_verbatim_prefix(std::path::Path::new(r"\\?\C:\gary\VfPf"));
        assert_eq!(p, std::path::PathBuf::from(r"C:\gary\VfPf"));
    }

    #[test]
    fn ship_toml_verbose_flag() {
        let dir = tempfile_dir();
        let ax = dir.join(".ax");
        std::fs::create_dir_all(&ax).unwrap();
        std::fs::write(
            ax.join("ship.toml"),
            "[ui]\nverbose_mcp = true\nshow_savings = true\n",
        )
        .unwrap();
        assert!(read_ship_verbose_mcp(&dir));
        std::fs::write(ax.join("ship.toml"), "[ui]\nshow_savings = true\n").unwrap();
        assert!(!read_ship_verbose_mcp(&dir));
    }

    fn tempfile_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ax-verbose-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
