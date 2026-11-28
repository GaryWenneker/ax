//! JSX child edges must point at the component the file actually uses.

use ax_db::queries::QueryBuilder;
use ax_db::Database;
use ax_resolution::callback_synthesizer::CallbackSynthesizer;
use ax_types::{Edge, EdgeKind, FileRecord, Language, Node, NodeKind};

fn node(id: &str, kind: NodeKind, name: &str, file: &str, start: i32, end: i32) -> Node {
    Node {
        id: id.into(),
        kind,
        name: name.into(),
        qualified_name: format!("{file}::{name}"),
        file_path: file.into(),
        language: Language::Tsx,
        start_line: start,
        end_line: end,
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

fn file(path: &str) -> FileRecord {
    FileRecord {
        path: path.into(),
        content_hash: "h".into(),
        language: Language::Tsx,
        size: 1,
        modified_at: 0,
        indexed_at: 0,
        node_count: 1,
        errors: None,
    }
}

const PAGE: &str = "export function Page() {\n  return <Button label=\"x\" />;\n}\n";

/// Two `Button` components in different folders, and a page that renders one.
/// The admin one is stored first, so a "first match" lookup picks it.
async fn setup(root: &std::path::Path, page_defines_button: bool) -> QueryBuilder {
    setup_with(root, page_defines_button, true).await
}

async fn setup_with(
    root: &std::path::Path,
    page_defines_button: bool,
    with_admin_button: bool,
) -> QueryBuilder {
    let db = Database::open(&root.join("ax.db")).await.unwrap();
    let q = QueryBuilder::new(db.pool().clone());
    std::fs::create_dir_all(root.join("app")).unwrap();
    let mut page_src = PAGE.to_string();
    if page_defines_button {
        page_src.push_str("function Button() {\n  return null;\n}\n");
    }
    std::fs::write(root.join("app/Page.tsx"), &page_src).unwrap();
    for p in ["admin/Button.tsx", "ui/Button.tsx", "app/Page.tsx"] {
        q.upsert_file(&file(p)).await.unwrap();
    }
    let mut nodes = vec![
        node(
            "btn-admin",
            NodeKind::Component,
            "Button",
            "admin/Button.tsx",
            1,
            3,
        ),
        node(
            "btn-ui",
            NodeKind::Component,
            "Button",
            "ui/Button.tsx",
            1,
            3,
        ),
        node(
            "file-page",
            NodeKind::File,
            "Page.tsx",
            "app/Page.tsx",
            1,
            3,
        ),
        node("page", NodeKind::Function, "Page", "app/Page.tsx", 1, 3),
    ];
    if !with_admin_button {
        nodes.remove(0);
    }
    if page_defines_button {
        nodes.push(node(
            "btn-local",
            NodeKind::Function,
            "Button",
            "app/Page.tsx",
            4,
            6,
        ));
    }
    q.upsert_nodes(&nodes).await.unwrap();
    q
}

async fn import(q: &QueryBuilder, target: &str) {
    q.upsert_edge(&Edge {
        source: "file-page".into(),
        target: target.into(),
        kind: EdgeKind::Imports,
        metadata: None,
        line: Some(1),
        column: Some(0),
        provenance: None,
        confidence: None,
    })
    .await
    .unwrap();
}

async fn rendered_children(q: &QueryBuilder) -> Vec<String> {
    let mut out: Vec<String> = q
        .get_outgoing_edges("page", Some(&[EdgeKind::Calls]))
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.target)
        .collect();
    out.sort();
    out
}

#[tokio::test]
async fn jsx_tag_resolves_to_the_imported_component() {
    let root = tempfile::tempdir().unwrap();
    let q = setup(root.path(), false).await;
    import(&q, "btn-ui").await;
    CallbackSynthesizer::new()
        .synthesize(root.path(), &q, None)
        .await
        .unwrap();
    assert_eq!(rendered_children(&q).await, vec!["btn-ui".to_string()]);
}

#[tokio::test]
async fn jsx_tag_prefers_a_same_file_definition() {
    let root = tempfile::tempdir().unwrap();
    let q = setup(root.path(), true).await;
    CallbackSynthesizer::new()
        .synthesize(root.path(), &q, None)
        .await
        .unwrap();
    assert_eq!(rendered_children(&q).await, vec!["btn-local".to_string()]);
}

#[tokio::test]
async fn ambiguous_jsx_tag_without_an_import_gets_no_edge() {
    let root = tempfile::tempdir().unwrap();
    let q = setup(root.path(), false).await;
    CallbackSynthesizer::new()
        .synthesize(root.path(), &q, None)
        .await
        .unwrap();
    assert!(rendered_children(&q).await.is_empty());
}

#[tokio::test]
async fn a_unique_component_is_linked_without_an_import() {
    let root = tempfile::tempdir().unwrap();
    let q = setup_with(root.path(), false, false).await;
    CallbackSynthesizer::new()
        .synthesize(root.path(), &q, None)
        .await
        .unwrap();
    assert_eq!(rendered_children(&q).await, vec!["btn-ui".to_string()]);
}

#[tokio::test]
async fn jsx_source_comes_from_the_source_store() {
    let root = tempfile::tempdir().unwrap();
    let q = setup_with(root.path(), false, false).await;
    // The stored copy renders <Button/>; the file on disk no longer exists.
    q.upsert_file_content("app/Page.tsx", "h", PAGE)
        .await
        .unwrap();
    std::fs::remove_file(root.path().join("app/Page.tsx")).unwrap();
    CallbackSynthesizer::new()
        .synthesize(root.path(), &q, None)
        .await
        .unwrap();
    assert_eq!(rendered_children(&q).await, vec!["btn-ui".to_string()]);
}
