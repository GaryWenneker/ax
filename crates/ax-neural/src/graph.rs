use petgraph::graphmap::UnGraphMap;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct InputEdge {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Suggestion {
    pub source: String,
    pub target: String,
    pub score: f64,
    pub reason: String,
    pub provenance: String,
}

pub const MAX_SHARED_DEGREE: usize = 64;
pub const EDGE_LIMIT: usize = 200_000;
pub const MAX_CANDIDATE_PAIRS: usize = 500_000;
const MAX_SUGGESTIONS: usize = 50;
const MIN_CO_OCCURRENCE: usize = 2;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GraphReport {
    pub suggestions: Vec<Suggestion>,
    pub edges_read: usize,
    pub skipped_hubs: usize,
    pub truncated: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub edge_limit_reached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_pair_limit: Option<usize>,
}

pub fn gnn_status() -> &'static str {
    "not_in_binary"
}

/// Shared-neighbor pairs that are not already edges. Provenance is always predicted.
pub fn suggest(edges: &[InputEdge]) -> Vec<Suggestion> {
    suggest_report(edges, &[]).suggestions
}

/// Neighbor overlap (Jaccard) over the given edges, plus ids that appear together in at
/// least two successful feedback rows. Nodes and rows wider than `MAX_SHARED_DEGREE` are
/// skipped, because they relate everything to everything.
pub fn suggest_report(edges: &[InputEdge], co_occurrence: &[Vec<String>]) -> GraphReport {
    suggest_report_capped(edges, co_occurrence, EDGE_LIMIT)
}

/// Overlap suggestions over at most `max_edges` input edges (see [`EDGE_LIMIT`] in dispatch).
pub fn suggest_report_capped(
    edges: &[InputEdge],
    co_occurrence: &[Vec<String>],
    max_edges: usize,
) -> GraphReport {
    let edge_limit_reached = edges.len() > max_edges;
    let edge_limit = edge_limit_reached.then_some(max_edges);
    let edges = &edges[..edges.len().min(max_edges)];
    let mut graph = Graph::default();
    for edge in edges {
        if !edge.source.is_empty() && !edge.target.is_empty() && edge.source != edge.target {
            graph.connect(&edge.source, &edge.target);
        }
    }
    let overlap = graph.neighbor_overlap(MAX_CANDIDATE_PAIRS);
    let mut suggestions = overlap.suggestions;
    suggestions.extend(co_occurring(co_occurrence, &graph));
    let candidate_pair_limit = overlap.truncated.then_some(MAX_CANDIDATE_PAIRS);
    GraphReport {
        suggestions,
        edges_read: edges.len(),
        skipped_hubs: overlap.skipped_hubs,
        truncated: overlap.truncated,
        edge_limit_reached,
        edge_limit,
        candidate_pair_limit,
    }
}

#[derive(Default)]
struct Graph {
    ids: HashMap<std::rc::Rc<str>, u32>,
    names: Vec<std::rc::Rc<str>>,
    edges: UnGraphMap<u32, ()>,
}

struct Overlap {
    suggestions: Vec<Suggestion>,
    skipped_hubs: usize,
    truncated: bool,
}

impl Graph {
    fn id(&mut self, name: &str) -> u32 {
        if let Some(id) = self.ids.get(name) {
            return *id;
        }
        let id = self.names.len() as u32;
        let owned: std::rc::Rc<str> = name.into();
        self.ids.insert(std::rc::Rc::clone(&owned), id);
        self.names.push(owned);
        id
    }

    fn connect(&mut self, source: &str, target: &str) {
        let a = self.id(source);
        let b = self.id(target);
        self.edges.add_edge(a, b, ());
    }

    fn adjacent(&self, a: &str, b: &str) -> bool {
        match (self.ids.get(a), self.ids.get(b)) {
            (Some(a), Some(b)) => self.edges.contains_edge(*a, *b),
            _ => false,
        }
    }

    fn degree(&self, node: u32) -> usize {
        self.edges.neighbors(node).count()
    }

