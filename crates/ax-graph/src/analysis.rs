//! Whole-graph analysis: community detection (Leiden), god-node ranking, and
//! surprising cross-community connections.
//!
//! These are pure, deterministic functions over an in-memory node/edge set.
//! Persistence and DB access live in [`crate::queries::GraphQueryManager`].

use std::collections::HashMap;

use ax_types::{Edge, EdgeKind, Node};
use leiden_rs::{GraphDataBuilder, Leiden, LeidenConfig};
use serde::{Deserialize, Serialize};

/// Edge kinds that represent a *semantic* relationship between concepts.
///
/// `Contains` (structural nesting) and `Exports` are excluded so that files do
/// not become artificial super-hubs that dominate both clustering and god-node
/// rankings.
///
/// `Documents` (doc -> code mentions) is included here so documentation nodes
/// stay connected in the graph visualization and export, but it is excluded
/// from the architecture math via [`is_architectural_edge`].
pub fn is_semantic_edge(kind: EdgeKind) -> bool {
    matches!(
        kind,
        EdgeKind::Calls
            | EdgeKind::References
            | EdgeKind::Imports
            | EdgeKind::Extends
            | EdgeKind::Implements
            | EdgeKind::Instantiates
            | EdgeKind::Overrides
            | EdgeKind::TypeOf
            | EdgeKind::Returns
            | EdgeKind::Decorates
            | EdgeKind::Documents
    )
}

/// Edge kinds that count toward *architecture* analysis: community detection,
/// god-node ranking, and surprising connections.
///
/// This is [`is_semantic_edge`] minus `Documents` — documentation mentions
/// should not distort subsystem clustering or make heavily-referenced docs look
/// like god nodes, even though they remain visible in the graph.
pub fn is_architectural_edge(kind: EdgeKind) -> bool {
    is_semantic_edge(kind) && !matches!(kind, EdgeKind::Documents)
}

/// One node's community assignment plus a human-readable label.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityAssignment {
    pub node_id: String,
    pub community_id: i64,
    pub label: Option<String>,
}

/// Summary of one detected community.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunitySummary {
    pub community_id: i64,
    pub label: String,
    pub size: usize,
    /// Highest-degree member node names (up to 5).
    pub key_nodes: Vec<String>,
}

/// Result of running community detection over the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommunityResult {
    pub assignments: Vec<CommunityAssignment>,
    pub summaries: Vec<CommunitySummary>,
    pub num_communities: usize,
    pub modularity: f64,
}

/// A highly-connected "god node" — everything flows through these.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GodNode {
    pub node_id: String,
    pub name: String,
    pub qualified_name: String,
    pub file_path: String,
    pub kind: String,
    pub in_degree: usize,
    pub out_degree: usize,
    pub degree: usize,
}

/// An edge that unexpectedly links two different communities and modules.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurprisingEdge {
    pub source_id: String,
    pub target_id: String,
    pub source_name: String,
    pub target_name: String,
    pub kind: String,
    pub confidence: String,
    pub source_community: i64,
    pub target_community: i64,
    pub source_module: String,
    pub target_module: String,
    /// Higher = more surprising (rarer cross-community pairing).
    pub score: f64,
}

/// Aggregate insight payload consumed by the report, MCP tool, and web API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphInsights {
    pub node_count: usize,
    pub edge_count: usize,
    pub num_communities: usize,
    pub modularity: f64,
    pub god_nodes: Vec<GodNode>,
    pub communities: Vec<CommunitySummary>,
    pub surprising_connections: Vec<SurprisingEdge>,
}

/// Run every analysis over an in-memory node/edge set in one pass.
///
/// This does not touch the database; callers fetch nodes/edges and persist
/// community assignments separately.
pub fn compute_insights(
    nodes: &[Node],
    edges: &[Edge],
    resolution: f64,
    god_limit: usize,
    surprising_limit: usize,
) -> (GraphInsights, Vec<CommunityAssignment>) {
    let community = detect_communities(nodes, edges, resolution);
    let community_map: HashMap<String, i64> = community
        .assignments
        .iter()
        .map(|a| (a.node_id.clone(), a.community_id))
        .collect();
    let gods = god_nodes(nodes, edges, god_limit);
    let surprising = surprising_connections(nodes, edges, &community_map, surprising_limit);

    let insights = GraphInsights {
        node_count: nodes.len(),
        edge_count: edges.len(),
        num_communities: community.num_communities,
        modularity: community.modularity,
        god_nodes: gods,
        communities: community.summaries,
        surprising_connections: surprising,
    };
    (insights, community.assignments)
}

