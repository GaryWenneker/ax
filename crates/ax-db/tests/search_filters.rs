use ax_db::queries::QueryBuilder;
use ax_db::Database;
use ax_types::{Language, Node, NodeKind, SearchOptions};

fn node(id: &str, kind: NodeKind, name: &str, file: &str, language: Language) -> Node {
    Node {
        id: id.to_string(),
        kind,
        name: name.to_string(),
        qualified_name: name.to_string(),
        file_path: file.to_string(),
        language,
        start_line: 1,
        end_line: 2,
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
        updated_at: 0,
    }
}

#[tokio::test]
async fn exact_name_supplement_respects_kind_language_and_path_filters() {
    let root = tempfile::tempdir().unwrap();
    let db = Database::open(&root.path().join("ax.db")).await.unwrap();
    let q = QueryBuilder::new(db.pool().clone());
    q.upsert_nodes(&[
        node(
            "f-runner",
            NodeKind::Function,
            "runner",
            "src/lib.rs",
            Language::Rust,
        ),
        node(
            "f-run",
            NodeKind::Function,
            "run",
            "src/main.rs",
            Language::Rust,
        ),
        node(
            "s-run",
            NodeKind::Struct,
            "run",
            "src/types.rs",
            Language::Rust,
        ),
        node(
            "t-run",
            NodeKind::Function,
            "run",
            "tests/it.rs",
            Language::Rust,
        ),
        node(
            "py-run",
            NodeKind::Function,
            "run",
            "src/run.py",
            Language::Python,
        ),
    ])
    .await
    .unwrap();

    let opts = SearchOptions {
        kinds: Some(vec![NodeKind::Function]),
        languages: Some(vec![Language::Rust]),
        exclude_patterns: Some(vec!["tests/".to_string()]),
        limit: Some(20),
        ..Default::default()
    };
    let results = q.search_nodes("run", &opts).await.unwrap();
    let ids: Vec<&str> = results.iter().map(|r| r.node.id.as_str()).collect();
    assert!(ids.contains(&"f-run"), "{ids:?}");
    assert!(ids.contains(&"f-runner"), "{ids:?}");
    for excluded in ["s-run", "t-run", "py-run"] {
        assert!(!ids.contains(&excluded), "{excluded} leaked: {ids:?}");
    }

    let only_src = SearchOptions {
        include_patterns: Some(vec!["src/lib.rs".to_string()]),
        limit: Some(20),
        ..Default::default()
    };
    let results = q.search_nodes("runner run", &only_src).await.unwrap();
    assert!(
        results.iter().all(|r| r.node.file_path == "src/lib.rs"),
        "{:?}",
        results.iter().map(|r| &r.node.id).collect::<Vec<_>>()
    );
    db.close().await;
}
