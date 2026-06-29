//! Search query utilities.

pub fn is_test_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("/test/") || lower.contains("_test.") || lower.contains(".test.") || lower.ends_with("_test.rs")
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

use ax_types::Node;
use crate::query_parser::ParsedQuery;

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