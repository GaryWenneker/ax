//! Bounded personalized PageRank over memory anchors and file co-change (Phase 6).

use std::collections::{HashMap, HashSet};

/// One personalized PageRank step over an undirected adjacency list.
pub fn personalized_pagerank(
    seeds: &[String],
    edges: &[(String, String)],
    damping: f64,
    steps: usize,
) -> HashMap<String, f64> {
    let mut nodes: HashSet<String> = HashSet::new();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for (a, b) in edges {
        nodes.insert(a.clone());
        nodes.insert(b.clone());
        adj.entry(a.clone()).or_default().push(b.clone());
        adj.entry(b.clone()).or_default().push(a.clone());
    }
    for s in seeds {
        nodes.insert(s.clone());
    }
    if nodes.is_empty() {
        return HashMap::new();
    }
    let n = nodes.len() as f64;
    let seed_set: HashSet<&str> = seeds.iter().map(|s| s.as_str()).collect();
    let seed_mass = if seed_set.is_empty() {
        0.0
    } else {
        1.0 / seed_set.len() as f64
    };
    let mut rank: HashMap<String, f64> = nodes
        .iter()
        .map(|id| {
            let v = if seed_set.contains(id.as_str()) {
                seed_mass
            } else {
                0.0
            };
            (id.clone(), v)
        })
        .collect();
    for _ in 0..steps.max(1) {
        let mut next: HashMap<String, f64> = nodes.iter().map(|id| (id.clone(), 0.0)).collect();
        for id in &nodes {
            let neighbors = adj.get(id).map(|v| v.as_slice()).unwrap_or(&[]);
            let share = if neighbors.is_empty() {
                0.0
            } else {
                rank.get(id).copied().unwrap_or(0.0) / neighbors.len() as f64
            };
            for nb in neighbors {
                *next.entry(nb.clone()).or_default() += damping * share;
            }
            let teleport = if seed_set.contains(id.as_str()) {
                (1.0 - damping) * seed_mass
            } else if seed_set.is_empty() {
                (1.0 - damping) / n
            } else {
                0.0
            };
            *next.entry(id.clone()).or_default() += teleport;
        }
        rank = next;
    }
    rank
}

#[cfg(test)]
mod tests {
    use super::personalized_pagerank;

    #[test]
    fn seed_outranks_distant_node() {
        let edges = vec![("a".into(), "b".into()), ("b".into(), "c".into())];
        let rank = personalized_pagerank(&["a".into()], &edges, 0.85, 20);
        assert!(rank["a"] > rank["c"]);
    }
}
