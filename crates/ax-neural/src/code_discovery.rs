//! Experimental cross-file discovery. Production rank does not call it.
//! A missing definition is added only across an extracted link or a
//! unique-name link whose ends are in different files.

use std::collections::{HashMap, HashSet};

use crate::code_aware::{hybrid_rank_promoted, symbol_key, CodeCandidate, Selection, VerifiedEdge};

#[derive(Clone, Debug)]
pub struct DiscoveryLink {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub proof: String,
}

#[derive(Clone, Debug)]
pub struct DiscoveryBudget {
    pub depth: usize,
    pub fan_out: usize,
    pub max_added: usize,
    pub time_ms: u128,
}

impl Default for DiscoveryBudget {
    fn default() -> Self {
        Self {
            depth: 2,
            fan_out: 8,
            max_added: 16,
            time_ms: 50,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DiscoveryConfig {
    pub enabled: bool,
    pub project_id: String,
    pub limit: usize,
    pub budget: DiscoveryBudget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveryHit {
    pub id: String,
    pub proof: String,
    pub kind: String,
    pub via: String,
    pub depth: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Discovery {
    pub hits: Vec<DiscoveryHit>,
    pub trace: Vec<String>,
}

fn relation_kind(kind: &str) -> bool {
    matches!(kind, "calls" | "references" | "documents")
}

fn same_project(project_id: &str, node: &CodeCandidate) -> bool {
    project_id.is_empty() || node.project_id.is_empty() || node.project_id == project_id
}

fn node_by_id<'a>(nodes: &'a [CodeCandidate], id: &str) -> Option<&'a CodeCandidate> {
    nodes.iter().find(|node| node.id == id)
}

/// Nodes by id, built once per call so lookups are not linear scans.
fn node_index(nodes: &[CodeCandidate]) -> HashMap<&str, &CodeCandidate> {
    let mut index = HashMap::with_capacity(nodes.len());
    for node in nodes {
        // The first node with an id wins, as `node_by_id` does.
        index.entry(node.id.as_str()).or_insert(node);
    }
    index
}

fn files_differ(index: &HashMap<&str, &CodeCandidate>, source: &str, target: &str) -> bool {
    let (Some(left), Some(right)) = (index.get(source), index.get(target)) else {
        return false;
    };
    !left.file_path.is_empty() && !right.file_path.is_empty() && left.file_path != right.file_path
}

fn link_allowed(link: &DiscoveryLink, index: &HashMap<&str, &CodeCandidate>) -> bool {
    if !relation_kind(&link.kind) {
        return false;
    }
    match link.proof.as_str() {
        "extracted" => true,
        "unique-cross-file" => files_differ(index, &link.source, &link.target),
        _ => false,
    }
}

fn proof_rank(proof: &str) -> u8 {
    match proof {
        "extracted" => 0,
        "unique-cross-file" => 1,
        _ => 2,
    }
}

pub fn discover_candidates(
    seeds: &[CodeCandidate],
    nodes: &[CodeCandidate],
    links: &[DiscoveryLink],
    project_id: &str,
    budget: &DiscoveryBudget,
) -> Discovery {
    let started = std::time::Instant::now();
    let index = node_index(nodes);
    let mut by_source: HashMap<&str, Vec<&DiscoveryLink>> = HashMap::new();
    for link in links {
        by_source
            .entry(link.source.as_str())
            .or_default()
            .push(link);
    }
    let mut hits = Vec::new();
    let mut trace = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue = Vec::new();
    for seed in seeds {
        if !seen.insert(seed.id.clone()) {
            continue;
        }
        queue.push((seed.id.clone(), 0usize));
    }
    let mut head = 0;
    while head < queue.len() {
        if started.elapsed().as_millis() >= budget.time_ms {
            trace.push("stopped:time".to_string());
            break;
        }
        if hits.len() >= budget.max_added {
            trace.push("stopped:candidates".to_string());
            break;
        }
        let (id, depth) = queue[head].clone();
        head += 1;
        if depth >= budget.depth {
            continue;
        }
        let mut outgoing: Vec<&DiscoveryLink> = by_source
            .get(id.as_str())
            .map(|from| {
                from.iter()
                    .copied()
                    .filter(|link| link_allowed(link, &index))
                    .collect()
            })
            .unwrap_or_default();
        outgoing.sort_by(|left, right| {
            proof_rank(&left.proof)
                .cmp(&proof_rank(&right.proof))
                .then(left.target.cmp(&right.target))
        });
        // Fan-out counts the targets this node actually adds; links to
        // missing, foreign or already seen nodes do not use it up.
        let mut followed = 0usize;
        for link in outgoing {
            if followed >= budget.fan_out {
                trace.push(format!("stopped:fan-out:{id}"));
                break;
            }
            if hits.len() >= budget.max_added {
                break;
            }
            if started.elapsed().as_millis() >= budget.time_ms {
                trace.push("stopped:time".to_string());
                return Discovery { hits, trace };
            }
            let Some(target) = index.get(link.target.as_str()).copied() else {
                continue;
            };
            if !same_project(project_id, target) {
                trace.push(format!("rejected:{}", target.id));
                continue;
            }
            if !seen.insert(target.id.clone()) {
                continue;
            }
            followed += 1;
            let next_depth = depth + 1;
            hits.push(DiscoveryHit {
                id: target.id.clone(),
                proof: link.proof.clone(),
                kind: link.kind.clone(),
                via: id.clone(),
                depth: next_depth,
            });
            trace.push(format!(
                "discovered:{}:{}:{}:via={}:depth={}",
                target.id, link.kind, link.proof, id, next_depth
            ));
            queue.push((target.id.clone(), next_depth));
        }
    }
    Discovery { hits, trace }
}

pub fn resolve_seeds(
    baseline: &[CodeCandidate],
    nodes: &[CodeCandidate],
    project_id: &str,
) -> (Vec<CodeCandidate>, Vec<String>) {
    let mut seeds = Vec::new();
    let mut trace = Vec::new();
    for item in baseline {
        if let Some(found) = node_by_id(nodes, &item.id) {
            if !same_project(project_id, found) {
                trace.push(format!("rejected:{}", found.id));
                continue;
            }
            if seeds.iter().any(|seed: &CodeCandidate| seed.id == found.id) {
                continue;
            }
            seeds.push(found.clone());
            trace.push(format!("seed:{}:id", found.id));
            continue;
        }
        let key = symbol_key(&item.name);
        if key.is_empty() {
            trace.push(format!("unresolved:{}", item.id));
            continue;
        }
        let matches: Vec<&CodeCandidate> = nodes
            .iter()
            .filter(|node| same_project(project_id, node) && symbol_key(&node.name) == key)
            .collect();
        if matches.len() == 1 {
            let found = matches[0];
            if seeds.iter().any(|seed: &CodeCandidate| seed.id == found.id) {
                continue;
            }
            seeds.push(found.clone());
            trace.push(format!("seed:{}:name", found.id));
        } else {
            trace.push(format!("unresolved:{}", item.id));
        }
    }
    (seeds, trace)
}

pub fn expand_with_graph(
    query: &str,
    baseline_ids: &[String],
    candidates: &[CodeCandidate],
    nodes: &[CodeCandidate],
    links: &[DiscoveryLink],
    rank_edges: &[VerifiedEdge],
    config: &DiscoveryConfig,
) -> Selection {
    if !config.enabled {
        return Selection {
            ids: baseline_ids.to_vec(),
            trace: vec!["baseline".to_string()],
        };
    }
    let mut trace = Vec::new();
    let mut baseline_rows = Vec::new();
    for id in baseline_ids {
        if let Some(row) = candidates.iter().find(|item| item.id == *id) {
            baseline_rows.push(row.clone());
        } else if let Some(row) = node_by_id(nodes, id) {
            baseline_rows.push(row.clone());
        } else {
            trace.push(format!("unresolved:{id}"));
        }
    }
    let (seeds, seed_trace) = resolve_seeds(&baseline_rows, nodes, &config.project_id);
    trace.extend(seed_trace);
    let found = discover_candidates(&seeds, nodes, links, &config.project_id, &config.budget);
    trace.extend(found.trace.clone());
    let mut pool = candidates.to_vec();
    let mut pooled: HashSet<String> = pool.iter().map(|item| item.id.clone()).collect();
    for hit in &found.hits {
        if pooled.contains(&hit.id) {
            continue;
        }
        if let Some(node) = node_by_id(nodes, &hit.id) {
            pooled.insert(node.id.clone());
            pool.push(node.clone());
        }
    }
    let (ranked, prefix) = hybrid_rank_promoted(query, baseline_ids, &pool, rank_edges, 0);
    let mut ids = ranked.ids;
    let mut present: HashSet<String> = ids.iter().cloned().collect();
    let mut insert_at = prefix;
    for hit in &found.hits {
        if !present.insert(hit.id.clone()) {
            trace.push(format!("duplicate:{}", hit.id));
            continue;
        }
        ids.insert(insert_at, hit.id.clone());
        insert_at += 1;
    }
    trace.extend(ranked.trace);
    if config.limit > 0 && ids.len() > config.limit {
        for id in ids.iter().skip(config.limit) {
            trace.push(format!("displaced:{id}"));
        }
        ids.truncate(config.limit);
    }
    Selection { ids, trace }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, name: &str, file: &str) -> CodeCandidate {
        CodeCandidate {
            id: id.to_string(),
            name: name.to_string(),
            file_path: file.to_string(),
            project_id: "ax".to_string(),
            mandatory: false,
        }
    }

    fn link(source: &str, target: &str, kind: &str, proof: &str) -> DiscoveryLink {
        DiscoveryLink {
            source: source.to_string(),
            target: target.to_string(),
            kind: kind.to_string(),
            proof: proof.to_string(),
        }
    }

    fn open_budget() -> DiscoveryBudget {
        DiscoveryBudget {
            depth: 2,
            fan_out: 8,
            max_added: 16,
            time_ms: 1_000,
        }
    }

    #[test]
    fn unique_cross_file_link_adds_the_missing_callee() {
        let caller = node("capturePage", "capturePage", "site/scripts/render-og.mjs");
        let callee = node("clipBox", "clipBox", "site/src/lib/og-card.mjs");
        let found = discover_candidates(
            &[caller],
            &[
                callee.clone(),
                node("capturePage", "capturePage", "site/scripts/render-og.mjs"),
            ],
            &[link("capturePage", "clipBox", "calls", "unique-cross-file")],
            "ax",
            &open_budget(),
        );
        assert_eq!(
            found.hits,
            vec![DiscoveryHit {
                id: "clipBox".to_string(),
                proof: "unique-cross-file".to_string(),
                kind: "calls".to_string(),
                via: "capturePage".to_string(),
                depth: 1,
            }]
        );
    }

    #[test]
    fn inferred_same_file_and_contains_links_add_nothing() {
        let caller = node("capturePage", "capturePage", "site/scripts/render-og.mjs");
        let same = node("helper", "helper", "site/scripts/render-og.mjs");
        let other = node("clipBox", "clipBox", "site/src/lib/og-card.mjs");
        let nodes = vec![caller.clone(), same, other];
        let found = discover_candidates(
            &[caller],
            &nodes,
            &[
                link("capturePage", "clipBox", "calls", "inferred"),
                link("capturePage", "helper", "calls", "unique-cross-file"),
                link("capturePage", "clipBox", "contains", "extracted"),
            ],
            "ax",
            &open_budget(),
        );
        assert!(found.hits.is_empty());
    }

    #[test]
    fn extracted_hop_then_cross_file_hop_stops_at_the_depth() {
        let caller = node(
            "pagesToRender",
            "pagesToRender",
            "site/scripts/render-og.mjs",
        );
        let local = node("capturePage", "capturePage", "site/scripts/render-og.mjs");
        let foreign = node("clipBox", "clipBox", "site/src/lib/og-card.mjs");
        let nodes = vec![caller.clone(), local, foreign];
        let links = vec![
            link("pagesToRender", "capturePage", "calls", "extracted"),
            link("capturePage", "clipBox", "calls", "unique-cross-file"),
        ];
        let one = discover_candidates(
            std::slice::from_ref(&caller),
            &nodes,
            &links,
            "ax",
            &open_budget(),
        );
        // open_budget depth is 2, so both hops are in range.
        assert_eq!(
            one.hits
                .iter()
                .map(|hit| hit.id.as_str())
                .collect::<Vec<_>>(),
            vec!["capturePage", "clipBox"]
        );
        let mut shallow = open_budget();
        shallow.depth = 1;
        let direct = discover_candidates(&[caller], &nodes, &links, "ax", &shallow);
        assert_eq!(
            direct
                .hits
                .iter()
                .map(|hit| hit.id.as_str())
                .collect::<Vec<_>>(),
            vec!["capturePage"]
        );
    }

    #[test]
    fn fan_out_prefers_extracted_links_then_the_lowest_target_id() {
        let caller = node("seed", "seed", "a.rs");
        let nodes = vec![
            caller.clone(),
            node("c", "c", "c.rs"),
            node("a", "a", "a-file.rs"),
            node("b", "b", "b.rs"),
            node("z", "z", "z.rs"),
            node("m", "m", "m.rs"),
        ];
        let mut budget = open_budget();
        budget.fan_out = 2;
        let found = discover_candidates(
            std::slice::from_ref(&caller),
            &nodes,
            &[
                link("seed", "c", "calls", "extracted"),
                link("seed", "a", "calls", "extracted"),
                link("seed", "b", "calls", "extracted"),
            ],
            "ax",
            &budget,
        );
        assert_eq!(
            found
                .hits
                .iter()
                .map(|hit| hit.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert!(found
            .trace
            .iter()
            .any(|line| line == "stopped:fan-out:seed"));
        budget.fan_out = 1;
        let preferred = discover_candidates(
            &[caller],
            &nodes,
            &[
                link("seed", "z", "calls", "unique-cross-file"),
                link("seed", "m", "calls", "extracted"),
            ],
            "ax",
            &budget,
        );
        assert_eq!(
            preferred
                .hits
                .iter()
                .map(|hit| hit.id.as_str())
                .collect::<Vec<_>>(),
            vec!["m"]
        );
    }

    #[test]
    fn fan_out_counts_only_targets_that_are_added() {
        let nodes = vec![
            node("seed", "seed", "seed.rs"),
            node("a", "a", "a.rs"),
            node("b", "b", "b.rs"),
            node("c", "c", "c.rs"),
        ];
        let mut budget = open_budget();
        budget.fan_out = 2;
        // `a` and `b` are seeds already, so only `c` is new.
        let found = discover_candidates(
            &nodes[..3],
            &nodes,
            &[
                link("seed", "a", "calls", "extracted"),
                link("seed", "b", "calls", "extracted"),
                link("seed", "c", "calls", "extracted"),
            ],
            "ax",
            &budget,
        );
        assert_eq!(
            found
                .hits
                .iter()
                .map(|hit| hit.id.as_str())
                .collect::<Vec<_>>(),
            vec!["c"]
        );
    }

    #[test]
    fn a_large_hub_is_scanned_within_the_time_budget() {
        let count = 20_000;
        let mut nodes = vec![node("hub", "hub", "hub.rs")];
        let mut links = Vec::new();
        for index in 0..count {
            let id = format!("n{index:05}");
            nodes.push(node(&id, &id, &format!("{id}.rs")));
            links.push(link("hub", &id, "calls", "unique-cross-file"));
        }
        let budget = DiscoveryBudget {
            depth: 1,
            fan_out: 8,
            max_added: 16,
            time_ms: 200,
        };
        let started = std::time::Instant::now();
        let found = discover_candidates(&nodes[..1], &nodes, &links, "ax", &budget);
        let elapsed = started.elapsed();
        assert_eq!(found.hits.len(), 8);
        // Generous for an unoptimized build: the point is that the scan is
        // not quadratic in the node count.
        assert!(elapsed.as_millis() < 1_000, "took {elapsed:?}");
    }

    #[test]
    fn a_cycle_adds_the_neighbor_once() {
        let left = node("left", "left", "left.rs");
        let right = node("right", "right", "right.rs");
        let mut budget = open_budget();
        budget.depth = 4;
        let found = discover_candidates(
            &[left],
            &[node("left", "left", "left.rs"), right],
            &[
                link("left", "right", "calls", "extracted"),
                link("right", "left", "calls", "extracted"),
            ],
            "ax",
            &budget,
        );
        assert_eq!(found.hits.len(), 1);
        assert_eq!(found.hits[0].id, "right");
    }

    #[test]
    fn a_foreign_project_is_not_added() {
        let caller = node(
            "mountMinimalDownload",
            "mountMinimalDownload",
            "site/src/scripts/minimal-download.ts",
        );
        let mut foreign = node(
            "detectInstallOS",
            "detectInstallOS",
            "site/src/lib/install-command.ts",
        );
        foreign.project_id = "other".to_string();
        let found = discover_candidates(
            &[caller],
            &[
                node(
                    "mountMinimalDownload",
                    "mountMinimalDownload",
                    "site/src/scripts/minimal-download.ts",
                ),
                foreign,
            ],
            &[link(
                "mountMinimalDownload",
                "detectInstallOS",
                "calls",
                "extracted",
            )],
            "ax",
            &open_budget(),
        );
        assert!(found.hits.is_empty());
        assert!(found
            .trace
            .iter()
            .any(|line| line == "rejected:detectInstallOS"));
    }

    #[test]
    fn max_added_stops_and_a_zero_time_budget_adds_nothing() {
        let caller = node("seed", "seed", "a.rs");
        let nodes = vec![
            caller.clone(),
            node("a", "a", "a-file.rs"),
            node("b", "b", "b.rs"),
            node("c", "c", "c.rs"),
        ];
        let links = vec![
            link("seed", "a", "calls", "extracted"),
            link("seed", "b", "calls", "extracted"),
            link("seed", "c", "calls", "extracted"),
        ];
        let mut capped = open_budget();
        capped.max_added = 1;
        let found =
            discover_candidates(std::slice::from_ref(&caller), &nodes, &links, "ax", &capped);
        assert_eq!(found.hits.len(), 1);
        assert_eq!(found.hits[0].id, "a");
        assert!(found.trace.iter().any(|line| line == "stopped:candidates"));
        let mut expired = open_budget();
        expired.time_ms = 0;
        let none = discover_candidates(&[caller], &nodes, &links, "ax", &expired);
        assert!(none.hits.is_empty());
        assert!(none.trace.iter().any(|line| line == "stopped:time"));
    }

    #[test]
    fn resolve_seeds_uses_one_name_and_refuses_two() {
        let baseline = node("mem", "clipBox", "query.rs");
        let unique = node("clipBox", "clipBox", "site/src/lib/og-card.mjs");
        let (seeds, trace) = resolve_seeds(std::slice::from_ref(&baseline), &[unique], "ax");
        assert_eq!(seeds.len(), 1);
        assert_eq!(seeds[0].id, "clipBox");
        assert!(trace.iter().any(|line| line == "seed:clipBox:name"));
        let twin = node("clipBoxJs", "clipBox", "site/src/lib/og-card.js");
        let (none, unresolved) = resolve_seeds(
            &[baseline],
            &[node("clipBox", "clipBox", "site/src/lib/og-card.mjs"), twin],
            "ax",
        );
        assert!(none.is_empty());
        assert!(unresolved.iter().any(|line| line == "unresolved:mem"));
    }

    #[test]
    fn expand_puts_the_missing_callee_inside_the_budget_and_stays_off_when_disabled() {
        let caller = node("capturePage", "capturePage", "site/scripts/render-og.mjs");
        let noise = node("noise", "noise", "noise.rs");
        let callee = node("clipBox", "clipBox", "site/src/lib/og-card.mjs");
        let candidates = vec![caller.clone(), noise.clone()];
        let nodes = vec![caller, noise, callee];
        let links = vec![link("capturePage", "clipBox", "calls", "unique-cross-file")];
        let baseline = vec!["capturePage".to_string(), "noise".to_string()];
        let off = expand_with_graph(
            "capturePage",
            &baseline,
            &candidates,
            &nodes,
            &links,
            &[],
            &DiscoveryConfig::default(),
        );
        assert_eq!(off.ids, baseline);
        assert_eq!(off.trace, vec!["baseline".to_string()]);
        let on = expand_with_graph(
            "capturePage",
            &baseline,
            &candidates,
            &nodes,
            &links,
            &[],
            &DiscoveryConfig {
                enabled: true,
                project_id: "ax".to_string(),
                limit: 2,
                budget: open_budget(),
            },
        );
        assert_eq!(
            on.ids,
            vec!["capturePage".to_string(), "clipBox".to_string()]
        );
        assert!(on.trace.iter().any(|line| {
            line == "discovered:clipBox:calls:unique-cross-file:via=capturePage:depth=1"
        }));
        assert!(on.trace.iter().any(|line| line == "displaced:noise"));
        let already = expand_with_graph(
            "capturePage",
            &["capturePage".to_string(), "clipBox".to_string()],
            &nodes,
            &nodes,
            &links,
            &[],
            &DiscoveryConfig {
                enabled: true,
                project_id: "ax".to_string(),
                limit: 0,
                budget: open_budget(),
            },
        );
        assert_eq!(
            already
                .ids
                .iter()
                .filter(|id| id.as_str() == "clipBox")
                .count(),
            1
        );
    }

    fn ranked_metrics(ranked: &[String], gold: &[String]) -> (f64, f64, f64, f64) {
        let mut reciprocal = 0.0;
        for (index, id) in ranked.iter().enumerate() {
            if gold.iter().any(|item| item == id) {
                reciprocal = 1.0 / (index as f64 + 1.0);
                break;
            }
        }
        let mut dcg = 0.0;
        let mut hits = 0.0;
        for (index, id) in ranked.iter().take(10).enumerate() {
            if gold.iter().any(|item| item == id) {
                dcg += 1.0 / ((index as f64) + 2.0).ln();
                if index < 3 {
                    hits += 1.0;
                }
            }
        }
        let ideal_len = gold.len().min(10);
        let mut ideal = 0.0;
        for index in 0..ideal_len {
            ideal += 1.0 / ((index as f64) + 2.0).ln();
        }
        let ndcg = if ideal == 0.0 { 0.0 } else { dcg / ideal };
        let precision = hits / 3.0;
        let recall = if gold.is_empty() {
            0.0
        } else {
            hits / gold.len() as f64
        };
        (reciprocal, ndcg, precision, recall)
    }

    fn pool_recall(pool: &[String], gold: &[String]) -> f64 {
        let hits = gold
            .iter()
            .filter(|id| pool.iter().any(|item| item == *id))
            .count();
        hits as f64 / gold.len() as f64
    }

    /// Thresholds are the independent protocol. Do not edit them after the recorded run.
    #[tokio::test]
    #[ignore]
    async fn independent_graph_discovery() {
        let _env = crate::BENCH_ENV.lock().await;
        for name in [
            "AX_MEMORY_BENCH_RAW",
            "AX_MEMORY_BENCH_SKIP_RELATIVE_COVERAGE",
            "AX_MEMORY_BENCH_IDENTIFIER_STEM",
            "AX_MEMORY_BENCH_IDENTIFIER_BOUNDARY",
            "AX_MEMORY_BENCH_IDENTIFIER_AFFIX",
        ] {
            std::env::remove_var(name);
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/memory/graph-discovery-independent.json");
        let bytes = std::fs::read(&path).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["protocol"], "graph-discovery-independent-1");
        assert_eq!(
            value["snapshot"],
            "85fa176a9b4b6ce5e33687bee268767d87731c60"
        );
        let tasks = value["tasks"].as_array().unwrap();
        let gold_tasks = tasks
            .iter()
            .filter(|task| !task["relevant"].as_array().unwrap().is_empty())
            .count();
        assert!(gold_tasks >= 20);
        let candidates_json = value["candidates"].as_array().unwrap();
        assert!(candidates_json.len() <= 800);
        for item in candidates_json {
            let file = item["file"].as_str().unwrap();
            assert!(!file.starts_with("crates/ax-memory/"));
            assert!(!file.starts_with("crates/ax-neural/"));
            assert!(!file.starts_with("site/"));
        }
        let mut rows = Vec::new();
        for item in candidates_json {
            rows.push(CodeCandidate {
                id: item["id"].as_str().unwrap().to_string(),
                name: item["name"].as_str().unwrap().to_string(),
                file_path: item["file"].as_str().unwrap().to_string(),
                project_id: String::new(),
                mandatory: false,
            });
        }
        let links: Vec<DiscoveryLink> = value["links"]
            .as_array()
            .unwrap()
            .iter()
            .map(|link| DiscoveryLink {
                source: link["source"].as_str().unwrap().to_string(),
                target: link["target"].as_str().unwrap().to_string(),
                kind: link["kind"].as_str().unwrap().to_string(),
                proof: link["proof"].as_str().unwrap().to_string(),
            })
            .collect();
        let dir = tempfile::tempdir().unwrap();
        let database = ax_db::Database::open(&dir.path().join("graph-discovery.db"))
            .await
            .unwrap();
        let pool = database.pool();
        for row in &rows {
            let signature = candidates_json
                .iter()
                .find(|item| item["id"] == row.id)
                .unwrap()["signature"]
                .as_str()
                .unwrap();
            ax_memory::bench::remember_stable(
                pool,
                &row.id,
                ax_memory::RememberInput {
                    title: String::new(),
                    body: signature.to_string(),
                    kind: Some("note".into()),
                    tags: Vec::new(),
                    files: Vec::new(),
                    source: None,
                },
            )
            .await
            .unwrap();
        }
        let config = DiscoveryConfig {
            enabled: true,
            project_id: String::new(),
            limit: 0,
            budget: DiscoveryBudget::default(),
        };
        let mut sums = std::collections::BTreeMap::new();
        let mut kind_sums = std::collections::BTreeMap::new();
        let mut counted = std::collections::BTreeMap::new();
        let mut kind_counted = std::collections::BTreeMap::new();
        let mut abstain = std::collections::BTreeMap::new();
        let mut added_negative = std::collections::BTreeMap::new();
        let mut false_positive = std::collections::BTreeMap::new();
        let mut retrieval_ms = std::collections::BTreeMap::new();
        let mut traversal_ms = std::collections::BTreeMap::new();
        let arms = ["production", "graph_discovery"];
        for arm in arms {
            sums.insert(arm, [0.0; 5]);
            abstain.insert(arm, 0.0);
            added_negative.insert(arm, 0.0);
            false_positive.insert(arm, 0.0);
            retrieval_ms.insert(arm, 0u128);
            traversal_ms.insert(arm, 0u128);
        }
        let mut negatives = 0.0;
        let rss_before = crate::rss::process_rss_bytes().unwrap_or(0);
        for task in tasks {
            let kind = task["kind"].as_str().unwrap();
            let query = task["query"].as_str().unwrap();
            let gold: Vec<String> = task["relevant"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_str().unwrap().to_string())
                .collect();
            let started = std::time::Instant::now();
            let recalled = ax_memory::recall_for_prompt(pool, query, 10).await.unwrap();
            let recall_elapsed = started.elapsed().as_millis();
            let baseline: Vec<String> = recalled.iter().map(|hit| hit.memory.id.clone()).collect();
            let started = std::time::Instant::now();
            let expanded = expand_with_graph(query, &baseline, &rows, &rows, &links, &[], &config);
            let expand_elapsed = started.elapsed().as_millis();
            let discovered: Vec<String> = expanded
                .ids
                .iter()
                .filter(|id| !baseline.iter().any(|kept| kept == *id))
                .cloned()
                .collect();
            let ranked_discovery: Vec<String> = expanded.ids.iter().take(10).cloned().collect();
            let production_pool = baseline.clone();
            let mut discovery_pool = baseline.clone();
            for id in &discovered {
                if !discovery_pool.iter().any(|kept| kept == id) {
                    discovery_pool.push(id.clone());
                }
            }
            let lists = [
                (
                    "production",
                    production_pool,
                    baseline.clone(),
                    Vec::<String>::new(),
                ),
                (
                    "graph_discovery",
                    discovery_pool,
                    ranked_discovery,
                    discovered,
                ),
            ];
            *retrieval_ms.get_mut("production").unwrap() += recall_elapsed;
            *retrieval_ms.get_mut("graph_discovery").unwrap() += recall_elapsed;
            *traversal_ms.get_mut("graph_discovery").unwrap() += expand_elapsed;
            if gold.is_empty() {
                negatives += 1.0;
                for (arm, pool, _ranked, added) in &lists {
                    if pool.is_empty() {
                        *abstain.get_mut(*arm).unwrap() += 1.0;
                    }
                    *added_negative.get_mut(*arm).unwrap() += added.len() as f64;
                }
                continue;
            }
            for (arm, pool, ranked, added) in &lists {
                let scores = ranked_metrics(ranked, &gold);
                let total = sums.get_mut(*arm).unwrap();
                total[0] += scores.0;
                total[1] += scores.1;
                total[2] += scores.2;
                total[3] += scores.3;
                total[4] += pool_recall(pool, &gold);
                *counted.entry(*arm).or_insert(0.0) += 1.0;
                let by_kind = kind_sums.entry((*arm, kind)).or_insert([0.0; 4]);
                by_kind[0] += scores.0;
                by_kind[1] += scores.1;
                by_kind[2] += scores.2;
                by_kind[3] += scores.3;
                *kind_counted.entry((*arm, kind)).or_insert(0.0) += 1.0;
                let extras = added
                    .iter()
                    .filter(|id| !gold.iter().any(|item| item == *id))
                    .count();
                *false_positive.get_mut(*arm).unwrap() += extras as f64;
            }
        }
        let rss_after = crate::rss::process_rss_bytes().unwrap_or(0);
        let mut arms_json = serde_json::Map::new();
        for arm in arms {
            let n = *counted.get(arm).unwrap();
            let total = sums[arm];
            let mut kind_json = serde_json::Map::new();
            for ((name, kind), kind_total) in &kind_sums {
                if *name != arm {
                    continue;
                }
                let count = kind_counted[&(*name, *kind)];
                kind_json.insert(
                    (*kind).to_string(),
                    serde_json::json!({
                        "tasks": count,
                        "mrr": kind_total[0] / count,
                        "ndcg": kind_total[1] / count,
                        "precisionAt3": kind_total[2] / count,
                        "recallAt3": kind_total[3] / count,
                    }),
                );
            }
            let task_count = tasks.len() as f64;
            arms_json.insert(
                arm.to_string(),
                serde_json::json!({
                    "tasksWithGold": n,
                    "mrr": total[0] / n,
                    "ndcg": total[1] / n,
                    "precisionAt3": total[2] / n,
                    "recallAt3": total[3] / n,
                    "candidateRecall": total[4] / n,
                    "negativeQueries": negatives,
                    "abstained": abstain[arm],
                    "negativeAdditions": added_negative[arm],
                    "falsePositiveAdditions": false_positive[arm],
                    "meanRetrievalMillis": retrieval_ms[arm] as f64 / task_count,
                    "meanTraversalMillis": traversal_ms[arm] as f64 / task_count,
                    "meanEndToEndMillis": (retrieval_ms[arm] + traversal_ms[arm]) as f64 / task_count,
                    "kinds": kind_json,
                }),
            );
        }
        let production = arms_json["production"].as_object().unwrap();
        let discovery = arms_json["graph_discovery"].as_object().unwrap();
        let metric = |arm: &serde_json::Map<String, serde_json::Value>, key: &str| {
            arm[key].as_f64().unwrap()
        };
        let mut kind_ok = true;
        let discovery_kinds = discovery["kinds"].as_object().unwrap();
        let production_kinds = production["kinds"].as_object().unwrap();
        for (kind, row) in discovery_kinds {
            let Some(other) = production_kinds.get(kind) else {
                continue;
            };
            if metric(row.as_object().unwrap(), "mrr") + 0.05 + 1e-9
                < metric(other.as_object().unwrap(), "mrr")
            {
                kind_ok = false;
            }
        }
        let promoted = gold_tasks >= 20
            && metric(discovery, "candidateRecall") >= metric(production, "candidateRecall") + 0.05
            && metric(discovery, "mrr") >= metric(production, "mrr") + 0.02
            && metric(discovery, "recallAt3") + 1e-9 >= metric(production, "recallAt3") - 0.01
            && metric(discovery, "precisionAt3") + 1e-9 >= metric(production, "precisionAt3")
            && metric(discovery, "ndcg") + 1e-9 >= metric(production, "ndcg") - 0.01
            && kind_ok
            && metric(discovery, "abstained") + 1e-9 >= metric(production, "abstained")
            && metric(discovery, "negativeAdditions") <= 0.0
            && metric(discovery, "meanTraversalMillis") <= 100.0
            && metric(discovery, "meanEndToEndMillis")
                <= metric(production, "meanEndToEndMillis") + 100.0
            && rss_after.saturating_sub(rss_before) <= 64 * 1024 * 1024;
        let report = serde_json::json!({
            "snapshot": value["snapshot"],
            "protocol": value["protocol"],
            "inputBlake3": blake3::hash(&bytes).to_hex().to_string(),
            "promoted": promoted,
            "rssBytesBefore": rss_before,
            "rssBytesAfter": rss_after,
            "arms": arms_json,
        });
        let text = serde_json::to_string_pretty(&report).unwrap();
        std::fs::write(
            crate::measurement_output("graph-discovery-results.json"),
            text.as_bytes(),
        )
        .unwrap();
        // The checked-in record changes only when asked for explicitly.
        if std::env::var_os("AX_BENCH_RECORD").is_some() {
            let destination = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs/specs/ax-neural/GRAPH-DISCOVERY-RESULTS.json");
            std::fs::write(&destination, text.as_bytes()).unwrap();
        }
        println!("{text}");
    }
}
