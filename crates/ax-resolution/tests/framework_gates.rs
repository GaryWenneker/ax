//! Framework resolvers only act on projects and languages they belong to.

use ax_db::queries::QueryBuilder;
use ax_db::Database;
use ax_resolution::ReferenceResolver;
use ax_types::{FileRecord, Language, Node, NodeKind, ReferenceKind, UnresolvedReference};

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

/// Targets that a framework resolver claimed for one ref.
async fn framework_targets(
    root: &std::path::Path,
    q: &QueryBuilder,
    name: &str,
    file_path: &str,
    language: Language,
) -> Vec<String> {
    q.insert_unresolved_ref(&UnresolvedReference {
        from_node_id: "caller".into(),
        reference_name: name.into(),
        reference_kind: ReferenceKind::Calls,
        line: 3,
        column: 4,
        file_path: Some(file_path.into()),
        language: Some(language),
        candidates: None,
    })
    .await
    .unwrap();
    let mut resolver = ReferenceResolver::new(root);
    let result = resolver.resolve_all(q, None, None).await.unwrap();
    result
        .resolved
        .into_iter()
        .filter(|r| r.resolved_by == ax_resolution::ResolvedBy::Framework)
        .map(|r| r.target_node_id)
        .collect()
}

#[tokio::test]
async fn flask_resolver_does_not_claim_rust_calls() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Cargo.toml"), "[package]\nname = \"a\"\n").unwrap();
    std::fs::create_dir_all(root.path().join("src")).unwrap();
    std::fs::write(
        root.path().join("src/main.rs"),
        "fn main() {\n\n    get_user();\n}\n",
    )
    .unwrap();
    let q = open(root.path()).await;
    q.upsert_file(&file("src/main.rs", Language::Rust))
        .await
        .unwrap();
    q.upsert_file(&file("tools/helpers.py", Language::Python))
        .await
        .unwrap();
    q.upsert_nodes(&[
        node(
            "caller",
            NodeKind::Function,
            "main",
            "src/main.rs",
            Language::Rust,
        ),
        node(
            "py-get-user",
            NodeKind::Function,
            "get_user",
            "tools/helpers.py",
            Language::Python,
        ),
    ])
    .await
    .unwrap();
    let targets =
        framework_targets(root.path(), &q, "get_user", "src/main.rs", Language::Rust).await;
    assert!(
        targets.is_empty(),
        "a framework resolver claimed a Rust call: {targets:?}"
    );
}

#[tokio::test]
async fn django_resolver_needs_a_django_project() {
    // A Java project with two `User` classes: no framework may pick one.
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("pom.xml"), "<project/>").unwrap();
    let q = open(root.path()).await;
    let files = [
        "src/main/java/App.java",
        "src/main/java/a/models/User.java",
        "src/main/java/b/models/User.java",
    ];
    for p in files {
        q.upsert_file(&file(p, Language::Java)).await.unwrap();
    }
    q.upsert_nodes(&[
        node(
            "caller",
            NodeKind::Function,
            "main",
            files[0],
            Language::Java,
        ),
        node("user-a", NodeKind::Class, "User", files[1], Language::Java),
        node("user-b", NodeKind::Class, "User", files[2], Language::Java),
    ])
    .await
    .unwrap();
    let targets = framework_targets(root.path(), &q, "User", files[0], Language::Java).await;
    assert!(
        targets.is_empty(),
        "a framework resolver claimed User: {targets:?}"
    );
}

#[tokio::test]
async fn django_model_resolves_only_when_one_candidate_is_in_a_model_dir() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("manage.py"), "").unwrap();
    let q = open(root.path()).await;
    let files = [
        "shop/views.py",
        "shop/models.py",
        "billing/models.py",
        "shop/admin.py",
    ];
    for p in files {
        q.upsert_file(&file(p, Language::Python)).await.unwrap();
    }
    q.upsert_nodes(&[
        node(
            "caller",
            NodeKind::Function,
            "index",
            files[0],
            Language::Python,
        ),
        node(
            "order",
            NodeKind::Class,
            "Order",
            files[1],
            Language::Python,
        ),
        node(
            "invoice-a",
            NodeKind::Class,
            "Invoice",
            files[1],
            Language::Python,
        ),
        node(
            "invoice-b",
            NodeKind::Class,
            "Invoice",
            files[2],
            Language::Python,
        ),
        node(
            "audit",
            NodeKind::Class,
            "Audit",
            files[3],
            Language::Python,
        ),
    ])
    .await
    .unwrap();
    let order = framework_targets(root.path(), &q, "Order", files[0], Language::Python).await;
    assert_eq!(order, vec!["order".to_string()]);
    // Two models named Invoice: ambiguous, so the framework resolver steps aside.
    let invoice = framework_targets(root.path(), &q, "Invoice", files[0], Language::Python).await;
    assert!(invoice.is_empty(), "{invoice:?}");
    // Audit is not in a model directory: no arbitrary fallback.
    let audit = framework_targets(root.path(), &q, "Audit", files[0], Language::Python).await;
    assert!(audit.is_empty(), "{audit:?}");
}

#[tokio::test]
async fn flask_dependency_still_resolves_in_a_flask_project() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("requirements.txt"), "flask==3.0\n").unwrap();
    let q = open(root.path()).await;
    for p in ["app/routes.py", "app/deps/users.py"] {
        q.upsert_file(&file(p, Language::Python)).await.unwrap();
    }
    q.upsert_nodes(&[
        node(
            "caller",
            NodeKind::Function,
            "index",
            "app/routes.py",
            Language::Python,
        ),
        node(
            "dep",
            NodeKind::Function,
            "get_user",
            "app/deps/users.py",
            Language::Python,
        ),
    ])
    .await
    .unwrap();
    let targets = framework_targets(
        root.path(),
        &q,
        "get_user",
        "app/routes.py",
        Language::Python,
    )
    .await;
    assert_eq!(targets, vec!["dep".to_string()]);
}
