use std::fs;
use std::path::PathBuf;

use ax_db::queries::QueryBuilder;
use ax_db::Database;
use ax_extraction::contracts::index_contracts;
use ax_types::{Edge, EdgeKind};

fn temp_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "ax-contract-federation-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join("client")).unwrap();
    path
}

#[tokio::test]
async fn shared_operation_links_per_file_nodes_without_self_loops() {
    let root = temp_root("shared");
    fs::write(
        root.join("openapi.yaml"),
        "openapi: 3.0.0\npaths:\n  /users:\n    get:\n      operationId: listUsers\n",
    )
    .unwrap();
    fs::write(
        root.join("client/openapi.json"),
        r#"{"openapi":"3.0.0","paths":{"/users":{"get":{"operationId":"listUsers"}}}}"#,
    )
    .unwrap();
    let db_path = root.join(".ax/ax.db");
    fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let db = Database::open(&db_path).await.unwrap();
    let q = QueryBuilder::new(db.pool().clone());

    let written = index_contracts(&root, &q, &[]).await.unwrap();
    assert_eq!(written, 2);

    let routes: Vec<(String, String)> =
        sqlx::query_as("SELECT id, file_path FROM nodes WHERE kind = 'route' ORDER BY id")
            .fetch_all(db.pool())
            .await
            .unwrap();
    assert_eq!(routes.len(), 2, "one operation node per file: {routes:?}");
    assert_ne!(routes[0].1, routes[1].1, "{routes:?}");

    let edges = q.get_all_edges().await.unwrap();
    assert!(
        edges.iter().all(|e| e.source != e.target),
        "self-loop edge written: {:?}",
        edges
            .iter()
            .filter(|e| e.source == e.target)
            .map(|e| &e.source)
            .collect::<Vec<_>>()
    );
    let federated: Vec<_> = edges
        .iter()
        .filter(|e| {
            e.kind == EdgeKind::References
                && e.metadata
                    .as_ref()
                    .and_then(|m| m.get("reason"))
                    .and_then(|v| v.as_str())
                    == Some("shared_contract_operation")
        })
        .collect();
    assert_eq!(federated.len(), 1, "{federated:?}");
    let ids: Vec<&str> = routes.iter().map(|(id, _)| id.as_str()).collect();
    assert!(ids.contains(&federated[0].source.as_str()));
    assert!(ids.contains(&federated[0].target.as_str()));

    db.close().await;
    let _ = fs::remove_dir_all(&root);
}

fn route_node(id: &str, file: &str) -> ax_types::Node {
    ax_types::Node {
        id: id.into(),
        kind: ax_types::NodeKind::Route,
        name: "GET /users".into(),
        qualified_name: id.into(),
        file_path: file.into(),
        language: ax_types::Language::Unknown,
        start_line: 1,
        end_line: 1,
        start_column: 0,
        end_column: 0,
        docstring: None,
        signature: None,
        visibility: None,
        is_exported: Some(true),
        is_async: None,
        is_static: None,
        is_abstract: None,
        decorators: None,
        type_parameters: None,
        return_type: None,
        updated_at: 0,
    }
}

#[tokio::test]
async fn reindex_removes_contract_nodes_it_no_longer_writes() {
    let root = temp_root("stale");
    fs::write(
        root.join("openapi.yaml"),
        "openapi: 3.0.0\npaths:\n  /users:\n    get:\n      operationId: listUsers\n",
    )
    .unwrap();
    let db_path = root.join(".ax/ax.db");
    fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let db = Database::open(&db_path).await.unwrap();
    let q = QueryBuilder::new(db.pool().clone());

    // An old-format id without the file, one from a deleted contract file,
    // and a References edge between them, as an older extractor wrote them.
    q.upsert_nodes(&[
        route_node("contract:openapi:GET:/users", "openapi.yaml"),
        route_node("contract:openapi:gone.yaml:GET:/users", "gone.yaml"),
    ])
    .await
    .unwrap();
    q.upsert_edges(&[Edge {
        source: "contract:openapi:GET:/users".into(),
        target: "contract:openapi:gone.yaml:GET:/users".into(),
        kind: EdgeKind::References,
        metadata: None,
        line: None,
        column: None,
        provenance: None,
        confidence: None,
    }])
    .await
    .unwrap();

    let written = index_contracts(&root, &q, &[]).await.unwrap();
    assert_eq!(written, 1);

    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM nodes WHERE id LIKE 'contract:%'")
        .fetch_all(db.pool())
        .await
        .unwrap();
    assert_eq!(
        ids,
        vec!["contract:openapi:openapi.yaml:GET:/users".to_string()]
    );
    let edges = q.get_all_edges().await.unwrap();
    assert!(
        edges
            .iter()
            .all(|e| e.source != "contract:openapi:GET:/users"
                && e.target != "contract:openapi:gone.yaml:GET:/users"),
        "edges of stale contract nodes remain: {edges:?}"
    );

    db.close().await;
    let _ = fs::remove_dir_all(&root);
}