    fn neighbor_overlap(&self, max_pairs: usize) -> Overlap {
        let mut shared: HashMap<(u32, u32), usize> = HashMap::new();
        let mut skipped_hubs = 0;
        let mut truncated = false;
        for middle in self.edges.nodes() {
            let around: Vec<u32> = self.edges.neighbors(middle).collect();
            if around.len() > MAX_SHARED_DEGREE {
                skipped_hubs += 1;
                continue;
            }
            for (index, &a) in around.iter().enumerate() {
                for &b in &around[index + 1..] {
                    if self.edges.contains_edge(a, b) {
                        continue;
                    }
                    let key = (a.min(b), a.max(b));
                    if let Some(count) = shared.get_mut(&key) {
                        *count += 1;
                    } else if shared.len() < max_pairs {
                        shared.insert(key, 1);
                    } else {
                        truncated = true;
                    }
                }
            }
        }
        let mut scored = Vec::with_capacity(shared.len());
        for ((a, b), common) in shared {
            let union = self.degree(a) + self.degree(b) - common;
            let left: &str = &self.names[a as usize];
            let right: &str = &self.names[b as usize];
            let (source, target) = if left <= right {
                (left, right)
            } else {
                (right, left)
            };
            scored.push((common as f64 / union as f64, source, target));
        }
        Overlap {
            suggestions: keep_best(scored, "shared-neighbor"),
            skipped_hubs,
            truncated,
        }
    }
}

fn co_occurring(rows: &[Vec<String>], graph: &Graph) -> Vec<Suggestion> {
    let mut counts: HashMap<(&str, &str), usize> = HashMap::new();
    for row in rows {
        let mut ids: Vec<&str> = row
            .iter()
            .map(String::as_str)
            .filter(|id| !id.is_empty())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        if ids.len() > MAX_SHARED_DEGREE {
            continue;
        }
        for (index, a) in ids.iter().enumerate() {
            for b in &ids[index + 1..] {
                *counts.entry((a, b)).or_insert(0) += 1;
            }
        }
    }
    let total = rows.len() as f64;
    let scored = counts
        .into_iter()
        .filter(|((a, b), count)| *count >= MIN_CO_OCCURRENCE && !graph.adjacent(a, b))
        .map(|((a, b), count)| {
            let (source, target) = if a <= b { (a, b) } else { (b, a) };
            (count as f64 / total, source, target)
        })
        .collect();
    keep_best(scored, "co-occurrence")
}

fn predicted(a: &str, b: &str, score: f64, reason: &str) -> Suggestion {
    let (source, target) = pair(a, b);
    Suggestion {
        source,
        target,
        score,
        reason: reason.into(),
        provenance: "predicted".into(),
    }
}

fn keep_best(mut rows: Vec<(f64, &str, &str)>, reason: &str) -> Vec<Suggestion> {
    let order = |left: &(f64, &str, &str), right: &(f64, &str, &str)| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.cmp(right.1))
            .then_with(|| left.2.cmp(right.2))
    };
    if rows.len() > MAX_SUGGESTIONS {
        rows.select_nth_unstable_by(MAX_SUGGESTIONS, order);
        rows.truncate(MAX_SUGGESTIONS);
    }
    rows.sort_by(order);
    rows.into_iter()
        .map(|(score, source, target)| predicted(source, target, score, reason))
        .collect()
}