/// Detect communities using the Leiden algorithm on the semantic subgraph.
///
/// `resolution` tunes granularity (higher → more, smaller communities).
/// Returns an empty result when there are no nodes.
pub fn detect_communities(nodes: &[Node], edges: &[Edge], resolution: f64) -> CommunityResult {
    if nodes.is_empty() {
        return CommunityResult {
            assignments: Vec::new(),
            summaries: Vec::new(),
            num_communities: 0,
            modularity: 0.0,
        };
    }

    // Dense 0..n index over node ids.
    let mut id_to_idx: HashMap<&str, usize> = HashMap::with_capacity(nodes.len());
    for (i, n) in nodes.iter().enumerate() {
        id_to_idx.insert(n.id.as_str(), i);
    }

    // Aggregate parallel/reverse edges into undirected weights.
    let mut weights: HashMap<(usize, usize), f64> = HashMap::new();
    for edge in edges {
        if !is_architectural_edge(edge.kind) {
            continue;
        }
        let (Some(&a), Some(&b)) =
            (id_to_idx.get(edge.source.as_str()), id_to_idx.get(edge.target.as_str()))
        else {
            continue;
        };
        if a == b {
            continue; // skip self-loops
        }
        let key = if a < b { (a, b) } else { (b, a) };
        *weights.entry(key).or_insert(0.0) += 1.0;
    }

    // No semantic edges → every node is its own singleton community.
    if weights.is_empty() {
        let assignments = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| CommunityAssignment {
                node_id: n.id.clone(),
                community_id: i as i64,
                label: None,
            })
            .collect();
        return CommunityResult {
            assignments,
            summaries: Vec::new(),
            num_communities: nodes.len(),
            modularity: 0.0,
        };
    }

    let mut builder = GraphDataBuilder::new(nodes.len());
    for ((a, b), w) in &weights {
        // add_edge only fails on out-of-range indices, which we control.
        let _ = builder.add_edge(*a, *b, *w);
    }
    let graph = match builder.build() {
        Ok(g) => g,
        Err(_) => {
            return CommunityResult {
                assignments: Vec::new(),
                summaries: Vec::new(),
                num_communities: 0,
                modularity: 0.0,
            }
        }
    };

    let mut config = LeidenConfig::default();
    config.resolution = resolution;
    // Seeded for deterministic, reproducible partitions across runs.
    config.seed = Some(42);
    let leiden = Leiden::new(config);
    let outcome = match leiden.run(&graph) {
        Ok(o) => o,
        Err(_) => {
            return CommunityResult {
                assignments: Vec::new(),
                summaries: Vec::new(),
                num_communities: 0,
                modularity: 0.0,
            }
        }
    };

    let membership = outcome.partition.as_slice();
    let num_communities = outcome.partition.num_communities();

    // Degree (semantic) per node for label + key-node selection.
    let degrees = semantic_degrees(nodes, edges, &id_to_idx);

    // Group node indices by community.
    let mut by_community: HashMap<i64, Vec<usize>> = HashMap::new();
    for (idx, &community) in membership.iter().enumerate() {
        by_community.entry(community as i64).or_default().push(idx);
    }

    // Build labels + summaries.
    let mut summaries: Vec<CommunitySummary> = Vec::new();
    let mut labels: HashMap<i64, String> = HashMap::new();
    for (community_id, members) in &by_community {
        let label = derive_label(members, nodes, &degrees);
        labels.insert(*community_id, label.clone());

        let mut ranked: Vec<usize> = members.clone();
        ranked.sort_by(|&x, &y| degrees[y].2.cmp(&degrees[x].2));
        let key_nodes: Vec<String> = ranked
            .iter()
            .take(5)
            .map(|&i| nodes[i].name.clone())
            .collect();

        summaries.push(CommunitySummary {
            community_id: *community_id,
            label,
            size: members.len(),
            key_nodes,
        });
    }
    summaries.sort_by(|a, b| b.size.cmp(&a.size));

    let assignments = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let community_id = membership[i] as i64;
            CommunityAssignment {
                node_id: n.id.clone(),
                community_id,
                label: labels.get(&community_id).cloned(),
            }
        })
        .collect();

    CommunityResult {
        assignments,
        summaries,
        num_communities,
        modularity: outcome.quality,
    }
}

