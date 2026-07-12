//! Generic tree-sitter extractor driven by per-language node specs.

use ax_types::{ExtractionResult, Language, NodeKind};
use tree_sitter::Tree;

use crate::languages::common::{extract_symbols, file_node_id, symbol_spans_from_result};
use crate::languages::refs::{append_lang_call_refs, emit_same_file_call_edges};
use crate::LanguageExtractor;

pub struct LangSpec {
    pub language: Language,
    pub extensions: &'static [&'static str],
    pub symbols: &'static [(NodeKind, &'static str)],
    pub call_kinds: &'static [&'static str],
}

pub struct GenericExtractor {
    pub spec: &'static LangSpec,
}

impl LanguageExtractor for GenericExtractor {
    fn language(&self) -> Language {
        self.spec.language
    }

    fn extensions(&self) -> &[&str] {
        self.spec.extensions
    }

    fn extract(&self, source: &[u8], tree: &Tree, path: &str) -> ExtractionResult {
        let lang = self.spec.language;
        let mut result = extract_symbols(tree, source, path, lang, self.spec.symbols);
        if !self.spec.call_kinds.is_empty() {
            let spans = symbol_spans_from_result(&result);
            let file_id = file_node_id(path);
            append_lang_call_refs(
                &mut result,
                tree,
                source,
                path,
                lang,
                &spans,
                &file_id,
                self.spec.call_kinds,
            );
            emit_same_file_call_edges(&mut result, path);
        }
        result
    }
}
