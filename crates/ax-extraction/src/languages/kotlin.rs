//! Kotlin extraction via the tree-sitter-kotlin-ng grammar.

use ax_types::{ExtractionResult, Language, NodeKind};
use tree_sitter::Tree;

use crate::languages::common::{extract_symbols_with, file_node_id, symbol_spans_from_result};
use crate::languages::refs::{append_lang_call_refs, RefCtx};
use crate::LanguageExtractor;

pub struct KotlinExtractor;

impl LanguageExtractor for KotlinExtractor {
    fn language(&self) -> Language {
        Language::Kotlin
    }

    fn extensions(&self) -> &[&str] {
        &[".kt", ".kts"]
    }

    fn extract(&self, source: &[u8], tree: &Tree, path: &str) -> ExtractionResult {
        let mut result = extract_symbols_with(
            tree,
            source,
            path,
            Language::Kotlin,
            &[
                (NodeKind::Class, "class_declaration"),
                (NodeKind::Class, "object_declaration"),
                (NodeKind::Function, "function_declaration"),
            ],
            kotlin_symbol_kind,
        );
        let spans = symbol_spans_from_result(&result);
        let file_id = file_node_id(path);
        append_lang_call_refs(
            &mut result,
            tree,
            &RefCtx {
                source,
                file_path: path,
                language: Language::Kotlin,
                file_id: &file_id,
                spans: &spans,
            },
            &["call_expression"],
        );
        result
    }
}

/// kotlin-ng has no separate interface or method nodes: an interface is a
/// `class_declaration` with the `interface` keyword, and a method is a
/// `function_declaration` inside a class or object body.
fn kotlin_symbol_kind(node: tree_sitter::Node, kind: NodeKind) -> NodeKind {
    match node.kind() {
        "class_declaration" => {
            let mut cursor = node.walk();
            let is_interface = node.children(&mut cursor).any(|c| c.kind() == "interface");
            if is_interface {
                NodeKind::Interface
            } else {
                kind
            }
        }
        "function_declaration" => {
            let in_body = node
                .parent()
                .is_some_and(|p| matches!(p.kind(), "class_body" | "enum_class_body"));
            if in_body {
                NodeKind::Method
            } else {
                NodeKind::Function
            }
        }
        _ => kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_types::{EdgeKind, ReferenceKind};

    const SOURCE: &str = r#"
package app

interface Greeter {
    fun greet(name: String): String
}

object Registry {
    fun lookup(key: String): String = key
}

class Service : Greeter {
    override fun greet(name: String): String {
        return format(name)
    }

    fun run() {
        greet("x")
        Registry.lookup("k")
    }
}

fun format(name: String): String = "Hello, $name"
"#;

    fn extract(path: &str, src: &str) -> ExtractionResult {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(src, None).unwrap();
        KotlinExtractor.extract(src.as_bytes(), &tree, path)
    }

    fn kind_of(result: &ExtractionResult, name: &str) -> Vec<NodeKind> {
        result
            .nodes
            .iter()
            .filter(|n| n.name == name)
            .map(|n| n.kind)
            .collect()
    }

    #[test]
    fn extracts_kotlin_classes_objects_interfaces_and_functions() {
        let result = extract("src/Service.kt", SOURCE);
        assert_eq!(kind_of(&result, "Service"), vec![NodeKind::Class]);
        assert_eq!(kind_of(&result, "Registry"), vec![NodeKind::Class]);
        assert_eq!(kind_of(&result, "Greeter"), vec![NodeKind::Interface]);
        assert_eq!(kind_of(&result, "format"), vec![NodeKind::Function]);
        assert_eq!(kind_of(&result, "run"), vec![NodeKind::Method]);
        assert_eq!(kind_of(&result, "lookup"), vec![NodeKind::Method]);
    }

    #[test]
    fn extracts_kotlin_call_references_and_local_call_edges() {
        let result = extract("src/Service.kt", SOURCE);
        let calls: Vec<&str> = result
            .unresolved_references
            .iter()
            .filter(|r| r.reference_kind == ReferenceKind::Calls)
            .map(|r| r.reference_name.as_str())
            .collect();
        for name in ["format", "greet", "lookup"] {
            assert!(calls.contains(&name), "missing call {name}: {calls:?}");
        }
        let format_id = &result.nodes.iter().find(|n| n.name == "format").unwrap().id;
        assert!(
            result
                .edges
                .iter()
                .any(|e| e.kind == EdgeKind::Calls && &e.target == format_id),
            "a local call to format() must become a Calls edge"
        );
    }

    #[test]
    fn kts_is_mapped_to_kotlin() {
        assert_eq!(
            crate::grammars::language_for_extension(".kts"),
            Some(Language::Kotlin)
        );
    }
}
