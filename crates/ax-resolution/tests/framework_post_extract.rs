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
    q.upsert_nodes(&[node(
        &stable_node_id(path, path),
        NodeKind::File,
        path,
        path,
        lang,
    )])
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

async fn call(q: &QueryBuilder, from: &str, name: &str, file_path: &str, language: Language) {
    q.insert_unresolved_ref(&ax_types::UnresolvedReference {
        from_node_id: from.into(),
        reference_name: name.into(),
        reference_kind: ax_types::ReferenceKind::Calls,
        line: 2,
        column: 4,
        file_path: Some(file_path.into()),
        language: Some(language),
        candidates: None,
    })
    .await
    .unwrap();
}

async fn self_loops(q: &QueryBuilder) -> Vec<String> {
    q.get_all_edges()
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.source == e.target)
        .map(|e| format!("{} {:?}", e.source, e.kind))
        .collect()
}

async fn open_refs(q: &QueryBuilder) -> Vec<String> {
    q.get_unresolved_refs()
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.reference_name)
        .collect()
}

#[tokio::test]
async fn compiler_macros_are_dropped_without_a_self_edge() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("package.json"),
        r#"{"dependencies":{"vue":"3.4.0","svelte":"5.0.0"}}"#,
    )
    .unwrap();
    let q = open(root.path()).await;
    let vue = "src/components/Card.vue";
    let svelte = "src/lib/Counter.svelte";
    add(
        &q,
        root.path(),
        vue,
        "<script setup>defineProps()</script>\n",
        Language::Vue,
    )
    .await;
    add(
        &q,
        root.path(),
        svelte,
        "<script>let n = $state(0)</script>\n",
        Language::Svelte,
    )
    .await;
    q.upsert_nodes(&[
        node("card", NodeKind::Component, "Card", vue, Language::Vue),
        node(
            "counter",
            NodeKind::Component,
            "Counter",
            svelte,
            Language::Svelte,
        ),
    ])
    .await
    .unwrap();
    call(&q, "card", "defineProps", vue, Language::Vue).await;
    call(&q, "card", "useRoute", vue, Language::Vue).await;
    call(&q, "counter", "$state", svelte, Language::Svelte).await;

    ReferenceResolver::new(root.path())
        .resolve_all(&q, None, None)
        .await
        .unwrap();

    assert_eq!(self_loops(&q).await, Vec::<String>::new());
    assert_eq!(open_refs(&q).await, Vec::<String>::new());
}

#[tokio::test]
async fn vue_auto_imports_are_left_alone_outside_vue_projects() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("package.json"),
        r#"{"dependencies":{"react":"18.3.1"}}"#,
    )
    .unwrap();
    let q = open(root.path()).await;
    let app = "src/App.tsx";
    add(
        &q,
        root.path(),
        app,
        "export function App() { useState(0) }\n",
        Language::Tsx,
    )
    .await;
    q.upsert_nodes(&[node("app", NodeKind::Function, "App", app, Language::Tsx)])
        .await
        .unwrap();
    call(&q, "app", "useState", app, Language::Tsx).await;

    ReferenceResolver::new(root.path())
        .resolve_all(&q, None, None)
        .await
        .unwrap();

    assert_eq!(self_loops(&q).await, Vec::<String>::new());
    assert_eq!(open_refs(&q).await, vec!["useState".to_string()]);
}

#[tokio::test]
async fn scoped_runs_reuse_framework_detection() {
    use ax_resolution::frameworks::FrameworkRegistry;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("pom.xml"), "<project/>").unwrap();
    let q = open(root.path()).await;
    let a = "src/main/java/A.java";
    let b = "src/main/java/B.java";
    add(&q, root.path(), a, "class A {}\n", Language::Java).await;
    add(&q, root.path(), b, "class B {}\n", Language::Java).await;

    let registry = FrameworkRegistry::new();
    let gates = registry
        .run_post_extract(root.path(), &q, None)
        .await
        .unwrap();
    assert!(!gates.spring);

    // B changes on disk, but a sync of A alone must not rescan every file.
    std::fs::write(root.path().join(b), "@RestController\nclass B {}\n").unwrap();
    let only_a = vec![a.to_string()];
    let gates = registry
        .run_post_extract(root.path(), &q, Some(&only_a))
        .await
        .unwrap();
    assert!(
        !gates.spring,
        "a scoped run re-read files outside its scope"
    );

    // A sync of B itself does see the new annotation.
    let only_b = vec![b.to_string()];
    let gates = registry
        .run_post_extract(root.path(), &q, Some(&only_b))
        .await
        .unwrap();
    assert!(gates.spring);

    // A fresh registry has nothing cached, so it detects over all files.
    let gates = FrameworkRegistry::new()
        .run_post_extract(root.path(), &q, Some(&only_a))
        .await
        .unwrap();
    assert!(gates.spring);
}

#[tokio::test]
async fn real_recursion_keeps_its_self_edge() {
    let root = tempfile::tempdir().unwrap();
    let q = open(root.path()).await;
    let py = "src/walk.py";
    add(
        &q,
        root.path(),
        py,
        "class Node:\n    def visit(self, n):\n        n.visit(n)\n",
        Language::Python,
    )
    .await;
    q.upsert_nodes(&[node(
        "visit",
        NodeKind::Method,
        "visit",
        py,
        Language::Python,
    )])
    .await
    .unwrap();
    call(&q, "visit", "visit", py, Language::Python).await;

    ReferenceResolver::new(root.path())
        .resolve_all(&q, None, None)
        .await
        .unwrap();

    assert_eq!(self_loops(&q).await, vec!["visit Calls".to_string()]);
    assert_eq!(open_refs(&q).await, Vec::<String>::new());
}
