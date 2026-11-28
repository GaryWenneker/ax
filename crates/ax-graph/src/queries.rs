//! Higher-level graph queries.

use ax_db::queries::QueryBuilder;
use ax_types::{Context, EdgeKind, NodeRef};

use crate::traversal::GraphTraverser;

pub struct GraphQueryManager {
    queries: QueryBuilder,
    traverser: GraphTraverser,
}

impl GraphQueryManager {
    pub fn new(queries: QueryBuilder) -> Self {
        let pool = queries.pool().clone();
        Self {
            queries,
            traverser: GraphTraverser::new(QueryBuilder::new(pool)),
        }
    }

    pub async fn get_context(&self, node_id: &str) -> Result<Context, ax_utils::errors::AxError> {
        let focal = self.queries.get_node_by_id(node_id).await?.ok_or_else(|| {
            ax_utils::errors::AxError::Other(format!("Node not found: {}", node_id))
        })?;

        let ancestors = self.traverser.get_ancestors(node_id).await?;
        let children = self.traverser.get_children(node_id).await?;

        let incoming_edges = self.queries.get_incoming_edges(node_id).await?;
        let mut incoming_refs = Vec::new();
        for edge in incoming_edges {
            if edge.kind == EdgeKind::Contains {
                continue;
            }
            if let Some(node) = self.queries.get_node_by_id(&edge.source).await? {
                incoming_refs.push(NodeRef { node, edge });
            }
        }

        let outgoing_edges = self.queries.get_outgoing_edges(node_id, None).await?;
        let mut outgoing_refs = Vec::new();
        for edge in outgoing_edges {
            if edge.kind == EdgeKind::Contains {
                continue;
            }
            if let Some(node) = self.queries.get_node_by_id(&edge.target).await? {
                outgoing_refs.push(NodeRef { node, edge });
            }
        }

        Ok(Context {
            focal,
            ancestors,
            children,
            incoming_refs,
            outgoing_refs,
            types: vec![],
            imports: vec![],
        })
    }

    /// Unreferenced non-exported functions, methods and classes: no edge
    /// other than `contains` points at them. One anti-join query per page of
    /// `DEAD_CODE_PAGE` rows, read until the last page, so nothing is cut off.
    pub async fn find_dead_code(&self) -> Result<Vec<ax_types::Node>, ax_utils::errors::AxError> {
        use ax_types::NodeKind;

        const DEAD_CODE_PAGE: u32 = 5_000;
        let kinds = [NodeKind::Function, NodeKind::Method, NodeKind::Class];
        let mut dead = Vec::new();
        loop {
            let page = self
                .queries
                .get_unreferenced_nodes(&kinds, DEAD_CODE_PAGE, dead.len() as u64)
                .await?;
            let last = page.len() < DEAD_CODE_PAGE as usize;
            dead.extend(page);
            if last {
                return Ok(dead);
            }
        }
    }

    pub async fn get_affected_tests(
        &self,
        changed_files: &[String],
    ) -> Result<Vec<String>, ax_utils::errors::AxError> {
        use crate::query_utils::is_test_file;
        use std::collections::HashSet;

        let mut affected = HashSet::new();
        for path in changed_files {
            if is_test_file(path) {
                affected.insert(path.clone());
            }
            let nodes = self.queries.get_nodes_by_file(path).await?;
            for node in nodes {
                let sg = self.traverser.get_impact_radius(&node.id, 2).await?;
                for n in sg.nodes.values() {
                    if is_test_file(&n.file_path) {
                        affected.insert(n.file_path.clone());
                    }
                }
            }
        }
        Ok(affected.into_iter().collect())
    }

    pub async fn get_dependents(
        &self,
        node_id: &str,
        depth: u32,
    ) -> Result<Vec<ax_types::Node>, ax_utils::errors::AxError> {
        self.traverser.get_dependents(node_id, depth).await
    }

    pub async fn get_impact_subgraph(
        &self,
        node_id: &str,
        depth: u32,
    ) -> Result<ax_types::Subgraph, ax_utils::errors::AxError> {
        self.traverser.get_impact_subgraph(node_id, depth).await
    }

    /// Run whole-graph analysis (communities, god nodes, surprising edges) and
    /// persist the community assignments. `resolution` tunes cluster granularity.
    pub async fn compute_insights(
        &self,
        resolution: f64,
        god_limit: usize,
        surprising_limit: usize,
    ) -> Result<crate::analysis::GraphInsights, ax_utils::errors::AxError> {
        let nodes = self.queries.get_all_nodes().await?;
        let edges = self.queries.get_all_edges().await?;
        let (insights, assignments) = crate::analysis::compute_insights(
            &nodes,
            &edges,
            resolution,
            god_limit,
            surprising_limit,
        );
        let rows: Vec<(String, i64, Option<String>)> = assignments
            .into_iter()
            .map(|a| (a.node_id, a.community_id, a.label))
            .collect();
        self.queries.replace_node_communities(&rows).await?;
        Ok(insights)
    }
}
