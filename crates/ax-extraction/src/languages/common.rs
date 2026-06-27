//! Shared extraction helpers.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use ax_types::{
    Edge, EdgeKind, ExtractionResult, Language, Node, NodeKind, Provenance,
};
use tree_sitter::{Node as TsNode, Tree};

#[derive(Debug, Clone)]
pub struct SymbolSpan {
    pub id: String,
    pub start_line: i32,
    pub end_line: i32,
}

pub fn make_node_id(file_path: &str, qualified_name: &str) -> String {
    let mut hasher = DefaultHasher::new();
    file_path.hash(&mut hasher);
    qualified_name.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn file_node_id(file_path: &str) -> String {
    make_node_id(file_path, file_path)
}

pub fn symbol_spans_from_result(result: &ExtractionResult) -> Vec<SymbolSpan> {
    result
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method | NodeKind::Class))
        .map(|n| SymbolSpan {
            id: n.id.clone(),
            start_line: n.start_line,
            end_line: n.end_line,
        })
        .collect()
}

pub fn extract_symbols(
    tree: &Tree,
    source: &[u8],
    file_path: &str,
    language: Language,
    kinds: &[(NodeKind, &str)],
) -> ExtractionResult {
    let mut result = ExtractionResult::default();
    let root = tree.root_node();
    let file_id = file_node_id(file_path);
    let now = now_ms();

    let file_node = Node {
        id: file_id.clone(),
        kind: NodeKind::File,
        name: file_path.rsplit('/').next().unwrap_or(file_path).to_string(),
        qualified_name: file_path.to_string(),
        file_path: file_path.to_string(),
        language,
        start_line: 1,
        end_line: root.end_position().row as i32 + 1,
        start_column: 0,
        end_column: 0,
        docstring: None,
        signature: None,
        visibility: None,
        is_exported: None,
        is_async: None,
        is_static: None,
        is_abstract: None,
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: now,
    };
    result.nodes.push(file_node);

    for (kind, node_type) in kinds {
        walk_nodes(root, source, node_type, &mut |n| {
            let name = declaration_name(n, source, node_type);
            if name.is_empty() {
                return;
            }
            let qualified = format!("{}::{}", file_path, name);
            let id = make_node_id(file_path, &qualified);
            let end_line = n.end_position().row as i32 + 1;
            result.nodes.push(Node {
                id: id.clone(),
                kind: *kind,
                name,
                qualified_name: qualified,
                file_path: file_path.to_string(),
                language,
                start_line: n.start_position().row as i32 + 1,
                end_line,
                start_column: n.start_position().column as i32,
                end_column: n.end_position().column as i32,
                docstring: None,
                signature: None,
                visibility: None,
                is_exported: None,
                is_async: None,
                is_static: None,
                is_abstract: None,
                decorators: None,
                type_parameters: None,
                return_type: None,
                updated_at: now,
            });
            result.edges.push(Edge {
                source: file_id.clone(),
                target: id,
                kind: EdgeKind::Contains,
                metadata: None,
                line: None,
                column: None,
                provenance: Some(Provenance::TreeSitter),
            });
        });
    }

    result
}


fn declaration_name(node: TsNode, source: &[u8], node_type: &str) -> String {
    if let Some(name_node) = node.child_by_field_name("name") {
        let t = name_node.utf8_text(source).unwrap_or("").trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    // method_definition: name field on child identifier
    if node_type == "method_definition" {
        for i in 0..node.named_child_count() {
            let child = node.named_child(i).unwrap();
            if child.kind() == "property_identifier" || child.kind() == "identifier" {
                let t = child.utf8_text(source).unwrap_or("").trim();
                if !t.is_empty() {
                    return t.to_string();
                }
            }
        }
    }
    node.utf8_text(source).unwrap_or("").trim().to_string()
}
fn walk_nodes<F>(node: TsNode, source: &[u8], node_type: &str, f: &mut F)
where
    F: FnMut(TsNode),
{
    if node.kind() == node_type {
        f(node);
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_nodes(child, source, node_type, f);
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}