//! Plain-text formatter for explore results (CLI + MCP parity).

use ax_types::{CallNeighbor, ExploreResult};

/// Format an explore result as agent-readable plain text (CodeGraph explore shape).
pub fn format_explore_text(result: &ExploreResult) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Explore: {}\n\n", result.query));
    out.push_str(&format!("## Summary\n{}\n\n", result.summary));
    out.push_str(&format!("## Blast radius\n{}\n\n", result.blast_radius));

    if result.entries.is_empty() {
        out.push_str("No matching symbols.\n");
        return out;
    }

    for (i, entry) in result.entries.iter().enumerate() {
        let n = &entry.node;
        out.push_str(&format!(
            "## Entry {}: {} ({:?}) — {}:{}-{} [score: {:.3}]\n",
            i + 1,
            n.qualified_name,
            n.kind,
            n.file_path,
            n.start_line,
            n.end_line,
            entry.score
        ));
        if let Some(sig) = &n.signature {
            out.push_str(&format!("Signature: {}\n", sig));
        }

        format_node_list(&mut out, "Callers", &entry.callers);
        format_node_list(&mut out, "Callees", &entry.callees);

        if let Some(src) = &entry.source {
            out.push_str("\n### Source\n");
            out.push_str(src);
            if !src.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }
        out.push_str("---\n\n");
    }

    out
}

const MAX_NEIGHBORS_DEFAULT: usize = 15;

fn max_neighbors() -> usize {
    std::env::var("AX_EXPLORE_MAX_NEIGHBORS")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(MAX_NEIGHBORS_DEFAULT)
}

/// Direct edges (tagged with an edge kind) first, then transitive ones, up to
/// the cap; the heading keeps the full count.
fn format_node_list(out: &mut String, label: &str, neighbors: &[CallNeighbor]) {
    out.push_str(&format!("\n### {} ({})\n", label, neighbors.len()));
    if neighbors.is_empty() {
        out.push_str("(none)\n");
        return;
    }
    let cap = max_neighbors();
    let mut ordered: Vec<&CallNeighbor> = neighbors.iter().filter(|c| c.edge_kind.is_some()).collect();
    ordered.extend(neighbors.iter().filter(|c| c.edge_kind.is_none()));
    for c in ordered.iter().take(cap) {
        let n = &c.node;
        let edge_tag = match (c.edge_kind, c.confidence) {
            (Some(kind), Some(conf)) => format!(" [{} · {}]", kind.as_str(), conf.as_str()),
            (Some(kind), None) => format!(" [{}]", kind.as_str()),
            _ => String::new(),
        };
        out.push_str(&format!(
            "- {} @ {}:{}-{} ({:?}){}\n",
            n.qualified_name, n.file_path, n.start_line, n.end_line, n.kind, edge_tag
        ));
    }
    if ordered.len() > cap {
        let tool = if label == "Callers" { "ax_callers" } else { "ax_callees" };
        out.push_str(&format!(
            "_+{} more; call `{tool}` on this symbol for the full list._\n",
            ordered.len() - cap
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_types::{
        EdgeConfidence, EdgeKind, ExploreEntry, ExploreResult, Language, Node, NodeKind,
    };

    fn neighbor(
        name: &str,
        file: &str,
        line: i32,
        kind: Option<EdgeKind>,
        conf: Option<EdgeConfidence>,
    ) -> CallNeighbor {
        CallNeighbor {
            node: sample_node(name, file, line),
            edge_kind: kind,
            confidence: conf,
        }
    }

    fn sample_node(name: &str, file: &str, line: i32) -> Node {
        Node {
            id: format!("id-{name}"),
            kind: NodeKind::Function,
            name: name.to_string(),
            qualified_name: name.to_string(),
            file_path: file.to_string(),
            language: Language::Typescript,
            start_line: line,
            end_line: line + 2,
            start_column: 0,
            end_column: 0,
            docstring: None,
            signature: Some(format!("fn {name}()")),
            visibility: None,
            is_exported: Some(true),
            is_async: None,
            is_static: None,
            is_abstract: None,
            decorators: None,
            type_parameters: None,
            return_type: None,
            updated_at: 0,
        }
    }

    #[test]
    fn golden_explore_text_shape() {
        let caller = neighbor(
            "callerFn",
            "src/caller.ts",
            10,
            Some(EdgeKind::Calls),
            Some(EdgeConfidence::Extracted),
        );
        let callee = neighbor("calleeFn", "src/callee.ts", 20, None, None);
        let focal = sample_node("greet", "greet.ts", 1);
        let result = ExploreResult {
            query: "greet".to_string(),
            summary: "Found 1 entry point(s) for 'greet'".to_string(),
            blast_radius: "1 entry point(s); 1 caller(s), 1 callee(s) across 3 file(s)".to_string(),
            entries: vec![ExploreEntry {
                node: focal,
                score: 0.95,
                source: Some("1\texport function greet(name: string) { return name; }".to_string()),
                callers: vec![caller],
                callees: vec![callee],
            }],
        };

        let text = format_explore_text(&result);
        assert!(text.contains("# Explore: greet"));
        assert!(text.contains("## Blast radius"));
        assert!(text.contains("## Entry 1: greet"));
        assert!(text.contains("### Callers (1)"));
        assert!(text.contains("### Callees (1)"));
        assert!(text.contains("callerFn @ src/caller.ts"));
        // Direct edge carries a confidence tag; transitive neighbor does not.
        assert!(text.contains("[calls · extracted]"));
        assert!(text.contains("1\texport function greet"));
        assert!(text.contains("Signature: fn greet()"));
    }

    #[test]
    fn long_neighbor_list_is_capped_with_direct_edges_first() {
        let mut callees: Vec<CallNeighbor> = (0..40)
            .map(|i| neighbor(&format!("transitive{i}"), "src/t.ts", i, None, None))
            .collect();
        callees.push(neighbor("directFn", "src/d.ts", 1, Some(EdgeKind::Calls), Some(EdgeConfidence::Extracted)));
        let mut out = String::new();
        format_node_list(&mut out, "Callees", &callees);
        assert!(out.contains("### Callees (41)"), "{out}");
        assert!(out.find("directFn").unwrap() < out.find("transitive0").unwrap(), "{out}");
        assert_eq!(out.lines().filter(|l| l.starts_with("- ")).count(), MAX_NEIGHBORS_DEFAULT);
        assert!(out.contains(&format!("+{} more", 41 - MAX_NEIGHBORS_DEFAULT)), "{out}");
        assert!(out.contains("ax_callees"), "{out}");
    }

    #[test]
    fn short_neighbor_list_is_unchanged() {
        let callers = vec![neighbor("a", "src/a.ts", 1, None, None), neighbor("b", "src/b.ts", 2, None, None)];
        let mut out = String::new();
        format_node_list(&mut out, "Callers", &callers);
        assert_eq!(out.lines().filter(|l| l.starts_with("- ")).count(), 2);
        assert!(!out.contains("more"), "{out}");
    }

    #[test]
    fn empty_entries_message() {
        let result = ExploreResult {
            query: "missing".to_string(),
            summary: "No symbols".to_string(),
            blast_radius: "No symbols matching 'missing'".to_string(),
            entries: vec![],
        };
        let text = format_explore_text(&result);
        assert!(text.contains("No matching symbols."));
    }
}