/// Rank the most-connected nodes by total semantic degree.
pub fn god_nodes(nodes: &[Node], edges: &[Edge], limit: usize) -> Vec<GodNode> {
    let mut id_to_idx: HashMap<&str, usize> = HashMap::with_capacity(nodes.len());
    for (i, n) in nodes.iter().enumerate() {
        id_to_idx.insert(n.id.as_str(), i);
    }
    let degrees = semantic_degrees(nodes, edges, &id_to_idx);

    let mut ranked: Vec<GodNode> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let (in_deg, out_deg, total) = degrees[i];
            GodNode {
                node_id: n.id.clone(),
                name: n.name.clone(),
                qualified_name: n.qualified_name.clone(),
                file_path: n.file_path.clone(),
                kind: n.kind.as_str().to_string(),
                in_degree: in_deg,
                out_degree: out_deg,
                degree: total,
            }
        })
        .filter(|g| g.degree > 0)
        .collect();

    ranked.sort_by(|a, b| {
        b.degree
            .cmp(&a.degree)
            .then_with(|| b.in_degree.cmp(&a.in_degree))
            .then_with(|| a.name.cmp(&b.name))
    });
    ranked.truncate(limit);
    ranked
}

/// Find edges that cross both a community boundary and a module boundary.
///
/// `communities` maps node id → community id. Edges between rarely-paired
/// communities score higher (more surprising).
pub fn surprising_connections(
    nodes: &[Node],
    edges: &[Edge],
    communities: &HashMap<String, i64>,
    limit: usize,
) -> Vec<SurprisingEdge> {
    let node_by_id: HashMap<&str, &Node> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    // How many edges connect each unordered community pair (rarer = surprising).
    let mut pair_counts: HashMap<(i64, i64), usize> = HashMap::new();
    for edge in edges {
        if !is_architectural_edge(edge.kind) {
            continue;
        }
        let (Some(&sc), Some(&tc)) =
            (communities.get(&edge.source), communities.get(&edge.target))
        else {
            continue;
        };
        if sc == tc {
            continue;
        }
        let key = if sc < tc { (sc, tc) } else { (tc, sc) };
        *pair_counts.entry(key).or_insert(0) += 1;
    }

    let mut result: Vec<SurprisingEdge> = Vec::new();
    for edge in edges {
        if !is_architectural_edge(edge.kind) {
            continue;
        }
        let (Some(&sc), Some(&tc)) =
            (communities.get(&edge.source), communities.get(&edge.target))
        else {
            continue;
        };
        if sc == tc {
            continue;
        }
        let (Some(src), Some(tgt)) = (
            node_by_id.get(edge.source.as_str()),
            node_by_id.get(edge.target.as_str()),
        ) else {
            continue;
        };
        let src_module = top_module(&src.file_path);
        let tgt_module = top_module(&tgt.file_path);
        if src_module == tgt_module {
            continue; // same module → not surprising
        }
        let key = if sc < tc { (sc, tc) } else { (tc, sc) };
        let pair_count = *pair_counts.get(&key).unwrap_or(&1) as f64;
        // Rarer community pairings score higher.
        let score = 1.0 / pair_count;
        result.push(SurprisingEdge {
            source_id: edge.source.clone(),
            target_id: edge.target.clone(),
            source_name: src.name.clone(),
            target_name: tgt.name.clone(),
            kind: edge.kind.as_str().to_string(),
            confidence: edge.effective_confidence().as_str().to_string(),
            source_community: sc,
            target_community: tc,
            source_module: src_module,
            target_module: tgt_module,
            score,
        });
    }

    result.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.source_name.cmp(&b.source_name))
    });
    result.truncate(limit);
    result
}

/// (in_degree, out_degree, total) semantic degree per node, indexed like `nodes`.
fn semantic_degrees(
    nodes: &[Node],
    edges: &[Edge],
    id_to_idx: &HashMap<&str, usize>,
) -> Vec<(usize, usize, usize)> {
    let mut degrees = vec![(0usize, 0usize, 0usize); nodes.len()];
    for edge in edges {
        if !is_architectural_edge(edge.kind) {
            continue;
        }
        if let Some(&s) = id_to_idx.get(edge.source.as_str()) {
            degrees[s].1 += 1;
            degrees[s].2 += 1;
        }
        if let Some(&t) = id_to_idx.get(edge.target.as_str()) {
            degrees[t].0 += 1;
            degrees[t].2 += 1;
        }
    }
    degrees
}

/// Derive a community label from its dominant top-level module, falling back to
/// the highest-degree member's name.
fn derive_label(
    members: &[usize],
    nodes: &[Node],
    degrees: &[(usize, usize, usize)],
) -> String {
    // Most common top-level module among members.
    let mut module_counts: HashMap<String, usize> = HashMap::new();
    for &i in members {
        let module = top_module(&nodes[i].file_path);
        if !module.is_empty() {
            *module_counts.entry(module).or_insert(0) += 1;
        }
    }
    if let Some((module, count)) = module_counts.iter().max_by_key(|(_, c)| **c) {
        // Only trust the module label when it covers a clear majority.
        if *count * 2 >= members.len() {
            return module.clone();
        }
    }

    // Fallback: highest-degree member name.
    members
        .iter()
        .max_by_key(|&&i| degrees[i].2)
        .map(|&i| nodes[i].name.clone())
        .unwrap_or_else(|| "unnamed".to_string())
}

