//! Mark test functions and emit Covers edges from test calls to production symbols.

use ax_types::{Edge, EdgeConfidence, EdgeKind, ExtractionResult, Node, NodeKind, Provenance};
use tree_sitter::Tree;

/// Mark test functions and emit Covers edges from test calls to production symbols.
pub fn annotate_tests(result: &mut ExtractionResult, source: &[u8], tree: &Tree, path: &str) {
    if !is_likely_test_file(path) {
        return;
    }

    let lines = LineIndex::new(source);
    let test_ids: Vec<(String, u32, u32)> = result
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method))
        .filter(|n| is_test_function_node(n, source, &lines))
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
                        confidence: Some(EdgeConfidence::Inferred),
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

/// Byte ranges of each line in a source file (without the trailing newline).
struct LineIndex {
    ranges: Vec<(usize, usize)>,
}

impl LineIndex {
    fn new(source: &[u8]) -> Self {
        let mut ranges = Vec::new();
        let mut begin = 0;
        for (i, b) in source.iter().enumerate() {
            if *b == b'\n' {
                ranges.push((begin, i));
                begin = i + 1;
            }
        }
        if begin < source.len() {
            ranges.push((begin, source.len()));
        }
        Self { ranges }
    }

    /// Text of the 0-based line `idx`, or `None` past the end.
    fn line<'a>(&self, source: &'a [u8], idx: usize) -> Option<std::borrow::Cow<'a, str>> {
        self.ranges
            .get(idx)
            .map(|(a, b)| String::from_utf8_lossy(&source[*a..*b]))
    }
}

/// Lines above a function that we scan for test attributes and decorators.
const MAX_ATTRIBUTE_LINES: usize = 32;

/// A function is a test when its name says so, when the contiguous block of
/// attribute, decorator and comment lines directly above it holds a test
/// marker (`#[test]`, `#[tokio::test]`, `#[rstest]`, `@pytest`), or when it is
/// the callback named on an `it(` / `describe(` line.
fn is_test_function_node(node: &Node, source: &[u8], lines: &LineIndex) -> bool {
    if node.name.starts_with("test_") || node.name.starts_with("Test") {
        return true;
    }
    let Some(idx) = usize::try_from(node.start_line - 1).ok() else {
        return false;
    };
    let Some(own) = lines.line(source, idx) else {
        return false;
    };
    if has_attribute_marker(&own) || has_call_marker(&own) {
        return true;
    }
    for above in (0..idx).rev().take(MAX_ATTRIBUTE_LINES) {
        let Some(text) = lines.line(source, above) else {
            break;
        };
        let t = text.trim();
        let is_annotation = t.starts_with('#')
            || t.starts_with('@')
            || t.starts_with("//")
            || t.starts_with("/*")
            || t.starts_with('*');
        if !is_annotation {
            break;
        }
        if has_attribute_marker(t) {
            return true;
        }
    }
    false
}

fn has_attribute_marker(text: &str) -> bool {
    text.contains("#[test]")
        || text.contains("#[tokio::test")
        || text.contains("#[rstest")
        || text.contains("@pytest")
}

/// `it(` or `describe(` as a call of its own, not the tail of `split(` or `x.it(`.
fn has_call_marker(text: &str) -> bool {
    ["it(", "describe("].iter().any(|marker| {
        text.match_indices(marker).any(|(pos, _)| {
            text[..pos]
                .chars()
                .next_back()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '$' || c == '.'))
        })
    })
}

fn find_call_target<'a>(result: &'a ExtractionResult, name: &str) -> Option<&'a Node> {
    result.nodes.iter().find(|n| {
        matches!(n.kind, NodeKind::Function | NodeKind::Method)
            && (n.name == name || n.qualified_name.ends_with(name))
    })
}

fn collect_calls_in_range(
    tree: &Tree,
    source: &[u8],
    start_line: u32,
    end_line: u32,
) -> Vec<String> {
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
                    let name = text.split('.').next_back().unwrap_or(text).trim();
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

    fn fn_node(name: &str, start_line: i32) -> Node {
        Node {
            id: format!("id:{name}"),
            kind: NodeKind::Function,
            name: name.to_string(),
            qualified_name: name.to_string(),
            file_path: "tests/foo.rs".to_string(),
            language: ax_types::Language::Rust,
            start_line,
            end_line: start_line,
            start_column: 0,
            end_column: 0,
            docstring: None,
            signature: None,
            visibility: None,
            is_exported: None,
            is_async: None,
            is_static: None,
            is_abstract: None,
            decorators: None,
            type_parameters: None,
            return_type: None,
            updated_at: 0,
        }
    }

    fn check(source: &str, name: &str, line: i32) -> bool {
        let lines = LineIndex::new(source.as_bytes());
        is_test_function_node(&fn_node(name, line), source.as_bytes(), &lines)
    }

    #[test]
    fn helper_far_below_an_early_test_attribute_is_not_a_test() {
        let mut src = String::from("#[test]\nfn first() { helper(); }\n");
        for _ in 0..30 {
            src.push('\n');
        }
        src.push_str("fn helper() {}\n");
        let line = src
            .lines()
            .position(|l| l.starts_with("fn helper"))
            .unwrap() as i32
            + 1;
        assert!(!check(&src, "helper", line));
    }

    #[test]
    fn helper_right_after_a_test_fn_is_not_a_test() {
        let src = "#[test]\nfn first() { helper(); }\nfn helper() {}\n";
        assert!(!check(src, "helper", 3));
    }

    #[test]
    fn attribute_directly_above_marks_a_test_far_into_the_file() {
        let mut src = String::new();
        for i in 0..60 {
            src.push_str(&format!("// header comment line {i} without markers\n"));
        }
        src.push('\n');
        src.push_str("#[test]\n#[ignore]\nfn checks_login() {}\n");
        let line = src
            .lines()
            .position(|l| l.starts_with("fn checks_login"))
            .unwrap() as i32
            + 1;
        assert!(check(&src, "checks_login", line));
    }

    #[test]
    fn tokio_and_python_markers_are_detected() {
        let src = "#[tokio::test(flavor = \"multi_thread\")]\nasync fn runs() {}\n";
        assert!(check(src, "runs", 2));
        let py = "@pytest.mark.parametrize('x', [1])\ndef checks(x):\n    pass\n";
        assert!(check(py, "checks", 2));
    }

    #[test]
    fn it_marker_needs_a_token_boundary() {
        let src = "const parts = s.split(',');\nfunction helper() {}\n";
        assert!(!check(src, "helper", 2));
        let src = "  it('logs in', function loginCase() {\n  });\n";
        assert!(check(src, "loginCase", 1));
    }
}
