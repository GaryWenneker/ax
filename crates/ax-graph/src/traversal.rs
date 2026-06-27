//! DB-backed graph traversal.

use std::collections::{HashMap, HashSet, VecDeque};

use ax_db::queries::QueryBuilder;
use ax_types::{
    Edge, EdgeKind, Node, Subgraph, TraversalDirection, TraversalOptions,
};

pub struct GraphTraverser {
    queries: QueryBuilder,
}

impl GraphTraverser {
    pub fn new(queries: QueryBuilder) -> Self {
        Self { queries }
    }

    pub async fn traverse_bfs(
        &self,
        start_id: &str,
        options: TraversalOptions,
    ) -> Result<Subgraph, ax_utils::errors::AxError> {
        let opts = merge_options(options);
        let start = self.queries.get_node_by_id(start_id).await?;
        if start.is_none() {
            return Ok(Subgraph::default());
        }
        let start_node = start.unwrap();

        let mut nodes = HashMap::new();
        let mut edges = Vec::new();
        let mut visited = HashSet::new();
        let mut queue: VecDeque<(Node, Edge, u32)> = VecDeque::new();

        if opts.include_start.unwrap_or(true) {
            nodes.insert(start_node.id.clone(), start_node.clone());
        }
        queue.push_back((start_node, Edge {
            source: String::new(), target: String::new(), kind: EdgeKind::Contains,
            metadata: None, line: None, column: None, provenance: None,
        }, 0));

        while let Some((node, edge, depth)) = queue.pop_front() {
            if visited.contains(&node.id) {
                continue;
            }
            if nodes.len() >= opts.limit.unwrap_or(1000) as usize {
                continue;
            }
            visited.insert(node.id.clone());
            nodes.insert(node.id.clone(), node.clone());
            if edge.source != edge.target && edge.source.is_empty() == false || edge.target.is_empty() == false {
                if !edge.source.is_empty() {
                    edges.push(edge.clone());
                }
            }

            if depth >= opts.max_depth.unwrap_or(u32::MAX) {
                continue;
            }

            let edge_kinds = opts.edge_kinds.as_deref();
            let direction = opts.direction.unwrap_or(TraversalDirection::Outgoing);
            let follow_outgoing = matches!(
                direction,
                TraversalDirection::Outgoing | TraversalDirection::Both
            );
            let follow_incoming = matches!(
                direction,
                TraversalDirection::Incoming | TraversalDirection::Both
            );

            if follow_outgoing {
                let outgoing = self.queries.get_outgoing_edges(&node.id, edge_kinds).await?;
                for e in outgoing {
                    if let Some(target) = self.queries.get_node_by_id(&e.target).await? {
                        if filter_node(&target, &opts) {
                            queue.push_back((target, e, depth + 1));
                        }
                    }
                }
            }

            if follow_incoming {
                let incoming = self.queries.get_incoming_edges(&node.id).await?;
                let incoming: Vec<Edge> = if let Some(kinds) = edge_kinds {
                    incoming.into_iter().filter(|e| kinds.contains(&e.kind)).collect()
                } else {
                    incoming
                };
                for e in incoming {
                    if let Some(source) = self.queries.get_node_by_id(&e.source).await? {
                        if filter_node(&source, &opts) {
                            queue.push_back((source, e, depth + 1));
                        }
                    }
                }
            }
        }

        Ok(Subgraph {
            nodes,
            edges,
            roots: vec![start_id.to_string()],
            confidence: None,
        })
    }

    pub async fn get_callers(&self, node_id: &str, depth: u32) -> Result<Vec<Node>, ax_utils::errors::AxError> {
        let opts = TraversalOptions {
            max_depth: Some(depth),
            edge_kinds: Some(vec![EdgeKind::Calls]),
            direction: Some(TraversalDirection::Incoming),
            ..Default::default()
        };
        let sg = self.traverse_bfs(node_id, opts).await?;
        Ok(sg.nodes.values().filter(|n| n.id != node_id).cloned().collect())
    }

    pub async fn get_callees(&self, node_id: &str, depth: u32) -> Result<Vec<Node>, ax_utils::errors::AxError> {
        let opts = TraversalOptions {
            max_depth: Some(depth),
            edge_kinds: Some(vec![EdgeKind::Calls, EdgeKind::References]),
            direction: Some(TraversalDirection::Outgoing),
            ..Default::default()
        };
        let sg = self.traverse_bfs(node_id, opts).await?;
        Ok(sg.nodes.values().filter(|n| n.id != node_id).cloned().collect())
    }

    pub async fn get_impact_radius(&self, node_id: &str, depth: u32) -> Result<Subgraph, ax_utils::errors::AxError> {
        let opts = TraversalOptions {
            max_depth: Some(depth),
            direction: Some(TraversalDirection::Incoming),
            ..Default::default()
        };
        self.traverse_bfs(node_id, opts).await
    }

    /// CG: reverse callers — nodes that depend on `node_id` via Calls/References/Imports.
    pub async fn get_dependents(&self, node_id: &str, depth: u32) -> Result<Vec<Node>, ax_utils::errors::AxError> {
        let opts = TraversalOptions {
            max_depth: Some(depth),
            edge_kinds: Some(vec![EdgeKind::Calls, EdgeKind::References, EdgeKind::Imports]),
            direction: Some(TraversalDirection::Incoming),
            ..Default::default()
        };
        let sg = self.traverse_bfs(node_id, opts).await?;
        Ok(sg.nodes.values().filter(|n| n.id != node_id).cloned().collect())
    }

    /// CG: `getImpactSubgraph` — full incoming subgraph for blast-radius.
    pub async fn get_impact_subgraph(&self, node_id: &str, depth: u32) -> Result<Subgraph, ax_utils::errors::AxError> {
        self.get_impact_radius(node_id, depth).await
    }

    pub async fn get_ancestors(&self, node_id: &str) -> Result<Vec<Node>, ax_utils::errors::AxError> {
        let opts = TraversalOptions {
            edge_kinds: Some(vec![EdgeKind::Contains]),
            direction: Some(TraversalDirection::Incoming),
            ..Default::default()
        };
        let sg = self.traverse_bfs(node_id, opts).await?;
        Ok(sg.nodes.values().filter(|n| n.id != node_id).cloned().collect())
    }

    pub async fn get_children(&self, node_id: &str) -> Result<Vec<Node>, ax_utils::errors::AxError> {
        let edges = self.queries.get_outgoing_edges(node_id, Some(&[EdgeKind::Contains])).await?;
        let mut children = Vec::new();
        for e in edges {
            if let Some(n) = self.queries.get_node_by_id(&e.target).await? {
                children.push(n);
            }
        }
        Ok(children)
    }
}

fn merge_options(options: TraversalOptions) -> TraversalOptions {
    let default = TraversalOptions::default();
    TraversalOptions {
        max_depth: options.max_depth.or(default.max_depth),
        edge_kinds: options.edge_kinds.or(default.edge_kinds),
        node_kinds: options.node_kinds.or(default.node_kinds),
        direction: options.direction.or(default.direction),
        limit: options.limit.or(default.limit),
        include_start: options.include_start.or(default.include_start),
    }
}

fn filter_node(node: &Node, opts: &TraversalOptions) -> bool {
    if let Some(kinds) = &opts.node_kinds {
        return kinds.contains(&node.kind);
    }
    true
}
