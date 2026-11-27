//! Call and import reference extraction (Gap 1).

use std::collections::HashSet;

use ax_types::{Language, NodeKind, ReferenceKind, UnresolvedReference};
use tree_sitter::{Node as TsNode, Tree};

use super::common::SymbolSpan;

/// Per-file inputs shared by the call and function-reference walkers.
pub struct RefCtx<'a> {
    pub source: &'a [u8],
    pub file_path: &'a str,
    pub language: Language,
    pub file_id: &'a str,
    pub spans: &'a [SymbolSpan],
}

/// Append unresolved call/import references for JS/TS family grammars.
///
/// Calls whose target is a function of this file also get a Calls edge, unless
/// the call goes through another object (`JSON.parse()`, `v.len()`).
pub fn append_ts_js_refs(result: &mut ax_types::ExtractionResult, tree: &Tree, ctx: &RefCtx) {
    let root = tree.root_node();
    let mut local = Vec::new();
    walk_for_calls(root, ctx, result, &mut local);
    link_local_calls(result, ctx.file_path, &local);
    walk_for_imports(
        root,
        ctx.source,
        ctx.file_path,
        ctx.file_id,
        ctx.language,
        result,
    );
    let gate = build_fn_ref_gate(result, ctx.spans);
    walk_fn_ref_nodes(root, ctx, &gate, result);
}

/// Append call references for Rust / Python / Go / Java tree-sitter grammars.
///
/// Calls whose target is a function of this file also get a Calls edge, unless
/// the call goes through another object.
pub fn append_lang_call_refs(
    result: &mut ax_types::ExtractionResult,
    tree: &Tree,
    ctx: &RefCtx,
    call_kinds: &[&str],
) {
    let root = tree.root_node();
    let mut local = Vec::new();
    for kind in call_kinds {
        walk_call_nodes(root, ctx, kind, result, &mut local);
    }
    link_local_calls(result, ctx.file_path, &local);
}

/// Indices into `unresolved_references` of calls made without a foreign
/// receiver: only these may name a function of the same file.
type LocalCalls = Vec<usize>;

fn walk_for_calls(
    node: TsNode,
    ctx: &RefCtx,
    result: &mut ax_types::ExtractionResult,
    local: &mut LocalCalls,
) {
    if node.kind() == "call_expression" {
        if let Some(callee) = extract_ts_js_callee(node, ctx.source) {
            // extract_ts_js_callee keeps `receiver.` unless the receiver is this/self.
            let via_receiver = callee.contains('.');
            push_call_ref(node, ctx, &callee, via_receiver, result, local);
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_for_calls(child, ctx, result, local);
        }
    }
}

fn walk_call_nodes(
    node: TsNode,
    ctx: &RefCtx,
    call_kind: &str,
    result: &mut ax_types::ExtractionResult,
    local: &mut LocalCalls,
) {
    if node.kind() == call_kind {
        let callee = if call_kind == "method_invocation" {
            field_text(node, ctx.source, "name")
        } else {
            extract_generic_callee(node, ctx.source)
        };
        if let Some(callee) = callee {
            if !callee.is_empty() {
                let via_receiver = call_has_foreign_receiver(node, ctx.source);
                push_call_ref(node, ctx, &callee, via_receiver, result, local);
            }
        }
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_call_nodes(child, ctx, call_kind, result, local);
        }
    }
}

/// Whether a call goes through an object other than this/self
/// (`JSON.parse()`, `v.len()`, `Registry.lookup()`).
fn call_has_foreign_receiver(call: TsNode, source: &[u8]) -> bool {
    let receiver = if let Some(func) = call.child_by_field_name("function") {
        receiver_of(func)
    } else if let Some(obj) = call
        .child_by_field_name("object")
        .or_else(|| call.child_by_field_name("receiver"))
    {
        Some(obj)
    } else if call.kind() == "call_expression" {
        call.named_child(0).and_then(receiver_of)
    } else {
        None
    };
    receiver.is_some_and(|r| {
        let text = node_text(r, source);
        !text.is_empty() && !is_skip_receiver(&text)
    })
}

