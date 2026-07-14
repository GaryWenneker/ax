//! Type-flow warnings, business rules, and route/SQL flow tracing.

use ax_db::queries::QueryBuilder;
use ax_graph::GraphTraverser;
use ax_types::{EdgeKind, NodeKind, TraversalDirection, TraversalOptions};
use sqlx::SqlitePool;

use crate::state::{BreakingWarning, BusinessRuleWarning};

pub async fn detect_breaking_changes(
    pool: &SqlitePool,
    dirty_nodes: &[ax_git::DirtyNode],
) -> Result<Vec<BreakingWarning>, ax_utils::errors::AxError> {
    let queries = QueryBuilder::new(pool.clone());
    let mut warnings = Vec::new();

    for dirty in dirty_nodes {
        let node = match queries.get_node_by_id(&dirty.id).await? {
            Some(n) => n,
            None => continue,
        };
        if node.return_type.as_ref().is_some_and(|t| t.contains("Option<")) {
            let sg = GraphTraverser::new(QueryBuilder::new(pool.clone()))
                .traverse_bfs(
                    &node.id,
                    TraversalOptions {
                        direction: Some(TraversalDirection::Incoming),
                        edge_kinds: Some(vec![EdgeKind::Calls]),
                        max_depth: Some(3),
                        include_start: Some(false),
                        ..Default::default()
                    },
                )
                .await?;
            let callers: Vec<String> = sg
                .nodes
                .values()
                .map(|n| format!("{}:{}", n.file_path, n.name))
                .collect();
            if !callers.is_empty() {
                warnings.push(BreakingWarning {
                    node_id: node.id.clone(),
                    node_name: node.name.clone(),
                    reason: "Option type may require caller updates (unwrap/expect)".into(),
                    locations: callers,
                });
            }
        }
    }
    Ok(warnings)
}

pub async fn check_business_rules(
    pool: &SqlitePool,
    dirty_nodes: &[ax_git::DirtyNode],
) -> Result<Vec<BusinessRuleWarning>, ax_utils::errors::AxError> {
    let mut warnings = Vec::new();
    for dirty in dirty_nodes {
        let rows = sqlx::query_as::<_, (String, String, String)>(
            "SELECT id, rule_text, severity FROM business_rules WHERE node_id = ?",
        )
        .bind(&dirty.id)
        .fetch_all(pool)
        .await
        .map_err(|e| ax_utils::errors::AxError::Database(ax_utils::errors::DatabaseError::new(e.to_string())))?;

        for (id, rule_text, severity) in rows {
            warnings.push(BusinessRuleWarning {
                rule_id: id,
                rule_text,
                node_name: dirty.name.clone(),
                severity,
            });
        }
    }
    Ok(warnings)
}

pub async fn find_affected_routes(
    pool: &SqlitePool,
    dirty_nodes: &[ax_git::DirtyNode],
) -> Result<Vec<String>, ax_utils::errors::AxError> {
    let traverser = GraphTraverser::new(QueryBuilder::new(pool.clone()));
    let mut routes = std::collections::HashSet::new();

    for dirty in dirty_nodes {
        let sg = traverser
            .traverse_bfs(
                &dirty.id,
                TraversalOptions {
                    direction: Some(TraversalDirection::Incoming),
                    edge_kinds: Some(vec![EdgeKind::Calls, EdgeKind::References]),
                    max_depth: Some(6),
                    include_start: Some(false),
                    ..Default::default()
                },
            )
            .await?;
        for node in sg.nodes.values() {
            if node.kind == NodeKind::Route {
                routes.insert(node.name.clone());
            }
            if node.kind == NodeKind::Table {
                routes.insert(format!("table:{}", node.name));
            }
        }
    }
    Ok(routes.into_iter().collect())
}
