//! CLI↔MCP Logging direction parity.
//!
//! MCP writes `[ax-mcp] inbound tool=…` / `outbound tool=…` via the MCP server
//! trace buffer. CLI must emit the **same body shape** so Logging classifies
//! Inbound / Outbound, but with the `[ax]` brand — never `[ax-mcp]` (do not
//! fake CLI traffic as MCP).

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::Value;

use crate::log_brand::format_ax_tagged;
use crate::mcp_verbose_log::append_verbose_log;

const ARGS_FIELD_MAX: usize = 3_072;
const PREVIEW_MAX: usize = 160;

/// RAII-ish span: begin writes inbound; finish_* writes outbound or error.
pub struct ToolTrace {
    root: Option<PathBuf>,
    tool: String,
    started: Instant,
    finished: bool,
}

impl ToolTrace {
    /// Log `inbound tool={tool} args={…}` and start the timer.
    pub fn begin(project_root: Option<&Path>, tool: &str, args: &Value) -> Self {
        log_tool_inbound(project_root, tool, args);
        Self {
            root: project_root.map(Path::to_path_buf),
            tool: tool.to_string(),
            started: Instant::now(),
            finished: false,
        }
    }

    pub fn finish_ok(mut self, text: &str) {
        let ms = self.started.elapsed().as_millis() as i64;
        log_tool_outbound(self.root.as_deref(), &self.tool, ms, text);
        self.finished = true;
    }

    pub fn finish_err(mut self, message: &str) {
        let ms = self.started.elapsed().as_millis() as i64;
        log_tool_error(self.root.as_deref(), &self.tool, message, ms);
        self.finished = true;
    }
}

impl Drop for ToolTrace {
    fn drop(&mut self) {
        if !self.finished {
            log_tool_error(
                self.root.as_deref(),
                &self.tool,
                "aborted",
                self.started.elapsed().as_millis() as i64,
            );
        }
    }
}

/// Append `inbound tool=… args=…` (CLI brand).
pub fn log_tool_inbound(project_root: Option<&Path>, tool: &str, args: &Value) {
    let rendered = truncate_chars(&compact_json(args), ARGS_FIELD_MAX);
    let line = format_ax_tagged(format!("inbound tool={tool} args={rendered}"));
    append_verbose_log(&[line], project_root);
}

/// Append `outbound tool=…` and optional preview (CLI brand).
pub fn log_tool_outbound(
    project_root: Option<&Path>,
    tool: &str,
    duration_ms: i64,
    text: &str,
) {
    let text_chars = text.chars().count();
    let mut lines = vec![format_ax_tagged(format!(
        "outbound tool={tool} mode=lean text_chars={text_chars} structured=false duration_ms={duration_ms}"
    ))];
    let preview = truncate_chars(text, PREVIEW_MAX);
    if !preview.is_empty() {
        lines.push(format_ax_tagged(format!(
            "outbound preview tool={tool} text={preview}"
        )));
    }
    append_verbose_log(&lines, project_root);
}

/// Append `error tool=…`.
pub fn log_tool_error(
    project_root: Option<&Path>,
    tool: &str,
    message: &str,
    duration_ms: i64,
) {
    let msg = truncate_chars(message, ARGS_FIELD_MAX);
    let line = format_ax_tagged(format!(
        "error tool={tool} message={msg} duration_ms={duration_ms}"
    ));
    append_verbose_log(&[line], project_root);
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".into())
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[test]
    fn inbound_outbound_use_ax_brand_not_mcp() {
        let dir = tempfile_dir();
        fs::create_dir_all(dir.join(".ax")).unwrap();

        let trace = ToolTrace::begin(Some(&dir), "ax_explore", &json!({"query": "logging"}));
        trace.finish_ok("Explore: logging");

        let text = read_today_log(&dir);
        assert!(
            text.contains("inbound tool=ax_explore"),
            "missing inbound: {text}"
        );
        assert!(
            text.contains("outbound tool=ax_explore"),
            "missing outbound: {text}"
        );
        assert!(
            text.contains("[ax]"),
            "expected CLI brand [ax]: {text}"
        );
        assert!(
            !text.contains("[ax-mcp]"),
            "must not fake MCP brand: {text}"
        );
        assert!(
            text.contains("args={\"query\":\"logging\"}")
                || text.contains("args={\"query\": \"logging\"}"),
            "args missing: {text}"
        );
    }

    #[test]
    fn skill_query_writes_inbound_outbound() {
        let dir = tempfile_dir();
        fs::create_dir_all(dir.join(".ax")).unwrap();

        let trace = ToolTrace::begin(Some(&dir), "ax_skill", &json!({"name": "startup"}));
        trace.finish_ok("skill body");

        let text = read_today_log(&dir);
        assert!(text.contains("inbound tool=ax_skill"));
        assert!(text.contains("outbound tool=ax_skill"));
        assert!(text.contains("startup"));
    }

    #[test]
    fn remember_recall_tools_write_direction_lines() {
        let dir = tempfile_dir();
        fs::create_dir_all(dir.join(".ax")).unwrap();

        ToolTrace::begin(Some(&dir), "ax_remember", &json!({"body": "note"}))
            .finish_ok("Remembered");
        ToolTrace::begin(Some(&dir), "ax_recall", &json!({"query": "note"}))
            .finish_ok("1 memory");

        let text = read_today_log(&dir);
        assert!(text.contains("inbound tool=ax_remember"));
        assert!(text.contains("inbound tool=ax_recall"));
        assert!(text.contains("outbound tool=ax_remember"));
        assert!(text.contains("outbound tool=ax_recall"));
    }

    #[test]
    fn finish_err_writes_error_kind_line() {
        let dir = tempfile_dir();
        fs::create_dir_all(dir.join(".ax")).unwrap();

        ToolTrace::begin(Some(&dir), "ax_node", &json!({"symbol": "x"})).finish_err("missing");

        let text = read_today_log(&dir);
        assert!(text.contains("inbound tool=ax_node"));
        assert!(text.contains("error tool=ax_node"));
        assert!(text.contains("message=missing"));
    }

    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ax-tool-trace-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn read_today_log(root: &Path) -> String {
        let ax = root.join(".ax");
        let mut bodies = String::new();
        for entry in fs::read_dir(&ax).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with("mcp-verbose-") && name.ends_with(".log") {
                bodies.push_str(&fs::read_to_string(&path).unwrap_or_default());
            }
        }
        bodies
    }
}
