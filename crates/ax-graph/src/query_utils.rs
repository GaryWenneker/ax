//! Search query utilities.

/// Whether `path` is a test file; the shared rule is [`ax_utils::is_test_path`].
pub fn is_test_file(path: &str) -> bool {
    ax_utils::is_test_path(path)
}

pub fn extract_search_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_lowercase())
        .collect()
}

pub fn score_path_relevance(path: &str, terms: &[String]) -> f64 {
    let lower = path.to_lowercase();
    let mut score = 0.0;
    for term in terms {
        if lower.contains(term) {
            score += 1.0;
        }
    }
    score
}

pub fn get_stem_variants(term: &str) -> Vec<String> {
    let mut variants = vec![term.to_lowercase()];
    if term.ends_with('s') && term.len() > 2 {
        variants.push(term[..term.len() - 1].to_lowercase());
    }
    variants
}

pub fn is_distinctive_identifier(term: &str) -> bool {
    term.len() >= 3 && term.chars().any(|c| c.is_uppercase() || c == '_')
}

use crate::query_parser::ParsedQuery;
use ax_types::Node;

pub fn matches_parsed_query(node: &Node, parsed: &ParsedQuery) -> bool {
    if !parsed.kinds.is_empty() && !parsed.kinds.contains(&node.kind) {
        return false;
    }
    if !parsed.languages.is_empty() && !parsed.languages.contains(&node.language) {
        return false;
    }
    for p in &parsed.path_filters {
        if !node.file_path.contains(p) {
            return false;
        }
    }
    for n in &parsed.name_filters {
        if node.name != *n {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::is_test_file;

    #[test]
    fn test_file_detection_covers_common_layouts() {
        for path in [
            "crates/ax-db/tests/integrity.rs",
            "tests/cli.rs",
            "web/src/__tests__/App.tsx",
            "web/src/app.spec.ts",
            "web/src/app.test.tsx",
            "pkg/server_test.go",
            "python/test_parser.py",
            "python/tests/conftest.py",
            "src/test/java/FooTest.java",
            "C:\\repo\\tests\\win.rs",
        ] {
            assert!(is_test_file(path), "{path} should be a test file");
        }
        for path in [
            "src/main.rs",
            "src/testing_utils.rs",
            "python/contest.py",
            "src/attest/mod.rs",
            "docs/latest.md",
        ] {
            assert!(!is_test_file(path), "{path} should not be a test file");
        }
    }
}
