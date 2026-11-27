use std::fs;
use std::path::PathBuf;

use ax_db::queries::QueryBuilder;
use ax_db::Database;
use ax_extraction::contracts::index_contracts;
use ax_types::EdgeKind;

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
