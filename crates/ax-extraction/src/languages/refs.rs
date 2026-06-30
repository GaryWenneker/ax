//! Call and import reference extraction (Gap 1).

use std::collections::HashSet;

use ax_types::{Language, NodeKind, ReferenceKind, UnresolvedReference};
use tree_sitter::{Node as TsNode, Tree};

use super::common::SymbolSpan;

/// Append unresolved call/import references for JS/TS family grammars.
pub fn append_ts_js_refs(
    result: &mut ax_types::ExtractionResult,
    tree: &Tree,
    source: &[u8],
    file_path: &str,
    language: Language,
    file_id: &str,
    spans: &[SymbolSpan],
) {
    let root = tree.root_node();
    walk_for_calls(root, source, file_path, language, file_id, spans, result);
    walk_for_imports(root, source, file_path, file_id, language, result);
    let gate = build_fn_ref_gate(result, spans);
    walk_fn_ref_nodes(root, source, file_path, language, file_id, spans, &gate, result);
}

/// Append call references for Rust / Python / Go / Java tree-sitter grammars.
pub fn append_lang_call_refs(
    result: &mut ax_types::ExtractionResult,
    tree: &Tree,
    source: &[u8],
    file_path: &str,
    language: Language,
    spans: &[SymbolSpan],
    file_id: &str,
    call_kinds: &[&str],
) {
    let root = tree.root_node();
    for kind in call_kinds {
        walk_call_nodes(root, source, file_path, language, kind, file_id, spans, result);
    }
}

fn walk_for_calls(
    node: TsNode,
    source: &[u8],
    file_path: &str,
    language: Language,
    file_id: &str,
    spans: &[SymbolSpan],
    result: &mut ax_types::ExtractionResult,
) {
    if node.kind() == "call_expression" {
        if let Some(callee) = extract_ts_js_callee(node, source) {
            push_call_ref(node, source, file_path, language, file_id, spans, &callee, result);
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_for_calls(child, source, file_path, language, file_id, spans, result);
        }
    }
}

fn walk_call_nodes(
    node: TsNode,
    source: &[u8],
    file_path: &str,
    language: Language,
    call_kind: &str,
    file_id: &str,
    spans: &[SymbolSpan],
    result: &mut ax_types::ExtractionResult,
) {
    if node.kind() == call_kind {
        let callee = if call_kind == "method_invocation" {
            field_text(node, source, "name")
        } else {
            extract_generic_callee(node, source)
        };
        if let Some(callee) = callee {
            if !callee.is_empty() {
                push_call_ref(node, source, file_path, language, file_id, spans, &callee, result);
            }
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_call_nodes(child, source, file_path, language, call_kind, file_id, spans, result);
        }
    }
}

fn push_call_ref(
    node: TsNode,
    _source: &[u8],
    file_path: &str,
    language: Language,
    file_id: &str,
    spans: &[SymbolSpan],
    callee: &str,
    result: &mut ax_types::ExtractionResult,
) {
    if is_noise_callee(callee) {
        return;
    }
    let line = node.start_position().row as i32 + 1;
    let column = node.start_position().column as i32;
    let from_id = enclosing_symbol_id(line, spans).unwrap_or_else(|| file_id.to_string());
    result.unresolved_references.push(UnresolvedReference {
        from_node_id: from_id,
        reference_name: callee.to_string(),
        reference_kind: ReferenceKind::Calls,
        line,
        column,
        file_path: Some(file_path.to_string()),
        language: Some(language),
        candidates: None,
    });
}

fn walk_for_imports(
    node: TsNode,
    source: &[u8],
    file_path: &str,
    file_id: &str,
    language: Language,
    result: &mut ax_types::ExtractionResult,
) {
    match node.kind() {
        "import_statement" | "import_declaration" => {
            emit_ts_js_import_refs(node, source, file_path, file_id, language, result);
        }
        "export_statement" => {
            emit_ts_js_reexport_refs(node, source, file_path, file_id, result);
        }
        _ => {}
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_for_imports(child, source, file_path, file_id, language, result);
        }
    }
}

