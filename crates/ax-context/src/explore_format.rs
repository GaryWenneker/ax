//! Plain-text formatter for explore results (CLI + MCP parity).

use ax_types::{ExploreResult, Node};

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

fn format_node_list(out: &mut String, label: &str, nodes: &[Node]) {
    out.push_str(&format!("\n### {} ({})\n", label, nodes.len()));
    if nodes.is_empty() {
        out.push_str("(none)\n");
        return;
    }
    for n in nodes {
        out.push_str(&format!(
            "- {} @ {}:{}-{} ({:?})\n",
            n.qualified_name, n.file_path, n.start_line, n.end_line, n.kind
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_types::{ExploreEntry, ExploreResult, Language, Node, NodeKind};

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
        let caller = sample_node("callerFn", "src/caller.ts", 10);
        let callee = sample_node("calleeFn", "src/callee.ts", 20);
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
        assert!(text.contains("1\texport function greet"));
        assert!(text.contains("Signature: fn greet()"));
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