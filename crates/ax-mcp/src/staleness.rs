//! Per-response staleness banners for MCP tool results.
//!
//! When the file watcher has queued edits that are not yet synced, annotate
//! tool responses that reference those paths so agents Read the live file
//! instead of trusting a stale graph entry.

use std::collections::BTreeSet;

use ax_types::PendingFile;
use serde_json::Value;

/// Annotate `text` with a banner and/or footer based on pending files.
pub fn annotate_staleness(text: &str, value: &Value, pending: &[PendingFile]) -> String {
    if pending.is_empty() {
        return text.to_string();
    }

    let referenced = referenced_pending(text, value, pending);
    let mut out = String::new();

    if !referenced.is_empty() {
        out.push_str(&format_banner(&referenced));
        out.push('\n');
    }

    out.push_str(text);

    let elsewhere: Vec<&PendingFile> = pending
        .iter()
        .filter(|p| !referenced.iter().any(|r| r.path == p.path))
        .collect();
    if !elsewhere.is_empty() {
        out.push_str("\n\n");
        out.push_str(&format_footer(&elsewhere));
    }

    out
}

fn format_banner(files: &[&PendingFile]) -> String {
    let now = now_ms();
    let mut s = String::from(
        "⚠️ Some files referenced below were edited since the last index sync —\n\
         their ax entries may be stale:\n",
    );
    for p in files.iter().take(12) {
        let age = now.saturating_sub(p.last_seen_ms);
        s.push_str(&format!("  - {} (edited {}ms ago", p.path, age));
        if p.indexing {
            s.push_str(", indexing");
        } else {
            s.push_str(", pending sync");
        }
        s.push_str(")\n");
    }
    if files.len() > 12 {
        s.push_str(&format!("  - … and {} more\n", files.len() - 12));
    }
    s.push_str(
        "For accurate content of those specific files, Read them directly.\n\
         The rest of this response is fresh.\n",
    );
    s
}

fn format_footer(files: &[&PendingFile]) -> String {
    let names: Vec<&str> = files.iter().take(6).map(|p| p.path.as_str()).collect();
    let mut s = format!(
        "(Note: {} file(s) elsewhere in this project are pending index sync but were not referenced above: {}",
        files.len(),
        names.join(", ")
    );
    if files.len() > 6 {
        s.push_str(", …");
    }
    s.push(')');
    s
}

fn referenced_pending<'a>(
    text: &str,
    value: &Value,
    pending: &'a [PendingFile],
) -> Vec<&'a PendingFile> {
    let mut paths = BTreeSet::new();
    collect_path_strings(value, &mut paths);
    // Also match pending paths mentioned in the rendered text.
    for p in pending {
        if !p.path.is_empty() && text.contains(&p.path) {
            paths.insert(normalize_path(&p.path));
        }
    }

    pending
        .iter()
        .filter(|p| paths.contains(&normalize_path(&p.path)))
        .collect()
}

fn collect_path_strings(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let key = k.to_ascii_lowercase();
                let pathish = matches!(
                    key.as_str(),
                    "file"
                        | "filepath"
                        | "path"
                        | "file_path"
                        | "relatedfiles"
                        | "related_files"
                        | "files"
                );
                match v {
                    Value::String(s) if pathish || looks_like_path(s) => {
                        out.insert(normalize_path(s));
                    }
                    Value::Array(arr) if pathish => {
                        for item in arr {
                            if let Some(s) = item.as_str() {
                                out.insert(normalize_path(s));
                            } else {
                                collect_path_strings(item, out);
                            }
                        }
                    }
                    _ => collect_path_strings(v, out),
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                collect_path_strings(item, out);
            }
        }
        Value::String(s) if looks_like_path(s) => {
            out.insert(normalize_path(s));
        }
        _ => {}
    }
}

fn looks_like_path(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() || t.len() > 512 || t.contains('\n') || t.contains(' ') {
        return false;
    }
    let has_sep = t.contains('/') || t.contains('\\');
    let has_dot_ext = t.rsplit(['/', '\\']).next().is_some_and(|leaf| {
        leaf.contains('.') && !leaf.starts_with('.')
    });
    has_sep || has_dot_ext
}

fn normalize_path(s: &str) -> String {
    s.trim().replace('\\', "/")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pf(path: &str) -> PendingFile {
        PendingFile {
            path: path.into(),
            first_seen_ms: 0,
            last_seen_ms: now_ms() - 800,
            indexing: false,
        }
    }

    #[test]
    fn banner_when_response_references_pending_file() {
        let pending = vec![pf("src/Widget.ts"), pf("src/other.ts")];
        let value = json!({ "filePath": "src/Widget.ts", "name": "Widget" });
        let text = "## Code Context\nfn Widget() {}";
        let out = annotate_staleness(text, &value, &pending);
        assert!(out.starts_with("⚠️"));
        assert!(out.contains("src/Widget.ts"));
        assert!(out.contains("pending sync"));
        assert!(out.contains("Read them directly"));
        assert!(out.contains("elsewhere"));
        assert!(out.contains("src/other.ts"));
        assert!(out.contains("## Code Context"));
    }

    #[test]
    fn footer_only_when_no_referenced_pending() {
        let pending = vec![pf("src/a.ts")];
        let value = json!({ "filePath": "src/b.ts" });
        let out = annotate_staleness("hello", &value, &pending);
        assert!(!out.starts_with("⚠️"));
        assert!(out.contains("elsewhere"));
        assert!(out.contains("src/a.ts"));
        assert!(out.starts_with("hello"));
    }

    #[test]
    fn no_annotation_when_nothing_pending() {
        let value = json!({ "filePath": "src/a.ts" });
        assert_eq!(annotate_staleness("hello", &value, &[]), "hello");
    }

    #[test]
    fn text_mention_counts_as_reference() {
        let pending = vec![pf("crates/ax-mcp/src/server.rs")];
        let value = json!({});
        let text = "See crates/ax-mcp/src/server.rs for wrap logic";
        let out = annotate_staleness(text, &value, &pending);
        assert!(out.starts_with("⚠️"));
        assert!(out.contains("crates/ax-mcp/src/server.rs"));
    }
}