fn pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_neighbor_is_predicted_and_existing_edges_are_not_repeated() {
        let edges = vec![
            InputEdge {
                source: "a".into(),
                target: "hub".into(),
            },
            InputEdge {
                source: "b".into(),
                target: "hub".into(),
            },
            InputEdge {
                source: "a".into(),
                target: "b".into(),
            },
            InputEdge {
                source: "c".into(),
                target: "hub".into(),
            },
        ];
        let suggestions = suggest(&edges);
        assert!(suggestions
            .iter()
            .all(|item| item.provenance == "predicted"));
        assert!(suggestions
            .iter()
            .all(|item| item.reason == "shared-neighbor"));
        assert!(!suggestions
            .iter()
            .any(|item| pair(&item.source, &item.target) == pair("a", "b")));
        assert!(suggestions
            .iter()
            .any(|item| pair(&item.source, &item.target) == pair("a", "c")));
        assert_eq!(gnn_status(), "not_in_binary");
    }

    fn edge(source: &str, target: &str) -> InputEdge {
        InputEdge {
            source: source.into(),
            target: target.into(),
        }
    }

    fn find<'a>(report: &'a GraphReport, a: &str, b: &str) -> Option<&'a Suggestion> {
        report
            .suggestions
            .iter()
            .find(|item| pair(&item.source, &item.target) == pair(a, b))
    }

    #[test]
    fn hub_nodes_are_skipped() {
        let mut edges: Vec<InputEdge> = (0..=MAX_SHARED_DEGREE)
            .map(|i| edge(&format!("leaf{i}"), "hub"))
            .collect();
        edges.push(edge("x", "y"));
        edges.push(edge("z", "y"));
        let report = suggest_report(&edges, &[]);
        assert_eq!(report.skipped_hubs, 1);
        assert_eq!(report.edges_read, MAX_SHARED_DEGREE + 3);
        assert!(report
            .suggestions
            .iter()
            .all(|item| !item.source.starts_with("leaf") && !item.target.starts_with("leaf")));
        assert!(find(&report, "x", "z").is_some());
    }

    #[test]
    fn a_node_at_the_degree_limit_is_used() {
        let edges: Vec<InputEdge> = (0..MAX_SHARED_DEGREE)
            .map(|i| edge(&format!("leaf{i}"), "hub"))
            .collect();
        let report = suggest_report(&edges, &[]);
        assert_eq!(report.skipped_hubs, 0);
        assert!(find(&report, "leaf0", "leaf1").is_some());
    }

    #[test]
    fn overlap_is_bounded() {
        let edges = vec![
            edge("a", "n1"),
            edge("a", "n2"),
            edge("b", "n1"),
            edge("b", "n2"),
            edge("c", "n1"),
        ];
        let report = suggest_report(&edges, &[]);
        assert!(!report.suggestions.is_empty());
        for item in &report.suggestions {
            assert!(item.score > 0.0 && item.score <= 1.0, "{item:?}");
        }
        let ab = find(&report, "a", "b").expect("a and b share both neighbors");
        assert!((ab.score - 1.0).abs() < 1e-9, "{ab:?}");
        let ac = find(&report, "a", "c").expect("a and c share one neighbor");
        assert!((ac.score - 0.5).abs() < 1e-9, "{ac:?}");
        let hubs = find(&report, "n1", "n2").expect("n1 and n2 share a and b");
        assert!((hubs.score - 2.0 / 3.0).abs() < 1e-9, "{hubs:?}");
        assert_eq!(
            pair(&report.suggestions[0].source, &report.suggestions[0].target),
            pair("a", "b")
        );
    }

    #[test]
    fn feedback_co_occurrence_is_suggested() {
        let rows = vec![
            vec!["a".to_string(), "b".to_string()],
            vec!["b".to_string(), "a".to_string(), "a".to_string()],
            vec!["c".to_string(), "a".to_string()],
            vec!["a".to_string(), "c".to_string()],
        ];
        let report = suggest_report(&[edge("a", "c")], &rows);
        let found: Vec<&Suggestion> = report
            .suggestions
            .iter()
            .filter(|item| item.reason == "co-occurrence")
            .collect();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(pair(&found[0].source, &found[0].target), pair("a", "b"));
        assert!((found[0].score - 0.5).abs() < 1e-9, "{:?}", found[0]);
        assert_eq!(found[0].provenance, "predicted");
    }

    #[test]
    fn a_pair_seen_once_is_not_suggested() {
        let rows = vec![
            vec!["a".to_string(), "b".to_string()],
            vec!["c".to_string()],
        ];
        let report = suggest_report(&[], &rows);
        assert!(report.suggestions.is_empty(), "{:?}", report.suggestions);
    }

    #[test]
    fn empty_ids_and_self_loops_are_ignored_and_pairs_name_the_smaller_id_first() {
        let edges = vec![
            edge("", "x"),
            edge("z", "x"),
            edge("z", "z"),
            edge("y", "x"),
        ];
        let report = suggest_report(&edges, &[]);
        let found: Vec<(&str, &str, f64)> = report
            .suggestions
            .iter()
            .map(|item| (item.source.as_str(), item.target.as_str(), item.score))
            .collect();
        assert_eq!(found, vec![("y", "z", 1.0)]);
    }

    #[test]
    fn candidate_pairs_past_the_cap_are_truncated() {
        let mut graph = Graph::default();
        for leaf in ["a", "b", "c", "d"] {
            graph.connect(leaf, "hub");
        }
        let at_cap = graph.neighbor_overlap(6);
        assert!(!at_cap.truncated);
        assert_eq!(at_cap.suggestions.len(), 6);
        let past_cap = graph.neighbor_overlap(5);
        assert!(past_cap.truncated);
        assert_eq!(past_cap.suggestions.len(), 5);
    }

    #[test]
    fn a_feedback_row_at_the_degree_limit_is_used_and_a_wider_one_is_not() {
        let row =
            |width: usize| -> Vec<String> { (0..width).map(|i| format!("id{i:02}")).collect() };
        let at_limit = suggest_report(&[], &[row(MAX_SHARED_DEGREE), row(MAX_SHARED_DEGREE)]);
        assert!(!at_limit.suggestions.is_empty());
        let wider = suggest_report(
            &[],
            &[row(MAX_SHARED_DEGREE + 1), row(MAX_SHARED_DEGREE + 1)],
        );
        assert!(wider.suggestions.is_empty(), "{:?}", wider.suggestions);
    }
}
