use ax_types::{ExtractionResult, Language, NodeKind};
use tree_sitter::Tree;

use crate::languages::common::{extract_symbols, file_node_id, symbol_spans_from_result};
use crate::languages::refs::{
    append_lang_call_refs, append_rust_macro_call_refs, emit_same_file_call_edges, RefCtx,
};
use crate::LanguageExtractor;

pub struct RustExtractor;

impl LanguageExtractor for RustExtractor {
    fn language(&self) -> Language {
        Language::Rust
    }

    fn extensions(&self) -> &[&str] {
        &[".rs"]
    }

    fn extract(&self, source: &[u8], tree: &Tree, path: &str) -> ExtractionResult {
        let mut result = extract_symbols(
            tree,
            source,
            path,
            Language::Rust,
            &[
                (NodeKind::Function, "function_item"),
                (NodeKind::Struct, "struct_item"),
                (NodeKind::Trait, "trait_item"),
                (NodeKind::Method, "function_item"),
            ],
        );
        let spans = symbol_spans_from_result(&result);
        let file_id = file_node_id(path);
        let ctx = RefCtx {
            source,
            file_path: path,
            language: Language::Rust,
            file_id: &file_id,
            spans: &spans,
        };
        append_lang_call_refs(&mut result, tree, &ctx, &["call_expression"]);
        append_rust_macro_call_refs(&mut result, tree, &ctx);
        emit_same_file_call_edges(&mut result, path);
        crate::test_mapper::annotate_tests(&mut result, source, tree, path);
        result
    }
}

#[cfg(test)]
mod macro_call_tests {
    use super::*;
    use ax_types::{EdgeKind, ReferenceKind};
    use tree_sitter::Parser;

    fn extract(source: &str) -> ExtractionResult {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("grammar");
        let tree = parser.parse(source, None).expect("parse");
        RustExtractor.extract(source.as_bytes(), &tree, "src/lib.rs")
    }

    /// (caller name, callee name, line) for every call the extractor found.
    fn calls(result: &ExtractionResult) -> Vec<(String, String, i32)> {
        let name_of = |id: &str| {
            result
                .nodes
                .iter()
                .find(|n| n.id == id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| id.to_string())
        };
        let mut found: Vec<(String, String, i32)> = result
            .unresolved_references
            .iter()
            .filter(|r| r.reference_kind == ReferenceKind::Calls)
            .map(|r| (name_of(&r.from_node_id), r.reference_name.clone(), r.line))
            .collect();
        found.sort();
        found
    }

    fn callees(result: &ExtractionResult) -> Vec<String> {
        calls(result)
            .into_iter()
            .map(|(_, callee, _)| callee)
            .collect()
    }

    #[test]
    fn m1_a_call_in_a_println_argument_is_a_call_with_a_same_file_edge() {
        let result = extract(
            "pub fn invoice_total(items: &[u32]) -> u32 {\n    items.len() as u32\n}\n\npub fn print_invoice(items: &[u32]) {\n    println!(\"{}\", invoice_total(items));\n}\n",
        );
        assert!(
            calls(&result).contains(&("print_invoice".into(), "invoice_total".into(), 6)),
            "{:?}",
            calls(&result)
        );
        let id_of = |name: &str| {
            result
                .nodes
                .iter()
                .find(|n| n.name == name)
                .unwrap()
                .id
                .clone()
        };
        assert!(result.edges.iter().any(|e| e.kind == EdgeKind::Calls
            && e.source == id_of("print_invoice")
            && e.target == id_of("invoice_total")));
    }

    #[test]
    fn m2_calls_nested_in_macros_and_in_call_arguments() {
        let result =
            extract("fn check(a: u32) {\n    assert_eq!(total(a), compute(vec![one()]));\n}\n");
        assert_eq!(callees(&result), ["compute", "one", "total"]);
    }

    #[test]
    fn m3_method_and_path_calls() {
        let result = extract(
            "fn show(s: &S) -> String {\n    format!(\"{} {}\", s.name(), billing::rate())\n}\n",
        );
        assert_eq!(callees(&result), ["name", "rate"]);
    }

    #[test]
    fn m4_plain_tokens_macro_names_and_definitions_are_not_calls() {
        let result = extract(
            "fn quiet(count: u32, a: u32, b: u32, x: u32, list: &[u32]) {\n    println!(\"{} {}\", (count), list[0]);\n    let _ = vec![a, b];\n    let _ = matches!(x, 1 | 2);\n    wrap! { fn helper(v: u32) -> u32 { v } }\n}\n",
        );
        assert_eq!(callees(&result), Vec::<String>::new());
    }

    #[test]
    fn m5_the_call_belongs_to_the_enclosing_function_at_its_own_line() {
        let result = extract(
            "fn first() {}\n\nfn second() {\n    let _ = 1;\n    assert!(\n        ready(),\n        \"not ready\"\n    );\n}\n",
        );
        assert_eq!(
            calls(&result),
            [("second".to_string(), "ready".to_string(), 6)]
        );
    }
}
