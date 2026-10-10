//! Agent-facing rendering shared by stdio and embedded callers.
//! Selection and caching remain in their existing domain layers.
use ax_types::{GraphStats, PendingFile};
use serde_json::{json, Value};

pub fn model_text(value: &Value) -> String {
    for key in ["inject", "preview"] {
        if let Some(text) = value
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            return text.to_owned();
        }
    }
    if let Some(text) = value
        .pointer("/proposal/preview")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        return text.to_owned();
    }
    for key in ["text", "body", "markdown"] {
        if let Some(text) = value
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        {
            return text.to_owned();
        }
    }
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        return format!("Error: {error}");
    }
    if value.get("ok").and_then(Value::as_bool) == Some(true) {
        if let (Some(id), Some(instruction)) = (
            value.get("id").and_then(Value::as_str),
            value.get("instruction").and_then(Value::as_str),
        ) {
            // Similar-memory warnings stay actionable in text-only clients.
            let mut text = format!("{instruction}\nID: {id}");
            if let Some(similar) = value
                .get("similar")
                .and_then(Value::as_array)
                .filter(|a| !a.is_empty())
            {
                text.push_str("\nSimilar memories:");
                for entry in similar {
                    let memory = entry.get("memory").unwrap_or(entry);
                    text.push_str(&format!(
                        "\n- {} — {}",
                        memory["id"].as_str().unwrap_or(""),
                        memory["title"].as_str().unwrap_or("")
                    ));
                }
            }
            return text;
        }
    }
    // Machine-first tools without a text projection retain their existing contract.
    value.to_string()
}

/// Remove only the formatter-owned policy envelope. Rule bodies are byte-for-byte intact.
pub fn policy_markdown(block: &str) -> String {
    if block.starts_with("<ax_policy ") {
        if let Some((_, body)) = block.split_once('\n') {
            if let Some(body) = body.strip_suffix("</ax_policy>\n") {
                return format!("Apply matched rules and skills before editing.\n\n{body}");
            }
        }
    }
    block.to_owned()
}

/// Details belong to ax_status; preflight needs freshness and actionable warnings.
pub fn index_context(stats: &GraphStats, pending: &[PendingFile]) -> String {
    let mut text = format!(
        "## Index\n\n{} files; {} nodes; {} edges.",
        stats.file_count, stats.node_count, stats.edge_count
    );
    if let Some(warning) = ax_core::stats_format::source_store_warning(stats) {
        text.push_str(&format!("\n{warning}"));
    }
    if !pending.is_empty() {
        text.push_str(&format!(
            "\n{} files pending sync; call `ax_status` for details.",
            pending.len()
        ));
        for file in pending.iter().take(8) {
            text.push_str(&format!("\n- {}", file.path));
        }
    }
    text.push('\n');
    text
}

/// Strip an envelope only on a freshly generated auxiliary block, never on arbitrary user text.
pub fn generated_markdown(block: &str, tag: &str, heading: &str) -> String {
    if block.starts_with(&format!("<{tag}")) {
        if let Some((_, body)) = block.split_once('>') {
            if let Some(body) = body.trim_end().strip_suffix(&format!("</{tag}>")) {
                return format!("## {heading}\n\n{}", body.trim());
            }
        }
    }
    block.to_owned()
}

pub fn memory_results(matches: &[ax_memory::MemoryMatch]) -> String {
    let mut text =
        String::from("## Relevant memories\n\nUse `ax_recall` with `id` for full content.\n");
    for m in matches {
        let summary: String = m.memory.body.chars().take(240).collect();
        text.push_str(&format!(
            "\n- {} [{}] {} — {}{}\n",
            m.memory.id,
            m.memory.kind,
            m.memory.title.replace('\n', " "),
            summary.replace('\n', " "),
            if m.memory.body.chars().count() > 240 {
                "… (full content available by ID)"
            } else {
                ""
            }
        ));
        if !m.memory.files.is_empty() {
            text.push_str(&format!("  Files: {}\n", m.memory.files.join(", ")));
        }
    }
    text
}

pub fn recall_metadata(value: &Value) -> Value {
    let entries: Vec<Value> = value.get("matches").and_then(Value::as_array).into_iter().flatten().map(|entry| {
        let memory = &entry["memory"];
        let mut meta = json!({"id":memory.get("id"),"kind":memory.get("kind"),"score":entry.get("score"),"files":memory.get("files"),"source":memory.get("source")});
        if let Some(obj) = meta.as_object_mut() { obj.retain(|_, v| !v.is_null()); }
        meta
    }).collect();
    json!({"matches":entries})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_bodies_and_embedded_markup_are_preserved() {
        let body = "## Rules\nNever omit exceptions. <ax_policy>example</ax_policy>\n";
        let text = policy_markdown(&format!(
            "<ax_policy note=\"matched\">\n{body}</ax_policy>\n"
        ));
        assert!(text.contains(body));
        assert!(!text.starts_with('<'));
    }
    #[test]
    fn errors_and_reports_are_readable_without_json_wrapping() {
        assert_eq!(
            model_text(&json!({"error":"not found"})),
            "Error: not found"
        );
        assert_eq!(model_text(&json!({"markdown":"# Report"})), "# Report");
    }
    #[test]
    fn recall_metadata_keeps_provenance_without_repeating_bodies() {
        let meta = recall_metadata(
            &json!({"matches":[{"score":0.9,"memory":{"id":"m1","body":"large body","source":"mcp","files":["a.rs"]}}]}),
        );
        assert_eq!(meta["matches"][0]["id"], "m1");
        assert!(!meta.to_string().contains("large body"));
    }
}
