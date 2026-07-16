//! Human-readable index statistics for MCP inject and CLI output.

use std::collections::HashMap;

use ax_extraction::markdown::{MARKDOWN_EXTENSIONS, OPAQUE_DOC_EXTENSIONS};
use ax_types::{GraphStats, PendingFile};

const OFFICE_EXTENSIONS: &[&str] = &[
    "docx", "doc", "xlsx", "xls", "pptx", "ppt", "odt", "ods", "odp", "pages", "numbers",
    "keynote",
];

/// Compact extension breakdown sorted by count descending (e.g. `43 md, 10 json, 2 html`).
pub fn format_docs_by_extension(docs: &HashMap<String, i64>) -> String {
    if docs.is_empty() {
        return "none indexed".to_string();
    }
    let mut items: Vec<_> = docs.iter().collect();
    items.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    items
        .iter()
        .map(|(ext, count)| format!("{count} {ext}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn sum_extensions(docs: &HashMap<String, i64>, extensions: &[&str]) -> i64 {
    extensions
        .iter()
        .map(|ext| docs.get(*ext).copied().unwrap_or(0))
        .sum()
}

fn format_category_line(label: &str, docs: &HashMap<String, i64>, extensions: &[&str]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for ext in extensions {
        if let Some(count) = docs.get(*ext).copied().filter(|&n| n > 0) {
            parts.push(format!("{count} {ext}"));
        }
    }
    if parts.is_empty() {
        format!("{label}: none indexed")
    } else {
        format!("{label}: {}", parts.join(", "))
    }
}

/// Auto-injected index snapshot for `ax_preflight`.
pub fn format_index_inject_block(stats: &GraphStats, pending: &[PendingFile]) -> String {
    let doc_total = stats.nodes_by_kind.get("doc").copied().unwrap_or(0);
    let docs_edges = stats.edges_by_kind.get("documents").copied().unwrap_or(0);
    let refs_edges = stats.edges_by_kind.get("references").copied().unwrap_or(0);

    let mut body = String::from(
        "<ax_index note=\"Indexed project snapshot — auto-injected each turn.\">\n",
    );
    body.push_str(&format!(
        "Graph: {} nodes, {} edges, {} code files\n",
        stats.node_count, stats.edge_count, stats.file_count
    ));
    body.push_str(&format!(
        "Docs: {} total — {}\n",
        doc_total,
        format_docs_by_extension(&stats.docs_by_extension)
    ));
    body.push_str(&format!(
        "{}\n",
        format_category_line("Markdown (parsed)", &stats.docs_by_extension, MARKDOWN_EXTENSIONS)
    ));
    body.push_str(&format!(
        "{}\n",
        format_category_line("Office", &stats.docs_by_extension, OFFICE_EXTENSIONS)
    ));
    body.push_str(&format!(
        "{}\n",
        format_category_line("PDF", &stats.docs_by_extension, &["pdf"])
    ));

    let opaque_other = stats
        .docs_by_extension
        .iter()
        .filter(|(ext, _)| {
            !MARKDOWN_EXTENSIONS.contains(&ext.as_str())
                && !OFFICE_EXTENSIONS.contains(&ext.as_str())
                && ext.as_str() != "pdf"
        })
        .map(|(_, n)| *n)
        .sum::<i64>();
    if opaque_other > 0 {
        body.push_str(&format!(
            "Other opaque docs: {} ({})\n",
            opaque_other,
            OPAQUE_DOC_EXTENSIONS
                .iter()
                .filter(|ext| stats.docs_by_extension.get(**ext).copied().unwrap_or(0) > 0)
                .map(|ext| {
                    format!(
                        "{} {}",
                        stats.docs_by_extension.get(*ext).copied().unwrap_or(0),
                        ext
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    if docs_edges > 0 || refs_edges > 0 {
        body.push_str(&format!(
            "Doc links: {docs_edges} doc→code mentions, {refs_edges} doc→doc references\n"
        ));
    }

    if !pending.is_empty() {
        body.push_str("Pending sync:\n");
        for p in pending.iter().take(8) {
            let age_ms = now_ms().saturating_sub(p.last_seen_ms);
            body.push_str(&format!("  - {} (edited {}ms ago", p.path, age_ms));
            if p.indexing {
                body.push_str(", indexing");
            }
            body.push_str(")\n");
        }
        if pending.len() > 8 {
            body.push_str(&format!("  - … and {} more\n", pending.len() - 8));
        }
    }

    body.push_str("</ax_index>\n");
    body
}

/// Formatted text for `ax_status` MCP and CLI output.
pub fn format_status_text(
    stats: &GraphStats,
    last_indexed_at: i64,
    pending: &[PendingFile],
) -> String {
    let doc_total = stats.nodes_by_kind.get("doc").copied().unwrap_or(0);
    let docs_edges = stats.edges_by_kind.get("documents").copied().unwrap_or(0);

    let mut out = String::from("## ax Status\n\n");
    out.push_str(&format!(
        "Nodes: {} | Edges: {} | Code files: {}\n",
        stats.node_count, stats.edge_count, stats.file_count
    ));
    out.push_str(&format!(
        "Docs: {} — {}\n",
        doc_total,
        format_docs_by_extension(&stats.docs_by_extension)
    ));

    let md = sum_extensions(&stats.docs_by_extension, MARKDOWN_EXTENSIONS);
    let office = sum_extensions(&stats.docs_by_extension, OFFICE_EXTENSIONS);
    let pdf = stats.docs_by_extension.get("pdf").copied().unwrap_or(0);
    out.push_str(&format!(
        "Doc categories: {md} markdown (parsed), {office} office, {pdf} pdf\n"
    ));

    if docs_edges > 0 {
        out.push_str(&format!("Doc→code links: {docs_edges}\n"));
    }

    if let Some(unresolved) = stats.unresolved_ref_count.filter(|&n| n > 0) {
        out.push_str(&format!("Unresolved refs: {unresolved}\n"));
    }

    out.push_str(&format!("Last indexed: {last_indexed_at}\n"));

    if pending.is_empty() {
        out.push_str("\nIndex is up to date — no files pending sync.\n");
    } else {
        out.push_str("\n### Pending sync:\n");
        for p in pending {
            let age_ms = now_ms().saturating_sub(p.last_seen_ms);
            out.push_str(&format!("- {} (edited {}ms ago", p.path, age_ms));
            if p.indexing {
                out.push_str(", indexing");
            }
            out.push_str(")\n");
        }
    }

    out
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

    #[test]
    fn formats_extension_breakdown() {
        let mut docs = HashMap::new();
        docs.insert("md".to_string(), 43);
        docs.insert("json".to_string(), 10);
        docs.insert("html".to_string(), 2);
        assert_eq!(
            format_docs_by_extension(&docs),
            "43 md, 10 json, 2 html"
        );
    }

    #[test]
    fn index_inject_lists_categories() {
        let mut stats = GraphStats {
            node_count: 100,
            edge_count: 200,
            file_count: 50,
            ..Default::default()
        };
        stats.nodes_by_kind.insert("doc".to_string(), 3);
        stats.docs_by_extension.insert("md".to_string(), 2);
        stats.docs_by_extension.insert("pdf".to_string(), 1);
        stats.edges_by_kind.insert("documents".to_string(), 5);

        let block = format_index_inject_block(&stats, &[]);
        assert!(block.contains("<ax_index"));
        assert!(block.contains("Docs: 3 total"));
        assert!(block.contains("2 md"));
        assert!(block.contains("PDF: 1 pdf"));
        assert!(block.contains("Office: none indexed"));
        assert!(block.contains("5 doc→code mentions"));
    }

    #[test]
    fn status_text_includes_pending() {
        let mut stats = GraphStats {
            node_count: 10,
            edge_count: 20,
            file_count: 5,
            ..Default::default()
        };
        stats.nodes_by_kind.insert("doc".to_string(), 1);
        stats.docs_by_extension.insert("md".to_string(), 1);

        let pending = vec![PendingFile {
            path: "src/foo.rs".to_string(),
            first_seen_ms: 0,
            last_seen_ms: now_ms() - 500,
            indexing: false,
        }];
        let text = format_status_text(&stats, 12345, &pending);
        assert!(text.contains("## ax Status"));
        assert!(text.contains("1 md"));
        assert!(text.contains("Pending sync"));
        assert!(text.contains("src/foo.rs"));
    }
}
