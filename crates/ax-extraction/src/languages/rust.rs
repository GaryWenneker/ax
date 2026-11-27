use ax_types::{ExtractionResult, Language, NodeKind};
use tree_sitter::Tree;

use crate::languages::common::{extract_symbols_with, file_node_id, symbol_spans_from_result};
use crate::languages::refs::{append_lang_call_refs, emit_same_file_call_edges, RefCtx};
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
        let mut result = extract_symbols_with(
            tree,
            source,
            path,
            Language::Rust,
            &[
                (NodeKind::Function, "function_item"),
                (NodeKind::Struct, "struct_item"),
                (NodeKind::Trait, "trait_item"),
            ],
            rust_symbol_kind,
        );
        let spans = symbol_spans_from_result(&result);
        let file_id = file_node_id(path);
        append_lang_call_refs(
            &mut result,
            tree,
            &RefCtx {
                source,
                file_path: path,
                language: Language::Rust,
                file_id: &file_id,
                spans: &spans,
            },
            &["call_expression"],
        );
        emit_same_file_call_edges(&mut result, path);
        crate::test_mapper::annotate_tests(&mut result, source, tree, path);
        result
    }
}

/// A `fn` directly inside an `impl` or `trait` body is a method; any other
/// `fn` is a function.
fn rust_symbol_kind(node: tree_sitter::Node, kind: NodeKind) -> NodeKind {
    if kind != NodeKind::Function {
        return kind;
    }
    let in_impl_or_trait = node
        .parent()
        .filter(|p| p.kind() == "declaration_list")
        .and_then(|p| p.parent())
        .is_some_and(|g| matches!(g.kind(), "impl_item" | "trait_item"));
    if in_impl_or_trait {
        NodeKind::Method
    } else {
        NodeKind::Function
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_types::EdgeKind;

    fn extract(path: &str, src: &str) -> ExtractionResult {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(src, None).unwrap();
        RustExtractor.extract(src.as_bytes(), &tree, path)
    }

    fn kinds_named(result: &ExtractionResult, name: &str) -> Vec<NodeKind> {
        result
            .nodes
            .iter()
            .filter(|n| n.name == name)
            .map(|n| n.kind)
            .collect()
    }

    #[test]
    fn free_fn_is_one_function_and_impl_fn_is_one_method() {
        let result = extract(
            "src/lib.rs",
            "fn free() {}\nstruct S;\nimpl S {\n    fn m(&self) {}\n}\ntrait T {\n    fn d(&self) {}\n}\n",
        );
        assert_eq!(kinds_named(&result, "free"), vec![NodeKind::Function]);
        assert_eq!(kinds_named(&result, "m"), vec![NodeKind::Method]);
        assert_eq!(kinds_named(&result, "d"), vec![NodeKind::Method]);
        let contains = result
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Contains)
            .count();
        // file -> free, S, m, T, d
        assert_eq!(
            contains, 5,
            "one Contains edge per symbol: {:?}",
            result.edges
        );
    }

    #[test]
    fn test_fn_in_a_test_file_stays_a_test() {
        let result = extract(
            "tests/it.rs",
            "fn helper() {}\n#[test]\nfn test_checks() {\n    helper();\n}\n",
        );
        assert_eq!(kinds_named(&result, "test_checks"), vec![NodeKind::Test]);
    }
}
