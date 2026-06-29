use ax_types::{ExtractionResult, Language, NodeKind};
use tree_sitter::Tree;

use crate::languages::common::{extract_symbols, file_node_id, symbol_spans_from_result};
use crate::languages::refs::{append_lang_call_refs, emit_same_file_call_edges};
use crate::LanguageExtractor;

pub struct GoExtractor;

impl LanguageExtractor for GoExtractor {
    fn language(&self) -> Language {
        Language::Go
    }

    fn extensions(&self) -> &[&str] {
        &[".go"]
    }

    fn extract(&self, source: &[u8], tree: &Tree, path: &str) -> ExtractionResult {
        let mut result = extract_symbols(
            tree,
            source,
            path,
            Language::Go,
            &[
                (NodeKind::Function, "function_declaration"),
                (NodeKind::Method, "method_declaration"),
                (NodeKind::Struct, "type_spec"),
            ],
        );
        let spans = symbol_spans_from_result(&result);
        let file_id = file_node_id(path);
        append_lang_call_refs(&mut result, tree, source, path, Language::Go, &spans, &file_id, &["call_expression"]);
        emit_same_file_call_edges(&mut result, path);
        result
    }
}