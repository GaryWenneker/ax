//! Mark test functions and emit Covers edges from test calls to production symbols.

use ax_types::{Edge, EdgeKind, ExtractionResult, Node, NodeKind, Provenance};
use tree_sitter::{Node as TsNode, Tree};

/// Mark test functions and emit Covers edges from test calls to production symbols.
pub fn annotate_tests(result: &mut ExtractionResult, source: &[u8], tree: &Tree, path: &str) {
    if !is_likely_test_file(path) {
        return;
    }

    let test_ids: Vec<(String, u32, u32)> = result
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method))
        .filter(|n| is_test_function_node(n, source))
        .map(|n| (n.id.clone(), n.start_line as u32, n.end_line as u32))
        .collect();

    for (test_id, start, end) in &test_ids {
        if let Some(node) = result.nodes.iter_mut().find(|n| n.id == *test_id) {
            node.kind = NodeKind::Test;
        }
        let calls = collect_calls_in_range(tree, source, *start, *end);
        for name in calls {
            if let Some(target) = find_call_target(result, &name) {
                if target.id != *test_id {
                    result.edges.push(Edge {
                        source: test_id.clone(),
                        target: target.id.clone(),
                        kind: EdgeKind::Covers,
                        metadata: None,
                        line: None,
                        column: None,
                        provenance: Some(Provenance::Heuristic),
                    });
                }
            }
        }
    }
}

fn is_likely_test_file(path: &str) -> bool {
    let lower = path.replace('\\', "/").to_lowercase();
    lower.contains("/tests/")
        || lower.starts_with("tests/")
        || lower.contains("/test/")
        || lower.contains("_test.")
        || lower.contains(".test.")
        || lower.ends_with("_test.rs")
        || lower.contains("/__tests__/")
}

fn is_test_function_node(node: &Node, source: &[u8]) -> bool {
    if node.name.starts_with("test_") || node.name.starts_with("Test") {
        return true;
    }
    let start = node.start_line.saturating_sub(1) as usize;
    if start >= source.len() {
        return false;
    }
    let window = prefix_snippet(source, start, 400);
    window.contains("#[test]")
        || window.contains("#[tokio::test]")
        || window.contains("#[rstest")
        || window.contains("describe(")
        || window.contains("it(")
        || window.contains("@pytest")
}

fn prefix_snippet(source: &[u8], end: usize, max_len: usize) -> String {
    let start = end.saturating_sub(max_len);
    String::from_utf8_lossy(&source[start..end]).into_owned()
}

fn find_call_target<'a>(result: &'a ExtractionResult, name: &str) -> Option<&'a Node> {
    result.nodes.iter().find(|n| {
        matches!(n.kind, NodeKind::Function | NodeKind::Method)
            && (n.name == name || n.qualified_name.ends_with(name))
    })
}

fn collect_calls_in_range(tree: &Tree, source: &[u8], start_line: u32, end_line: u32) -> Vec<String> {
    let start_byte = line_to_byte(source, start_line);
    let end_byte = line_to_byte(source, end_line + 1);
    let mut calls = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.end_byte() <= start_byte || node.start_byte() >= end_byte {
            continue;
        }
        if node.kind() == "call_expression" {
            if let Some(func) = node.child_by_field_name("function") {
                if let Ok(text) = func.utf8_text(source) {
                    let name = text.split('.').last().unwrap_or(text).trim();
                    if !name.is_empty() {
                        calls.push(name.to_string());
                    }
                }
            }
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                stack.push(child);
            }
        }
    }
    calls
}

fn line_to_byte(source: &[u8], line: u32) -> usize {
    if line == 0 {
        return 0;
    }
    source
        .iter()
        .enumerate()
        .filter(|(_, b)| **b == b'\n')
        .nth(line as usize - 1)
        .map(|(i, _)| i + 1)
        .unwrap_or(source.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_test_file_paths() {
        assert!(is_likely_test_file("src/auth_test.rs"));
        assert!(is_likely_test_file("tests/integration/login.rs"));
        assert!(!is_likely_test_file("src/main.rs"));
    }
}