fn emit_ts_js_import_refs(
    node: TsNode,
    source: &[u8],
    file_path: &str,
    file_id: &str,
    language: Language,
    result: &mut ax_types::ExtractionResult,
) {
    let line = node.start_position().row as i32 + 1;
    let column = node.start_position().column as i32;
    let module_path = node
        .child_by_field_name("source")
        .map(|n| clean_string_literal(node_text(n, source)))
        .unwrap_or_default();

    // Side-effect import: import './module'
    if !module_path.is_empty() {
        result.unresolved_references.push(UnresolvedReference {
            from_node_id: file_id.to_string(),
            reference_name: module_path.clone(),
            reference_kind: ReferenceKind::Imports,
            line,
            column,
            file_path: Some(file_path.to_string()),
            language: Some(language),
            candidates: Some(vec![module_path.clone()]),
        });
    }

    // Named/default bindings from import_clause
    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        if child.kind() != "import_clause" {
            continue;
        }
        for j in 0..child.named_child_count() {
            let clause_child = child.named_child(j).unwrap();
            match clause_child.kind() {
                "identifier" => push_import_binding(file_id, file_path, &module_path, clause_child, source, language, result),
                "named_imports" => {
                    for k in 0..clause_child.named_child_count() {
                        let spec = clause_child.named_child(k).unwrap();
                        if spec.kind() == "import_specifier" {
                            let name = field_text(spec, source, "alias")
                                .or_else(|| field_text(spec, source, "name"))
                                .or_else(|| {
                                    spec.named_child(0).map(|n| node_text(n, source))
                                });
                            if let Some(name) = name {
                                if !name.is_empty() {
                                    push_import_name(file_id, file_path, &module_path, spec, source, language, &name, result);
                                }
                            }
                        }
                    }
                }
                "namespace_import" => {
                    for k in 0..clause_child.named_child_count() {
                        let id = clause_child.named_child(k).unwrap();
                        if id.kind() == "identifier" {
                            push_import_binding(file_id, file_path, &module_path, id, source, language, result);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

fn emit_ts_js_reexport_refs(
    node: TsNode,
    source: &[u8],
    file_path: &str,
    file_id: &str,
    result: &mut ax_types::ExtractionResult,
) {
    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        if child.kind() != "export_clause" {
            continue;
        }
        for j in 0..child.named_child_count() {
            let spec = child.named_child(j).unwrap();
            if spec.kind() != "export_specifier" {
                continue;
            }
            let name = field_text(spec, source, "name")
                .or_else(|| spec.named_child(0).map(|n| node_text(n, source)));
            if let Some(name) = name {
                if name.is_empty() || name == "default" {
                    continue;
                }
                push_import_name(file_id, file_path, "", spec, source, Language::Typescript, &name, result);
            }
        }
    }
}

fn push_import_binding(
    file_id: &str,
    file_path: &str,
    module_path: &str,
    node: TsNode,
    source: &[u8],
    language: Language,
    result: &mut ax_types::ExtractionResult,
) {
    let name = node_text(node, source);
    if name.is_empty() {
        return;
    }
    push_import_name(file_id, file_path, module_path, node, source, language, &name, result);
}

fn push_import_name(
    file_id: &str,
    file_path: &str,
    module_path: &str,
    node: TsNode,
    _source: &[u8],
    language: Language,
    name: &str,
    result: &mut ax_types::ExtractionResult,
) {
    result.unresolved_references.push(UnresolvedReference {
        from_node_id: file_id.to_string(),
        reference_name: name.to_string(),
        reference_kind: ReferenceKind::Imports,
        line: node.start_position().row as i32 + 1,
        column: node.start_position().column as i32,
        file_path: Some(file_path.to_string()),
        language: Some(language),
        candidates: if module_path.is_empty() {
            None
        } else {
            Some(vec![module_path.to_string()])
        },
    });
}

fn extract_ts_js_callee(node: TsNode, source: &[u8]) -> Option<String> {
    let func = node.child_by_field_name("function")?;
    match func.kind() {
        "identifier" => Some(node_text(func, source)),
        "member_expression" => {
            let prop = func.child_by_field_name("property");
            let method = prop.map(|p| node_text(p, source)).unwrap_or_default();
            if method.is_empty() {
                return None;
            }
            let obj = func.child_by_field_name("object");
            if let Some(obj) = obj {
                let receiver = node_text(obj, source);
                if receiver.is_empty() || is_skip_receiver(&receiver) {
                    Some(method)
                } else {
                    Some(format!("{}.{}", receiver, method))
                }
            } else {
                Some(method)
            }
        }
        "call_expression" => {
            // Chained call: inner().method() — use inner callee + ().method if member follows
            extract_ts_js_callee(func, source)
        }
        "parenthesized_expression" | "await_expression" => {
            for i in 0..func.named_child_count() {
                let child = func.named_child(i).unwrap();
                if let Some(c) = extract_ts_js_callee_from_expr(child, source) {
                    return Some(c);
                }
            }
            None
        }
        _ => extract_ts_js_callee_from_expr(func, source),
    }
    .filter(|s| !s.is_empty())
}

fn extract_ts_js_callee_from_expr(node: TsNode, source: &[u8]) -> Option<String> {
    if node.kind() == "call_expression" {
        return extract_ts_js_callee(node, source);
    }
    if node.kind() == "identifier" {
        let t = node_text(node, source);
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    } else {
        None
    }
}

fn extract_generic_callee(node: TsNode, source: &[u8]) -> Option<String> {
    if let Some(func) = node.child_by_field_name("function") {
        if func.kind() == "identifier" || func.kind() == "type_identifier" {
            return Some(node_text(func, source));
        }
        if let Some(scope) = func.child_by_field_name("path") {
            return Some(node_text(scope, source));
        }
        if func.kind() == "field_expression" || func.kind() == "selector_expression" {
            let field = func.child_by_field_name("field");
            return field.map(|f| node_text(f, source));
        }
    }
  if let Some(name) = node.child_by_field_name("name") {
        return Some(node_text(name, source));
    }
    None
}

fn enclosing_symbol_id(line: i32, spans: &[SymbolSpan]) -> Option<String> {
    spans
        .iter()
        .filter(|s| line >= s.start_line && line <= s.end_line)
        .max_by_key(|s| s.start_line)
        .map(|s| s.id.clone())
}

fn is_noise_callee(name: &str) -> bool {
    matches!(name, "if" | "for" | "while" | "switch" | "catch" | "require" | "import")
}

fn is_skip_receiver(receiver: &str) -> bool {
    matches!(receiver, "this" | "self" | "super" | "static")
}

fn field_text(node: TsNode, source: &[u8], field: &str) -> Option<String> {
    node.child_by_field_name(field).map(|n| node_text(n, source))
}

fn clean_string_literal(s: String) -> String {
    s.trim().trim_matches('"').trim_matches('\'').to_string()
}

fn node_text(node: TsNode, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").trim().to_string()
}


const FN_REF_STOPLIST: &[&str] = &[
    "this", "self", "super", "null", "nil", "true", "false", "undefined", "new", "NULL", "nullptr", "None",
];

fn build_fn_ref_gate(result: &ax_types::ExtractionResult, spans: &[SymbolSpan]) -> HashSet<String> {
    let mut names = HashSet::new();
    for n in &result.nodes {
        if matches!(n.kind, NodeKind::Function | NodeKind::Method) {
            names.insert(n.name.clone());
        }
    }
    for span in spans {
        if let Some(n) = result.nodes.iter().find(|x| x.id == span.id) {
            if matches!(n.kind, NodeKind::Function | NodeKind::Method) {
                names.insert(n.name.clone());
            }
        }
    }
    for r in &result.unresolved_references {
        if r.reference_kind == ReferenceKind::Imports {
            names.insert(r.reference_name.clone());
        }
    }
    names
}

fn walk_fn_ref_nodes(
    node: TsNode,
    source: &[u8],
    file_path: &str,
    language: Language,
    file_id: &str,
    spans: &[SymbolSpan],
    gate: &HashSet<String>,
    result: &mut ax_types::ExtractionResult,
) {
    match node.kind() {
        "call_expression" => {
            if let Some(args) = node.child_by_field_name("arguments") {
                collect_fn_ref_from_args(args, source, file_path, language, file_id, spans, gate, result);
            }
        }
        "assignment_expression" => {
            if let Some(rhs) = node.child_by_field_name("right") {
                maybe_emit_fn_ref(rhs, node, source, file_path, language, file_id, spans, gate, result);
            }
        }
        _ => {}
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_fn_ref_nodes(child, source, file_path, language, file_id, spans, gate, result);
        }
    }
}

fn collect_fn_ref_from_args(
    node: TsNode,
    source: &[u8],
    file_path: &str,
    language: Language,
    file_id: &str,
    spans: &[SymbolSpan],
    gate: &HashSet<String>,
    result: &mut ax_types::ExtractionResult,
) {
    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        maybe_emit_fn_ref(child, node, source, file_path, language, file_id, spans, gate, result);
    }
}

fn maybe_emit_fn_ref(
    value_node: TsNode,
    context_node: TsNode,
    source: &[u8],
    file_path: &str,
    language: Language,
    file_id: &str,
    spans: &[SymbolSpan],
    gate: &HashSet<String>,
    result: &mut ax_types::ExtractionResult,
) {
    let name = extract_fn_ref_name(value_node, source);
    if name.is_none() {
        return;
    }
    let name = name.unwrap();
    if name.is_empty() || FN_REF_STOPLIST.contains(&name.as_str()) || !gate.contains(&name) {
        return;
    }
    let line = context_node.start_position().row as i32 + 1;
    let column = context_node.start_position().column as i32;
    let from_id = enclosing_symbol_id(line, spans).unwrap_or_else(|| file_id.to_string());
    result.unresolved_references.push(UnresolvedReference {
        from_node_id: from_id,
        reference_name: name,
        reference_kind: ReferenceKind::FunctionRef,
        line,
        column,
        file_path: Some(file_path.to_string()),
        language: Some(language),
        candidates: None,
    });
}

fn extract_fn_ref_name(node: TsNode, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" => {
            let t = node_text(node, source);
            if t.is_empty() {
                None
            } else {
                Some(t)
            }
        }
        "parenthesized_expression" | "await_expression" => {
            for i in 0..node.named_child_count() {
                let child = node.named_child(i).unwrap();
                if let Some(n) = extract_fn_ref_name(child, source) {
                    return Some(n);
                }
            }
            None
        }
        _ => None,
    }
}


/// Same-file call edges for refs that match a local symbol by name.
pub fn emit_same_file_call_edges(result: &mut ax_types::ExtractionResult, file_path: &str) {
    let local_names: std::collections::HashMap<String, String> = result
        .nodes
        .iter()
        .filter(|n| n.file_path == file_path && (matches!(n.kind, ax_types::NodeKind::Function | ax_types::NodeKind::Method)))
        .map(|n| (n.name.clone(), n.id.clone()))
        .collect();

    let mut resolved_indices = Vec::new();
    for (idx, ref_) in result.unresolved_references.iter().enumerate() {
        if ref_.reference_kind != ReferenceKind::Calls {
            continue;
        }
        if ref_.reference_name.contains('.') {
            continue;
        }
        if let Some(target_id) = local_names.get(&ref_.reference_name) {
            result.edges.push(ax_types::Edge {
                source: ref_.from_node_id.clone(),
                target: target_id.clone(),
                kind: ax_types::EdgeKind::Calls,
                metadata: None,
                line: Some(ref_.line),
                column: Some(ref_.column),
                provenance: Some(ax_types::Provenance::TreeSitter),
            });
            resolved_indices.push(idx);
        }
    }
    // Keep unresolved for cross-file; same-file still in DB for resolution stats
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::common::{file_node_id, symbol_spans_from_result};
    use crate::languages::typescript::TypescriptExtractor;
    use crate::LanguageExtractor;
    use ax_types::ReferenceKind;
    use tree_sitter::Parser;

    #[test]
    fn ts_call_and_import_refs() {
        let source = r#"
import { hello } from "./hello";
export function greet(name: string) {
  return hello(name);
}
"#;
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("grammar");
        let tree = parser.parse(source, None).expect("parse");
        let extractor = TypescriptExtractor;
        let mut result = extractor.extract(source.as_bytes(), &tree, "greet.ts");
        let spans = symbol_spans_from_result(&result);
        let file_id = file_node_id("greet.ts");
        append_ts_js_refs(
            &mut result,
            &tree,
            source.as_bytes(),
            "greet.ts",
            Language::Typescript,
            &file_id,
            &spans,
        );
        let calls = result
            .unresolved_references
            .iter()
            .filter(|r| r.reference_kind == ReferenceKind::Calls)
            .count();
        let imports = result
            .unresolved_references
            .iter()
            .filter(|r| r.reference_kind == ReferenceKind::Imports)
            .count();
        assert!(calls >= 1);
        assert!(imports >= 1);
    }

    #[test]
    fn ts_function_ref_in_call_arg() {
        let source = r#"
function handler() { return 1; }
function register(cb: () => void) { cb(); }
export function setup() {
  register(handler);
}
"#;
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("grammar");
        let tree = parser.parse(source, None).expect("parse");
        let extractor = TypescriptExtractor;
        let result = extractor.extract(source.as_bytes(), &tree, "setup.ts");
        let fn_refs = result
            .unresolved_references
            .iter()
            .filter(|r| r.reference_kind == ReferenceKind::FunctionRef)
            .count();
        assert!(fn_refs >= 1, "expected function-ref candidate for handler");
    }
}