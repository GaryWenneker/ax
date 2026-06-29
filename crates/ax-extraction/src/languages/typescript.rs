use ax_types::{ExtractionResult, Language, NodeKind};
use tree_sitter::Tree;

use crate::languages::common::{extract_symbols, file_node_id, symbol_spans_from_result};
use crate::languages::refs::{append_ts_js_refs, emit_same_file_call_edges};
use crate::LanguageExtractor;

pub struct TypescriptExtractor;

impl LanguageExtractor for TypescriptExtractor {
    fn language(&self) -> Language {
        Language::Typescript
    }

    fn extensions(&self) -> &[&str] {
        &[".ts", ".tsx", ".mts", ".cts"]
    }

    fn extract(&self, source: &[u8], tree: &Tree, path: &str) -> ExtractionResult {
        let lang = if path.ends_with(".tsx") {
            Language::Tsx
        } else {
            Language::Typescript
        };
        let mut result = extract_symbols(
            tree,
            source,
            path,
            lang,
            &[
                (NodeKind::Function, "function_declaration"),
                (NodeKind::Class, "class_declaration"),
                (NodeKind::Interface, "interface_declaration"),
                (NodeKind::Method, "method_definition"),
            ],
        );
        let spans = symbol_spans_from_result(&result);
        let file_id = file_node_id(path);
        append_ts_js_refs(&mut result, tree, source, path, lang, &file_id, &spans);
        emit_same_file_call_edges(&mut result, path);
        result
    }
}