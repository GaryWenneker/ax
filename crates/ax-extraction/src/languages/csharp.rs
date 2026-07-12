//! C# extraction via tree-sitter-c-sharp.

use ax_types::{ExtractionResult, Language, NodeKind};
use tree_sitter::Tree;

use crate::languages::common::{extract_symbols, file_node_id, symbol_spans_from_result};
use crate::languages::refs::{append_lang_call_refs, emit_same_file_call_edges};
use crate::LanguageExtractor;

pub struct CsharpExtractor;

impl LanguageExtractor for CsharpExtractor {
    fn language(&self) -> Language {
        Language::Csharp
    }

    fn extensions(&self) -> &[&str] {
        &[".cs"]
    }

    fn extract(&self, source: &[u8], tree: &Tree, path: &str) -> ExtractionResult {
        let mut result = extract_symbols(
            tree,
            source,
            path,
            Language::Csharp,
            &[
                (NodeKind::Module, "namespace_declaration"),
                (NodeKind::Class, "class_declaration"),
                (NodeKind::Struct, "struct_declaration"),
                (NodeKind::Interface, "interface_declaration"),
                (NodeKind::Enum, "enum_declaration"),
                (NodeKind::Method, "method_declaration"),
                (NodeKind::Method, "constructor_declaration"),
                (NodeKind::Method, "property_declaration"),
            ],
        );
        let spans = symbol_spans_from_result(&result);
        let file_id = file_node_id(path);
        append_lang_call_refs(
            &mut result,
            tree,
            source,
            path,
            Language::Csharp,
            &spans,
            &file_id,
            &["invocation_expression"],
        );
        emit_same_file_call_edges(&mut result, path);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_pool::{ParsePool, ParseTask};
    use ax_types::Language;

    #[test]
    fn extracts_csharp_symbols() {
        let source = r#"
namespace VFPortal.Business {
    public class BusinessWebsite {
        public void Save() { }
    }
}
"#;
        let pool = ParsePool::new();
        let results = pool.parse_batch(vec![ParseTask {
            file_path: "BusinessWebsite.cs".into(),
            content: source.into(),
            language: Language::Csharp,
        }]);
        let (_, result) = &results[0];
        let extraction = result.as_ref().expect("parse ok");
        assert!(
            extraction.nodes.iter().any(|n| n.name == "BusinessWebsite"),
            "expected class symbol"
        );
        assert!(
            extraction.nodes.iter().any(|n| n.name == "Save"),
            "expected method symbol"
        );
    }

    #[test]
    fn skips_external_di_call_refs() {
        let source = r#"
namespace App {
    public class Module {
        public void Configure(IServiceCollection services) {
            services.AddSingleton<IMapper, Mapper>();
        }
    }
}
"#;
        let pool = ParsePool::new();
        let results = pool.parse_batch(vec![ParseTask {
            file_path: "ApplicationModule.cs".into(),
            content: source.into(),
            language: Language::Csharp,
        }]);
        let extraction = results[0].1.as_ref().expect("parse ok");
        assert!(
            extraction.unresolved_references.is_empty(),
            "AddSingleton should not be stored as unresolved: {:?}",
            extraction.unresolved_references
        );
    }
}
