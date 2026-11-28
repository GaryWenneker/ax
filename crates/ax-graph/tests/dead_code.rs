//! find_dead_code over a real database.

use ax_db::queries::QueryBuilder;
use ax_db::Database;
use ax_graph::GraphQueryManager;
use ax_types::{Edge, EdgeKind, Language, Node, NodeKind};

fn node(id: &str, kind: NodeKind, exported: Option<bool>) -> Node {
    Node {
        id: id.to_string(),
        kind,
        name: id.to_string(),
        qualified_name: id.to_string(),
        file_path: "src/lib.rs".to_string(),
        language: Language::Rust,
        start_line: 1,
        end_line: 2,
        start_column: 0,
        end_column: 0,
        docstring: None,
        signature: None,
        visibility: None,
        is_exported: exported,
        is_async: None,
        is_static: None,
        is_abstract: None,
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: 0,
    }
}

fn edge(source: &str, target: &str, kind: EdgeKind) -> Edge {
    Edge {
        source: source.to_string(),
        target: target.to_string(),
        kind,
        metadata: None,
        line: None,
        column: None,
        provenance: None,
        confidence: None,
    }
}

#[tokio::test]
async fn dead_code_covers_every_node_past_ten_thousand() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("ax.db")).await.unwrap();
    let q = QueryBuilder::new(db.pool().clone());

    let total = 10_050;
    let nodes: Vec<Node> = (0..total)
        .map(|i| node(&format!("f{i:05}"), NodeKind::Function, None))
        .collect();
    q.upsert_nodes(&nodes).await.unwrap();

    let manager = GraphQueryManager::new(QueryBuilder::new(db.pool().clone()));
    let dead = manager.find_dead_code().await.unwrap();
    assert_eq!(dead.len(), total);
}

#[tokio::test]
async fn dead_code_skips_referenced_and_exported_nodes() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("ax.db")).await.unwrap();
    let q = QueryBuilder::new(db.pool().clone());

    q.upsert_nodes(&[
        node("file", NodeKind::File, None),
        node("called", NodeKind::Function, None),
        node("caller", NodeKind::Function, None),
        node("only_contained", NodeKind::Method, None),
        node("exported", NodeKind::Function, Some(true)),
        node("Widget", NodeKind::Class, None),
        node("Shape", NodeKind::Struct, None),
    ])
    .await
    .unwrap();
    q.upsert_edges(&[
        edge("caller", "called", EdgeKind::Calls),
        edge("file", "only_contained", EdgeKind::Contains),
        edge("file", "caller", EdgeKind::Contains),
    ])
    .await
    .unwrap();

    let manager = GraphQueryManager::new(QueryBuilder::new(db.pool().clone()));
    let mut ids: Vec<String> = manager
        .find_dead_code()
        .await
        .unwrap()
        .into_iter()
        .map(|n| n.id)
        .collect();
    ids.sort();
    assert_eq!(ids, vec!["Widget", "caller", "only_contained"]);
}