/// The object part of a member access expression, for the grammars ax parses.
fn receiver_of(func: TsNode) -> Option<TsNode> {
    match func.kind() {
        "member_access_expression" => func.child_by_field_name("expression"),
        "field_expression" => func.child_by_field_name("value"),
        "selector_expression" => func.child_by_field_name("operand"),
        "member_expression" | "attribute" => func.child_by_field_name("object"),
        "navigation_expression" if func.named_child_count() > 1 => func.named_child(0),
        _ => None,
    }
}

fn push_call_ref(
    node: TsNode,
    ctx: &RefCtx,
    callee: &str,
    via_receiver: bool,
    result: &mut ax_types::ExtractionResult,
    local: &mut LocalCalls,
) {
    if is_noise_callee(callee) || is_external_callee(callee) {
        return;
    }
    let callee = normalize_callee_name(callee);
    if callee.is_empty() || is_external_callee(&callee) {
        return;
    }
    if !via_receiver {
        local.push(result.unresolved_references.len());
    }
    let line = node.start_position().row as i32 + 1;
    let column = node.start_position().column as i32;
    let from_id = enclosing_symbol_id(line, ctx.spans).unwrap_or_else(|| ctx.file_id.to_string());
    result.unresolved_references.push(UnresolvedReference {
        from_node_id: from_id,
        reference_name: callee.to_string(),
        reference_kind: ReferenceKind::Calls,
        line,
        column,
        file_path: Some(ctx.file_path.to_string()),
        language: Some(ctx.language),
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
                "identifier" => push_import_binding(
                    file_id,
                    file_path,
                    &module_path,
                    clause_child,
                    source,
                    language,
                    result,
                ),
                "named_imports" => {
                    for k in 0..clause_child.named_child_count() {
                        let spec = clause_child.named_child(k).unwrap();
                        if spec.kind() == "import_specifier" {
                            let name = field_text(spec, source, "alias")
                                .or_else(|| field_text(spec, source, "name"))
                                .or_else(|| spec.named_child(0).map(|n| node_text(n, source)));
                            if let Some(name) = name {
                                if !name.is_empty() {
                                    push_import_name(
                                        file_id,
                                        file_path,
                                        &module_path,
                                        spec,
                                        language,
                                        &name,
                                        result,
                                    );
                                }
                            }
                        }
                    }
                }
                "namespace_import" => {
                    for k in 0..clause_child.named_child_count() {
                        let id = clause_child.named_child(k).unwrap();
                        if id.kind() == "identifier" {
                            push_import_binding(
                                file_id,
                                file_path,
                                &module_path,
                                id,
                                source,
                                language,
                                result,
                            );
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
                push_import_name(
                    file_id,
                    file_path,
                    "",
                    spec,
                    Language::Typescript,
                    &name,
                    result,
                );
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
    push_import_name(
        file_id,
        file_path,
        module_path,
        node,
        language,
        &name,
        result,
    );
}

fn push_import_name(
    file_id: &str,
    file_path: &str,
    module_path: &str,
    node: TsNode,
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
        if let Some(name) = callee_from_expression(func, source) {
            return Some(normalize_callee_name(&name));
        }
    }
    if let Some(name) = node.child_by_field_name("name") {
        return Some(normalize_callee_name(&node_text(name, source)));
    }
    // Kotlin's call_expression has no fields: the callee is its first child.
    if node.kind() == "call_expression" {
        if let Some(name) = node
            .named_child(0)
            .and_then(|f| callee_from_expression(f, source))
        {
            return Some(normalize_callee_name(&name));
        }
    }
    None
}

fn callee_from_expression(node: TsNode, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "type_identifier" => Some(node_text(node, source)),
        "generic_name" => {
            if let Some(name) = node.child_by_field_name("name") {
                return Some(node_text(name, source));
            }
            for i in 0..node.named_child_count() {
                let child = node.named_child(i)?;
                if child.kind() == "identifier" {
                    return Some(node_text(child, source));
                }
            }
            None
        }
        "member_access_expression" => field_text(node, source, "name"),
        // Kotlin `a.b`: the last identifier is the member.
        "navigation_expression" => node
            .named_child(node.named_child_count().saturating_sub(1))
            .filter(|n| n.kind() == "identifier")
            .map(|n| node_text(n, source)),
        "field_expression" | "selector_expression" => node
            .child_by_field_name("field")
            .map(|f| node_text(f, source)),
        "qualified_name" => node
            .named_child(node.named_child_count().saturating_sub(1))
            .map(|n| node_text(n, source)),
        _ => node
            .child_by_field_name("path")
            .map(|scope| node_text(scope, source)),
    }
}

fn normalize_callee_name(name: &str) -> String {
    let trimmed = name.trim();
    let base = trimmed.split('<').next().unwrap_or(trimmed).trim();
    base.rsplit('.').next().unwrap_or(base).to_string()
}

fn is_external_callee(name: &str) -> bool {
    let base = normalize_callee_name(name);
    matches!(
        base.as_str(),
        "AddSingleton"
            | "AddScoped"
            | "AddTransient"
            | "AddMediatR"
            | "RegisterServicesFromAssembly"
            | "GetExecutingAssembly"
            | "AddControllers"
            | "AddDbContext"
            | "AddAutoMapper"
            | "AddHttpClient"
            | "AddMemoryCache"
            | "AddStackExchangeRedisCache"
            | "AddAuthentication"
            | "AddAuthorization"
            | "AddCors"
            | "AddSwaggerGen"
            | "UseAuthentication"
            | "UseAuthorization"
            | "UseRouting"
            | "UseEndpoints"
            | "UseSwagger"
            | "UseSwaggerUI"
            | "MapControllers"
            | "Configure"
            | "ConfigureServices"
            | "Build"
            | "Run"
            | "RunAsync"
            | "CreateBuilder"
            | "CreateDefaultBuilder"
            | "GetService"
            | "GetRequiredService"
            | "WriteLine"
            | "ToString"
    )
}

fn enclosing_symbol_id(line: i32, spans: &[SymbolSpan]) -> Option<String> {
    spans
        .iter()
        .filter(|s| line >= s.start_line && line <= s.end_line)
        .max_by_key(|s| s.start_line)
        .map(|s| s.id.clone())
}

fn is_noise_callee(name: &str) -> bool {
    matches!(
        name,
        "if" | "for" | "while" | "switch" | "catch" | "require" | "import"
    )
}

fn is_skip_receiver(receiver: &str) -> bool {
    matches!(receiver, "this" | "self" | "super" | "static")
}

fn field_text(node: TsNode, source: &[u8], field: &str) -> Option<String> {
    node.child_by_field_name(field)
        .map(|n| node_text(n, source))
}

fn clean_string_literal(s: String) -> String {
    s.trim().trim_matches('"').trim_matches('\'').to_string()
}

fn node_text(node: TsNode, source: &[u8]) -> String {
    node.utf8_text(source).unwrap_or("").trim().to_string()
}

const FN_REF_STOPLIST: &[&str] = &[
    "this",
    "self",
    "super",
    "null",
    "nil",
    "true",
    "false",
    "undefined",
    "new",
    "NULL",
    "nullptr",
    "None",
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
    ctx: &RefCtx,
    gate: &HashSet<String>,
    result: &mut ax_types::ExtractionResult,
) {
    match node.kind() {
        "call_expression" => {
            if let Some(args) = node.child_by_field_name("arguments") {
                collect_fn_ref_from_args(args, ctx, gate, result);
            }
        }
        "assignment_expression" => {
            if let Some(rhs) = node.child_by_field_name("right") {
                maybe_emit_fn_ref(rhs, node, ctx, gate, result);
            }
        }
        _ => {}
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_fn_ref_nodes(child, ctx, gate, result);
        }
    }
}

fn collect_fn_ref_from_args(
    node: TsNode,
    ctx: &RefCtx,
    gate: &HashSet<String>,
    result: &mut ax_types::ExtractionResult,
) {
    for i in 0..node.named_child_count() {
        let child = node.named_child(i).unwrap();
        maybe_emit_fn_ref(child, node, ctx, gate, result);
    }
}

fn maybe_emit_fn_ref(
    value_node: TsNode,
    context_node: TsNode,
    ctx: &RefCtx,
    gate: &HashSet<String>,
    result: &mut ax_types::ExtractionResult,
) {
    let name = extract_fn_ref_name(value_node, ctx.source);
    if name.is_none() {
        return;
    }
    let name = name.unwrap();
    if name.is_empty() || FN_REF_STOPLIST.contains(&name.as_str()) || !gate.contains(&name) {
        return;
    }
    let line = context_node.start_position().row as i32 + 1;
    let column = context_node.start_position().column as i32;
    let from_id = enclosing_symbol_id(line, ctx.spans).unwrap_or_else(|| ctx.file_id.to_string());
    result.unresolved_references.push(UnresolvedReference {
        from_node_id: from_id,
        reference_name: name,
        reference_kind: ReferenceKind::FunctionRef,
        line,
        column,
        file_path: Some(ctx.file_path.to_string()),
        language: Some(ctx.language),
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

/// Same-file Calls edges for the `local` call refs that name a function or
/// method of this file. The refs stay unresolved for cross-file resolution.
fn link_local_calls(result: &mut ax_types::ExtractionResult, file_path: &str, local: &[usize]) {
    if local.is_empty() {
        return;
    }
    let local_names: std::collections::HashMap<&str, &str> = result
        .nodes
        .iter()
        .filter(|n| {
            n.file_path == file_path
                && matches!(
                    n.kind,
                    ax_types::NodeKind::Function | ax_types::NodeKind::Method
                )
        })
        .map(|n| (n.name.as_str(), n.id.as_str()))
        .collect();
    let mut edges = Vec::new();
    for &idx in local {
        let Some(ref_) = result.unresolved_references.get(idx) else {
            continue;
        };
        if let Some(target_id) = local_names.get(ref_.reference_name.as_str()) {
            edges.push(ax_types::Edge {
                source: ref_.from_node_id.clone(),
                target: target_id.to_string(),
                kind: ax_types::EdgeKind::Calls,
                metadata: None,
                line: Some(ref_.line),
                column: Some(ref_.column),
                provenance: Some(ax_types::Provenance::TreeSitter),
                confidence: Some(ax_types::EdgeConfidence::Extracted),
            });
        }
    }
    result.edges.extend(edges);
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
            &RefCtx {
                source: source.as_bytes(),
                file_path: "greet.ts",
                language: Language::Typescript,
                file_id: &file_id,
                spans: &spans,
            },
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

    fn ts_extract(path: &str, source: &str) -> ax_types::ExtractionResult {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("grammar");
        let tree = parser.parse(source, None).expect("parse");
        TypescriptExtractor.extract(source.as_bytes(), &tree, path)
    }

    fn call_edges_into(result: &ax_types::ExtractionResult, name: &str) -> Vec<i32> {
        let id = &result.nodes.iter().find(|n| n.name == name).unwrap().id;
        result
            .edges
            .iter()
            .filter(|e| e.kind == ax_types::EdgeKind::Calls && &e.target == id)
            .filter_map(|e| e.line)
            .collect()
    }

    #[test]
    fn member_call_on_another_object_is_not_a_same_file_edge() {
        let source = "function parse(t: string) { return t; }\n\
                      function load(t: string) { return JSON.parse(t); }\n\
                      function run(t: string) { return parse(t); }\n";
        let result = ts_extract("load.ts", source);
        assert_eq!(
            call_edges_into(&result, "parse"),
            vec![3],
            "only the bare parse() on line 3 calls the local function"
        );
    }

    #[test]
    fn this_call_still_links_to_the_local_method() {
        let source = "class A {\n  helper() { return 1; }\n  run() { return this.helper(); }\n}\n";
        let result = ts_extract("a.ts", source);
        assert_eq!(call_edges_into(&result, "helper"), vec![3]);
    }

    #[test]
    fn rust_receiver_call_is_not_a_same_file_edge() {
        let source = "fn len() -> usize { 0 }\nfn f(v: Vec<u8>) -> usize { v.len() + len() }\n";
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("grammar");
        let tree = parser.parse(source, None).expect("parse");
        let result =
            crate::languages::rust::RustExtractor.extract(source.as_bytes(), &tree, "f.rs");
        let len_id = &result.nodes.iter().find(|n| n.name == "len").unwrap().id;
        let edges = result
            .edges
            .iter()
            .filter(|e| e.kind == ax_types::EdgeKind::Calls && &e.target == len_id)
            .count();
        assert_eq!(edges, 1, "v.len() must not link to the local fn len");
    }
}
