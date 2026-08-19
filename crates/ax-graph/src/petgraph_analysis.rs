//! petgraph helpers for subgraph analysis (cycle detection, path finding).

use std::collections::HashMap;

use ax_types::{Edge, EdgeKind};
use petgraph::algo::{astar, is_cyclic_directed, tarjan_scc};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use serde::Serialize;

/// Build a directed graph from call/reference edges and detect cycles.
pub fn call_graph_has_cycle(edges: &[Edge]) -> bool {
    let (graph, _) = build_call_graph(edges);
    is_cyclic_directed(&graph)
}

/// One call-graph cycle (or mutual SCC) as ordered node ids.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallCycle {
    pub nodes: Vec<String>,
}

/// List non-trivial strongly connected components on the Calls/References graph.
///
/// Multi-node SCCs are preferred. Pure self-loops (common noise in minified JS)
/// are omitted unless `include_self_loops` is true.
pub fn find_call_cycles(edges: &[Edge], limit: usize) -> Vec<CallCycle> {
    find_call_cycles_opts(edges, limit, false)
}

pub fn find_call_cycles_opts(
    edges: &[Edge],
    limit: usize,
    include_self_loops: bool,
) -> Vec<CallCycle> {
    let (graph, index) = build_call_graph(edges);
    let id_by_idx: HashMap<NodeIndex, String> = index.into_iter().map(|(k, v)| (v, k)).collect();
    let mut cycles: Vec<CallCycle> = tarjan_scc(&graph)
        .into_iter()
        .filter(|scc| {
            if scc.len() > 1 {
                return true;
            }
            include_self_loops
                && scc.len() == 1
                && graph.edges(scc[0]).any(|e| e.target() == scc[0])
        })
        .map(|scc| {
            let mut nodes: Vec<String> = scc
                .into_iter()
                .filter_map(|idx| id_by_idx.get(&idx).cloned())
                .collect();
            nodes.sort();
            CallCycle { nodes }
        })
        // Prefer source cycles over minified bundle noise.
        .filter(|c| !c.nodes.iter().any(|id| id_looks_like_bundle(id)))
        .collect();
    // Largest cycles first — those are the ones agents care about.
    cycles.sort_by(|a, b| {
        b.nodes
            .len()
            .cmp(&a.nodes.len())
            .then_with(|| a.nodes.cmp(&b.nodes))
    });
    if limit > 0 && cycles.len() > limit {
        cycles.truncate(limit);
    }
    cycles
}

fn id_looks_like_bundle(id: &str) -> bool {
    let s = id.replace('\\', "/").to_ascii_lowercase();
    s.contains("/dist/")
        || s.contains("/node_modules/")
        || s.contains(".min.js")
        || s.contains("/vendor/")
}

/// Shortest directed path (Calls/References) from `from_id` to `to_id`.
pub fn shortest_call_path(edges: &[Edge], from_id: &str, to_id: &str) -> Option<Vec<String>> {
    if from_id == to_id {
        return Some(vec![from_id.to_string()]);
    }
    let (graph, index) = build_call_graph(edges);
    let start = *index.get(from_id)?;
    let goal = *index.get(to_id)?;
    let id_by_idx: HashMap<NodeIndex, String> = index.into_iter().map(|(k, v)| (v, k)).collect();
    let (_cost, path) = astar(
        &graph,
        start,
        |n| n == goal,
        |_| 1usize,
        |_| 0usize,
    )?;
    Some(
        path.into_iter()
            .filter_map(|idx| id_by_idx.get(&idx).cloned())
            .collect(),
    )
}

fn build_call_graph(edges: &[Edge]) -> (DiGraph<(), EdgeKind>, HashMap<String, NodeIndex>) {
    let mut graph = DiGraph::<(), EdgeKind>::new();
    let mut index: HashMap<String, NodeIndex> = HashMap::new();

    for edge in edges {
        if !matches!(edge.kind, EdgeKind::Calls | EdgeKind::References) {
            continue;
        }
        if !index.contains_key(&edge.source) {
            let idx = graph.add_node(());
            index.insert(edge.source.clone(), idx);
        }
        if !index.contains_key(&edge.target) {
            let idx = graph.add_node(());
            index.insert(edge.target.clone(), idx);
        }
        let src = index[&edge.source];
        let dst = index[&edge.target];
        graph.add_edge(src, dst, edge.kind);
    }

    (graph, index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_types::Provenance;

    fn call(source: &str, target: &str) -> Edge {
        Edge {
            source: source.into(),
            target: target.into(),
            kind: EdgeKind::Calls,
            metadata: None,
            line: None,
            column: None,
            provenance: Some(Provenance::Heuristic),
            confidence: Some(ax_types::EdgeConfidence::Inferred),
        }
    }

    #[test]
    fn detects_simple_cycle() {
        let edges = vec![call("a", "b"), call("b", "a")];
        assert!(call_graph_has_cycle(&edges));
        let cycles = find_call_cycles(&edges, 10);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].nodes, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn acyclic_chain() {
        let edges = vec![call("a", "b"), call("b", "c")];
        assert!(!call_graph_has_cycle(&edges));
        assert!(find_call_cycles(&edges, 10).is_empty());
    }

    #[test]
    fn shortest_path_finds_chain() {
        let edges = vec![call("a", "b"), call("b", "c"), call("a", "c")];
        let path = shortest_call_path(&edges, "a", "c").unwrap();
        assert_eq!(path, vec!["a", "c"]);
    }

    #[test]
    fn shortest_path_missing_returns_none() {
        let edges = vec![call("a", "b")];
        assert!(shortest_call_path(&edges, "a", "z").is_none());
    }

    #[test]
    fn self_loops_omitted_by_default() {
        let edges = vec![call("a", "a"), call("b", "c"), call("c", "b")];
        let cycles = find_call_cycles(&edges, 10);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].nodes, vec!["b".to_string(), "c".to_string()]);
        let with_loops = find_call_cycles_opts(&edges, 10, true);
        assert!(with_loops.iter().any(|c| c.nodes == vec!["a".to_string()]));
    }
}
