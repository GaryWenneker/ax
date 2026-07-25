//! Reverse BFS from changed symbols to test nodes.

use std::collections::{HashSet};

use ax_db::queries::QueryBuilder;
use ax_graph::GraphTraverser;
use ax_types::{EdgeKind, NodeKind, TraversalDirection, TraversalOptions};
use sqlx::SqlitePool;

use crate::options::{is_test_path, TiaOptions};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImpactedTest {
    pub id: String,
    pub name: String,
    pub file_path: String,
    pub qualified_name: String,
    pub runner_hint: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TiaResult {
    pub tests: Vec<ImpactedTest>,
    pub test_files: Vec<String>,
    pub dirty_node_count: usize,
}

pub async fn find_impacted_tests(
    pool: &SqlitePool,
    dirty_node_ids: &[String],
    opts: &TiaOptions,
) -> Result<TiaResult, ax_utils::errors::AxError> {
    let queries = QueryBuilder::new(pool.clone());
    let traverser = GraphTraverser::new(QueryBuilder::new(pool.clone()));

    let mut test_ids = HashSet::new();
    let mut test_files = HashSet::new();

    for node_id in dirty_node_ids {
        let sg = traverser
            .traverse_bfs(
                node_id,
                TraversalOptions {
                    direction: Some(TraversalDirection::Incoming),
                    edge_kinds: Some(vec![EdgeKind::Covers, EdgeKind::Calls, EdgeKind::Imports]),
                    max_depth: Some(opts.depth),
                    include_start: Some(false),
                    ..Default::default()
                },
            )
            .await?;

        for node in sg.nodes.values() {
            if node.kind == NodeKind::Test {
                test_ids.insert(node.id.clone());
                test_files.insert(node.file_path.clone());
            } else if opts.include_test_files && is_test_path(&node.file_path, &opts.filter) {
                if matches!(node.kind, NodeKind::Function | NodeKind::Method) {
                    test_ids.insert(node.id.clone());
                }
                test_files.insert(node.file_path.clone());
            }
        }
    }

    let mut tests = Vec::new();
    for id in &test_ids {
        if let Some(node) = queries.get_node_by_id(id).await? {
            if opts.filter.as_ref().is_some_and(|g| !g.is_match(&node.file_path)) {
                continue;
            }
            tests.push(ImpactedTest {
                id: node.id.clone(),
                name: node.name.clone(),
                file_path: node.file_path.clone(),
                qualified_name: node.qualified_name.clone(),
                runner_hint: runner_hint_for(&node),
            });
        }
    }
    tests.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));

    let mut files: Vec<String> = test_files.into_iter().collect();
    files.sort();

    Ok(TiaResult {
        tests,
        test_files: files,
        dirty_node_count: dirty_node_ids.len(),
    })
}

pub async fn affected_files_from_changes(
    pool: &SqlitePool,
    changed_files: &[String],
    opts: &TiaOptions,
) -> Result<TiaResult, ax_utils::errors::AxError> {
    let queries = QueryBuilder::new(pool.clone());
    let mut dirty_ids = Vec::new();

    for path in changed_files {
        if is_test_path(path, &opts.filter) {
            continue;
        }
        let nodes = queries.get_nodes_by_file(path).await?;
        for node in nodes {
            if matches!(
                node.kind,
                NodeKind::Function
                    | NodeKind::Method
                    | NodeKind::Class
                    | NodeKind::Struct
                    | NodeKind::Trait
            ) {
                dirty_ids.push(node.id.clone());
            }
        }
    }

    find_impacted_tests(pool, &dirty_ids, opts).await
}

fn runner_hint_for(node: &ax_types::Node) -> String {
    let path = &node.file_path;
    if path.ends_with(".rs") {
        format!("cargo test {} -- --exact", node.name)
    } else if path.ends_with(".ts") || path.ends_with(".tsx") || path.ends_with(".js") || path.ends_with(".jsx") {
        // Prefer jest when the path looks like a Jest suite; otherwise Vitest.
        if path.contains("__tests__") || path.contains(".test.") || path.contains(".spec.") {
            format!("npx jest -t {}", node.name)
        } else {
            format!("npx vitest run -t {}", node.name)
        }
    } else if path.ends_with(".py") {
        format!("pytest -k {}", node.name)
    } else if path.ends_with("_test.go") || path.ends_with(".go") {
        format!("go test -run {}", node.name)
    } else {
        node.name.clone()
    }
}
