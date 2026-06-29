//! petgraph helpers for subgraph analysis (cycle detection, topo order).

use std::collections::HashMap;

use ax_types::{Edge, EdgeKind};
use petgraph::algo::is_cyclic_directed;
use petgraph::graph::{DiGraph, NodeIndex};

/// Build a directed graph from call/reference edges and detect cycles.
pub fn call_graph_has_cycle(edges: &[Edge]) -> bool {
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

    is_cyclic_directed(&graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_types::Provenance;

    #[test]
    fn detects_simple_cycle() {
        let edges = vec![
            Edge {
                source: "a".into(),
                target: "b".into(),
                kind: EdgeKind::Calls,
                metadata: None,
                line: None,
                column: None,
                provenance: Some(Provenance::Heuristic),
            },
            Edge {
                source: "b".into(),
                target: "a".into(),
                kind: EdgeKind::Calls,
                metadata: None,
                line: None,
                column: None,
                provenance: Some(Provenance::Heuristic),
            },
        ];
        assert!(call_graph_has_cycle(&edges));
    }

    #[test]
    fn acyclic_chain() {
        let edges = vec![
            Edge {
                source: "a".into(),
                target: "b".into(),
                kind: EdgeKind::Calls,
                metadata: None,
                line: None,
                column: None,
                provenance: Some(Provenance::Heuristic),
            },
            Edge {
                source: "b".into(),
                target: "c".into(),
                kind: EdgeKind::Calls,
                metadata: None,
                line: None,
                column: None,
                provenance: Some(Provenance::Heuristic),
            },
        ];
        assert!(!call_graph_has_cycle(&edges));
    }
}