/// First meaningful path segment (top-level module/dir) of a file path.
fn top_module(file_path: &str) -> String {
    let normalized = file_path.replace('\\', "/");
    let trimmed = normalized.trim_start_matches("./");
    let mut parts = trimmed.split('/').filter(|p| !p.is_empty());
    match (parts.next(), parts.clone().next()) {
        // If the first segment looks like a top-level dir, prefer "dir/sub".
        (Some(first), Some(second)) => {
            // For monorepo-ish layouts (crates/, packages/, src/), include one
            // more level so communities don't all collapse into "src".
            if matches!(first, "crates" | "packages" | "apps" | "src" | "lib" | "pkg") {
                format!("{first}/{second}")
            } else {
                first.to_string()
            }
        }
        (Some(first), None) => first.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_types::{EdgeConfidence, Language, NodeKind, Provenance};

    fn node(id: &str, name: &str, file: &str) -> Node {
        Node {
            id: id.to_string(),
            kind: NodeKind::Function,
            name: name.to_string(),
            qualified_name: name.to_string(),
            file_path: file.to_string(),
            language: Language::Rust,
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

    fn edge(src: &str, tgt: &str) -> Edge {
        Edge {
            source: src.to_string(),
            target: tgt.to_string(),
            kind: EdgeKind::Calls,
            metadata: None,
            line: None,
            column: None,
            provenance: Some(Provenance::TreeSitter),
            confidence: Some(EdgeConfidence::Extracted),
        }
    }

    #[test]
    fn documents_edge_is_semantic_but_not_architectural() {
        // Doc -> code mentions stay visible in the graph (semantic) but must
        // not distort community/god-node math (architectural).
        assert!(is_semantic_edge(EdgeKind::Documents));
        assert!(!is_architectural_edge(EdgeKind::Documents));
        // Regular code relationships remain both.
        assert!(is_semantic_edge(EdgeKind::Calls));
        assert!(is_architectural_edge(EdgeKind::Calls));
        // Structural nesting is neither.
        assert!(!is_semantic_edge(EdgeKind::Contains));
        assert!(!is_architectural_edge(EdgeKind::Contains));
    }

    #[test]
    fn documents_edges_excluded_from_god_nodes() {
        // A doc node that mentions everything must not rank as a god node.
        let nodes = vec![
            node("doc", "README.md", "README.md"),
            node("a", "a", "src/a.rs"),
            node("b", "b", "src/b.rs"),
        ];
        let doc_edge = |tgt: &str| Edge {
            source: "doc".to_string(),
            target: tgt.to_string(),
            kind: EdgeKind::Documents,
            metadata: None,
            line: None,
            column: None,
            provenance: Some(Provenance::Heuristic),
            confidence: Some(EdgeConfidence::Inferred),
        };
        let edges = vec![doc_edge("a"), doc_edge("b")];
        let gods = god_nodes(&nodes, &edges, 10);
        assert!(
            gods.iter().all(|g| g.node_id != "doc"),
            "documentation node should be excluded from god-node ranking"
        );
    }

    #[test]
    fn god_nodes_rank_by_degree() {
        let nodes = vec![
            node("a", "hub", "src/a.rs"),
            node("b", "b", "src/b.rs"),
            node("c", "c", "src/c.rs"),
        ];
        let edges = vec![edge("b", "a"), edge("c", "a"), edge("a", "b")];
        let gods = god_nodes(&nodes, &edges, 10);
        assert_eq!(gods[0].node_id, "a");
        assert!(gods[0].degree >= gods[1].degree);
    }

    #[test]
    fn communities_split_two_clusters() {
        // Two triangles connected by a single bridge edge.
        let nodes = vec![
            node("a1", "a1", "src/a/1.rs"),
            node("a2", "a2", "src/a/2.rs"),
            node("a3", "a3", "src/a/3.rs"),
            node("b1", "b1", "src/b/1.rs"),
            node("b2", "b2", "src/b/2.rs"),
            node("b3", "b3", "src/b/3.rs"),
        ];
        let edges = vec![
            edge("a1", "a2"),
            edge("a2", "a3"),
            edge("a3", "a1"),
            edge("b1", "b2"),
            edge("b2", "b3"),
            edge("b3", "b1"),
            edge("a1", "b1"), // bridge
        ];
        let result = detect_communities(&nodes, &edges, 1.0);
        assert!(result.num_communities >= 2);
        assert_eq!(result.assignments.len(), 6);
    }

    #[test]
    fn empty_graph_is_safe() {
        let result = detect_communities(&[], &[], 1.0);
        assert_eq!(result.num_communities, 0);
        assert!(result.assignments.is_empty());
    }
}
