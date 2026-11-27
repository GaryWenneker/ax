//! The framework post-extract pass adds the right nodes, and only those.

use ax_db::queries::QueryBuilder;
use ax_db::Database;
use ax_resolution::frameworks::extract::stable_node_id;
use ax_resolution::ReferenceResolver;
use ax_types::{FileRecord, Language, Node, NodeKind};

fn file(path: &str, language: Language) -> FileRecord {
    FileRecord {
        path: path.into(),
        content_hash: "h".into(),
        language,
        size: 1,
        modified_at: 0,
        indexed_at: 0,
        node_count: 1,
        errors: None,
    }
}

async fn open(root: &std::path::Path) -> QueryBuilder {
    let db = Database::open(&root.join("ax.db")).await.unwrap();
    QueryBuilder::new(db.pool().clone())
}

/// Write `path` on disk with `content` and register it as an indexed file.
async fn add(q: &QueryBuilder, root: &std::path::Path, path: &str, content: &str, lang: Language) {
    let full = root.join(path);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(full, content).unwrap();
    q.upsert_file(&file(path, lang)).await.unwrap();
    // The post-extract pass hangs its nodes under the file node.
    q.upsert_nodes(&[node(&stable_node_id(path, path), NodeKind::File, path, path, lang)])
        .await
        .unwrap();
}

fn node(id: &str, kind: NodeKind, name: &str, file: &str, language: Language) -> Node {
    Node {
        id: id.into(),
        kind,
        name: name.into(),
        qualified_name: format!("{file}::{name}"),
        file_path: file.into(),
        language,
        start_line: 1,
        end_line: 5,
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

async fn nodes_in(q: &QueryBuilder, path: &str) -> Vec<Node> {
    q.get_nodes_by_file(path).await.unwrap()
}

#[tokio::test]
async fn sveltekit_routes_reach_the_graph() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("package.json"),
        r#"{"devDependencies":{"@sveltejs/kit":"2.0.0","svelte":"5.0.0"}}"#,
    )
    .unwrap();
    let q = open(root.path()).await;
    let page = "src/routes/blog/[slug]/+page.svelte";
    add(&q, root.path(), page, "<h1>post</h1>\n", Language::Svelte).await;

    ReferenceResolver::new(root.path())
        .resolve_all(&q, None, None)
        .await
        .unwrap();

    let routes: Vec<String> = nodes_in(&q, page)
        .await
        .into_iter()
        .filter(|n| n.kind == NodeKind::Route)
        .map(|n| n.name)
        .collect();
    assert_eq!(routes, vec!["/blog/:slug".to_string()]);
}
