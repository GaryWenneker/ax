//! Deeply nested source must not overflow a parse worker's stack.
//!
//! A stack overflow is not a panic: it aborts the whole ax process during index
//! or sync. Generated but well-formatted files (data tables, nested calls) are
//! not caught by the generated-file check, so the AST walkers must not recurse
//! once per tree level.
//!
//! Run: cargo test -p ax-extraction --test deep_nesting

use ax_extraction::parse_pool::{ParsePool, ParseTask};
use ax_types::Language;

/// Depth well past what a recursive walk survives on a 2 MiB worker stack.
const DEPTH: usize = 100_000;

fn nested_source() -> String {
    let mut s = String::from("function leaf(x) { return x; }\nconst data = ");
    s.push_str(&"[".repeat(DEPTH));
    s.push('1');
    s.push_str(&"]".repeat(DEPTH));
    s.push_str(";\nconst value = ");
    s.push_str(&"leaf(".repeat(DEPTH));
    s.push('1');
    s.push_str(&")".repeat(DEPTH));
    s.push_str(";\n");
    s
}

/// A call chain `leaf(1)(1)(1)…` and a parenthesized argument `leaf((((leaf))))`.
fn chained_source() -> String {
    let mut s = String::from("function leaf(x) { return leaf; }\nconst chain = leaf");
    s.push_str(&"(1)".repeat(DEPTH));
    s.push_str(";\nconst wrapped = leaf(");
    s.push_str(&"(".repeat(DEPTH));
    s.push_str("leaf");
    s.push_str(&")".repeat(DEPTH));
    s.push_str(");\n");
    s
}

fn parse_on_small_stack(language: Language, path: &str) -> usize {
    extract_on_small_stack(language, path, nested_source())
}

fn extract_on_small_stack(language: Language, path: &str, source: String) -> usize {
    let path = path.to_string();
    // Same stack size as a default rayon worker; the pool itself runs on these.
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            let pool = ParsePool::new();
            let results = pool.parse_batch(vec![ParseTask {
                file_path: path,
                content: source,
                language,
            }]);
            let extraction = results[0].1.as_ref().expect("parse ok");
            extraction
                .unresolved_references
                .iter()
                .filter(|r| r.reference_name == "leaf")
                .count()
        })
        .expect("spawn")
        .join()
        .expect("extraction thread")
}

#[test]
fn deeply_nested_javascript_is_extracted_without_overflow() {
    assert_eq!(parse_on_small_stack(Language::Javascript, "deep.js"), DEPTH);
}

#[test]
fn deeply_nested_typescript_is_extracted_without_overflow() {
    assert_eq!(parse_on_small_stack(Language::Typescript, "deep.ts"), DEPTH);
}

#[test]
fn deep_call_chains_and_parentheses_are_extracted_without_overflow() {
    let leaf_calls = extract_on_small_stack(Language::Javascript, "chain.js", chained_source());
    // The innermost calls of the chain and the wrapped call name `leaf`; the
    // outer links of a very deep chain are not resolved to a name.
    assert!(leaf_calls >= 2, "got {leaf_calls}");
}
