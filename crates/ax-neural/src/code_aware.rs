//! Experimental code-aware ordering. Production rank does not call it.
//! A candidate moves only for exact symbol identity or an extracted graph edge.
//! Shared prefixes and suffixes do not count.

#[derive(Clone, Debug)]
pub struct CodeCandidate {
    pub id: String,
    pub name: String,
    pub file_path: String,
    pub project_id: String,
    pub mandatory: bool,
}

#[derive(Clone, Debug)]
pub struct VerifiedEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub confidence: String,
}

#[derive(Clone, Debug, Default)]
pub struct CodeAwareConfig {
    pub enabled: bool,
    pub project_id: String,
    pub limit: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub ids: Vec<String>,
    pub trace: Vec<String>,
}

pub fn symbol_key(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

pub fn exact_symbol(query: &str, name: &str) -> bool {
    let wanted = symbol_key(name);
    if wanted.is_empty() {
        return false;
    }
    query_tokens(query).any(|token| symbol_key(token) == wanted)
}

pub fn select(
    query: &str,
    candidates: &[CodeCandidate],
    edges: &[VerifiedEdge],
    config: &CodeAwareConfig,
) -> Selection {
    if !config.enabled {
        return Selection {
            ids: candidates.iter().map(|item| item.id.clone()).collect(),
            trace: vec!["baseline".to_string()],
        };
    }
    let mut trace = Vec::new();
    let allowed: Vec<&CodeCandidate> = candidates
        .iter()
        .filter(|item| {
            let foreign = !config.project_id.is_empty()
                && !item.project_id.is_empty()
                && item.project_id != config.project_id;
            if foreign {
                trace.push(format!("rejected:{}", item.id));
            }
            !foreign
        })
        .collect();
    let exact: Vec<String> = allowed
        .iter()
        .filter(|item| exact_symbol(query, &item.name))
        .map(|item| item.id.clone())
        .collect();
    for id in &exact {
        trace.push(format!("exact:{id}"));
    }
    let mut linked = Vec::new();
    for edge in edges {
        if !verified(edge) {
            continue;
        }
        let other = if exact.iter().any(|id| id == &edge.source) {
            Some(edge.target.as_str())
        } else if exact.iter().any(|id| id == &edge.target) {
            Some(edge.source.as_str())
        } else {
            None
        };
        let Some(other) = other else {
            continue;
        };
        if exact.iter().any(|id| id == other) || linked.iter().any(|id: &String| id == other) {
            continue;
        }
        if allowed.iter().any(|item| item.id == other) {
            linked.push(other.to_string());
            trace.push(format!("edge:{other}"));
        }
    }
    let mut ids = Vec::new();
    for item in &allowed {
        if item.mandatory && !ids.iter().any(|id| id == &item.id) {
            ids.push(item.id.clone());
            trace.push(format!("mandatory:{}", item.id));
        }
    }
    for id in exact.into_iter().chain(linked) {
        if !ids.iter().any(|kept| kept == &id) {
            ids.push(id);
        }
    }
    for item in &allowed {
        if !ids.iter().any(|id| id == &item.id) {
            ids.push(item.id.clone());
        }
    }
    if config.limit > 0 && ids.len() > config.limit {
        let kept_mandatory = ids.iter().filter(|id| is_mandatory(candidates, id)).count();
        let room = config.limit.max(kept_mandatory);
        for id in ids.iter().skip(room) {
            trace.push(format!("displaced:{id}"));
        }
        ids.truncate(room);
    }
    Selection { ids, trace }
}

/// Production order, with exact symbols and extracted edges moved ahead.
/// A positive limit keeps that prefix and records baseline ids that left it.
/// When nothing is verified and the baseline is empty, the result is empty.
pub fn hybrid_rank(
    query: &str,
    baseline: &[String],
    candidates: &[CodeCandidate],
    edges: &[VerifiedEdge],
    limit: usize,
) -> Selection {
    let selected = select(
        query,
        candidates,
        edges,
        &CodeAwareConfig {
            enabled: true,
            project_id: String::new(),
            limit: 0,
        },
    );
    let mut ids = Vec::new();
    for line in &selected.trace {
        let Some(id) = line
            .strip_prefix("exact:")
            .or_else(|| line.strip_prefix("edge:"))
        else {
            continue;
        };
        if ids.iter().any(|kept| kept == id) {
            continue;
        }
        if candidates.iter().any(|item| item.id == id) {
            ids.push(id.to_string());
        }
    }
    for id in baseline {
        if !ids.iter().any(|kept| kept == id) {
            ids.push(id.clone());
        }
    }
    let mut trace = selected.trace;
    if limit > 0 && ids.len() > limit {
        for id in baseline.iter().take(limit) {
            if !ids.iter().take(limit).any(|kept| kept == id) {
                trace.push(format!("displaced:{id}"));
            }
        }
        ids.truncate(limit);
    }
    Selection { ids, trace }
}

fn is_mandatory(candidates: &[CodeCandidate], id: &str) -> bool {
    candidates
        .iter()
        .any(|item| item.id == id && item.mandatory)
}

fn verified(edge: &VerifiedEdge) -> bool {
    matches!(edge.kind.as_str(), "calls" | "references" | "documents")
        && matches!(edge.confidence.as_str(), "extracted" | "exact")
}

fn query_tokens(query: &str) -> impl Iterator<Item = &str> {
    query
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, name: &str) -> CodeCandidate {
        CodeCandidate {
            id: id.to_string(),
            name: name.to_string(),
            file_path: format!("{id}.rs"),
            project_id: "ax".to_string(),
            mandatory: false,
        }
    }

    #[test]
    fn camel_case_and_snake_case_are_the_same_symbol() {
        assert!(exact_symbol("parseXml now", "parse_xml"));
        assert!(!exact_symbol("policy rows", "policy_rows"));
        assert!(!exact_symbol("the publishers", "publish_for_test"));
        assert!(!exact_symbol("policySync is on", "policy_sync_enabled"));
    }

    #[test]
    fn disabled_selection_keeps_baseline_order() {
        let rows = vec![candidate("a", "alpha"), candidate("b", "beta")];
        let selection = select("beta", &rows, &[], &CodeAwareConfig::default());
        assert_eq!(selection.ids, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(selection.trace, vec!["baseline".to_string()]);
    }

    #[test]
    fn exact_symbol_moves_ahead_of_a_prefix_neighbor() {
        let rows = vec![
            candidate("rows", "policy_rows"),
            candidate("xml", "parse_xml"),
            candidate("other", "unrelated"),
        ];
        let config = CodeAwareConfig {
            enabled: true,
            project_id: "ax".to_string(),
            limit: 0,
        };
        let selection = select("parse the parseXml path", &rows, &[], &config);
        assert_eq!(
            selection.ids,
            vec!["xml".to_string(), "rows".to_string(), "other".to_string()]
        );
    }

    #[test]
    fn extracted_call_follows_the_exact_symbol_and_an_inferred_edge_does_not() {
        let rows = vec![
            candidate("tail", "tail"),
            candidate("caller", "parse_xml"),
            candidate("callee", "write_bytes"),
            candidate("guess", "guess_bytes"),
        ];
        let edges = vec![
            VerifiedEdge {
                source: "caller".to_string(),
                target: "callee".to_string(),
                kind: "calls".to_string(),
                confidence: "extracted".to_string(),
            },
            VerifiedEdge {
                source: "caller".to_string(),
                target: "guess".to_string(),
                kind: "calls".to_string(),
                confidence: "inferred".to_string(),
            },
        ];
        let config = CodeAwareConfig {
            enabled: true,
            project_id: "ax".to_string(),
            limit: 0,
        };
        let selection = select("parseXml", &rows, &edges, &config);
        assert_eq!(
            selection.ids,
            vec![
                "caller".to_string(),
                "callee".to_string(),
                "tail".to_string(),
                "guess".to_string()
            ]
        );
    }

    #[test]
    fn same_file_without_an_edge_does_not_promote_a_sibling() {
        let mut sibling = candidate("sib", "other_fn");
        sibling.file_path = "same.rs".to_string();
        let mut exact = candidate("exact", "parse_xml");
        exact.file_path = "same.rs".to_string();
        let rows = vec![candidate("first", "first"), exact, sibling];
        let config = CodeAwareConfig {
            enabled: true,
            project_id: "ax".to_string(),
            limit: 0,
        };
        let selection = select("parseXml", &rows, &[], &config);
        assert_eq!(
            selection.ids,
            vec!["exact".to_string(), "first".to_string(), "sib".to_string()]
        );
    }

    #[test]
    fn mandatory_context_stays_ahead_and_a_foreign_project_is_omitted() {
        let mut required = candidate("rule", "house_rule");
        required.mandatory = true;
        let mut foreign = candidate("other", "parse_xml");
        foreign.project_id = "other".to_string();
        let rows = vec![candidate("local", "parse_xml"), foreign, required];
        let config = CodeAwareConfig {
            enabled: true,
            project_id: "ax".to_string(),
            limit: 2,
        };
        let selection = select("parseXml", &rows, &[], &config);
        assert_eq!(selection.ids, vec!["rule".to_string(), "local".to_string()]);
        assert!(selection.trace.iter().any(|line| line == "rejected:other"));
        assert!(selection.trace.iter().any(|line| line == "mandatory:rule"));
        assert!(selection.trace.iter().any(|line| line == "exact:local"));
        assert!(selection
            .trace
            .iter()
            .all(|line| !line.starts_with("displaced:")));
    }

    #[test]
    fn a_verified_hit_records_the_baseline_items_it_displaces() {
        let rows = vec![
            candidate("noise", "noise"),
            candidate("xml", "parse_xml"),
            candidate("later", "later"),
        ];
        let config = CodeAwareConfig {
            enabled: true,
            project_id: "ax".to_string(),
            limit: 1,
        };
        let selection = select("parseXml", &rows, &[], &config);
        assert_eq!(selection.ids, vec!["xml".to_string()]);
        assert!(selection.trace.iter().any(|line| line == "displaced:noise"));
        assert!(selection.trace.iter().any(|line| line == "displaced:later"));
    }

    #[test]
    fn hybrid_rank_puts_an_exact_symbol_ahead_of_the_baseline_and_abstains_when_both_are_empty() {
        let rows = vec![
            candidate("xml", "parse_xml"),
            candidate("rows", "policy_rows"),
        ];
        let promoted = hybrid_rank("parseXml", &["rows".to_string()], &rows, &[], 2);
        assert_eq!(promoted.ids, vec!["xml".to_string(), "rows".to_string()]);
        let quiet = hybrid_rank("weather in Utrecht", &[], &rows, &[], 2);
        assert!(quiet.ids.is_empty());
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

    #[tokio::test]
    #[ignore]
    async fn frozen_code_retrieval_comparison() {
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
            .join("../../benchmarks/memory/code-retrieval-frozen.json");
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            value["snapshot"],
            "85fa176a9b4b6ce5e33687bee268767d87731c60"
        );
        let candidates_json = value["candidates"].as_array().unwrap();
        let tasks = value["tasks"].as_array().unwrap();
        assert!(tasks.len() >= 60);
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
        rows.sort_by(|left, right| left.id.cmp(&right.id));
        let edges: Vec<VerifiedEdge> = value["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|edge| VerifiedEdge {
                source: edge["source"].as_str().unwrap().to_string(),
                target: edge["target"].as_str().unwrap().to_string(),
                kind: edge["kind"].as_str().unwrap().to_string(),
                confidence: edge["confidence"].as_str().unwrap().to_string(),
            })
            .collect();
        let dir = tempfile::tempdir().unwrap();
        let database = ax_db::Database::open(&dir.path().join("code-retrieval.db"))
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
        let mut report = serde_json::Map::new();
        let arms = ["production", "affix", "graph_hybrid"];
        let mut sums = std::collections::BTreeMap::new();
        let mut kind_sums = std::collections::BTreeMap::new();
        let mut counted = std::collections::BTreeMap::new();
        let mut kind_counted = std::collections::BTreeMap::new();
        let mut abstain = std::collections::BTreeMap::new();
        let mut negatives = 0.0;
        let mut millis = std::collections::BTreeMap::new();
        let mut rss = std::collections::BTreeMap::new();
        for arm in arms {
            sums.insert(arm, [0.0; 4]);
            abstain.insert(arm, 0.0);
            millis.insert(arm, 0u128);
            rss.insert(arm, crate::rss::process_rss_bytes().unwrap_or(0));
        }
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
            let production = ax_memory::recall_for_prompt(pool, query, 10).await.unwrap();
            *millis.get_mut("production").unwrap() += started.elapsed().as_millis();
            std::env::set_var("AX_MEMORY_BENCH_IDENTIFIER_AFFIX", "1");
            let started = std::time::Instant::now();
            let affix = ax_memory::recall_for_prompt(pool, query, 10).await.unwrap();
            *millis.get_mut("affix").unwrap() += started.elapsed().as_millis();
            std::env::remove_var("AX_MEMORY_BENCH_IDENTIFIER_AFFIX");
            let baseline: Vec<String> =
                production.iter().map(|hit| hit.memory.id.clone()).collect();
            let started = std::time::Instant::now();
            let hybrid = hybrid_rank(query, &baseline, &rows, &edges, 10);
            *millis.get_mut("graph_hybrid").unwrap() += started.elapsed().as_millis();
            let lists = [
                ("production", baseline),
                (
                    "affix",
                    affix.iter().map(|hit| hit.memory.id.clone()).collect(),
                ),
                ("graph_hybrid", hybrid.ids),
            ];
            if gold.is_empty() {
                negatives += 1.0;
                for (arm, ids) in &lists {
                    let arm: &str = arm;
                    if ids.is_empty() {
                        *abstain.get_mut(arm).unwrap() += 1.0;
                    }
                }
                continue;
            }
            for (arm, ids) in &lists {
                let arm: &str = arm;
                let scores = ranked_metrics(ids, &gold);
                let total = sums.get_mut(arm).unwrap();
                total[0] += scores.0;
                total[1] += scores.1;
                total[2] += scores.2;
                total[3] += scores.3;
                *counted.entry(arm).or_insert(0.0) += 1.0;
                let by_kind = kind_sums.entry((arm, kind)).or_insert([0.0; 4]);
                by_kind[0] += scores.0;
                by_kind[1] += scores.1;
                by_kind[2] += scores.2;
                by_kind[3] += scores.3;
                *kind_counted.entry((arm, kind)).or_insert(0.0) += 1.0;
            }
        }
        std::env::remove_var("AX_MEMORY_BENCH_IDENTIFIER_AFFIX");
        let mut arms_json = serde_json::Map::new();
        for arm in arms {
            let n = *counted.get(arm).unwrap();
            let total = sums[arm];
            let mut kind_json = serde_json::Map::new();
            for ((name, kind), total) in &kind_sums {
                if *name != arm {
                    continue;
                }
                let count = kind_counted[&(*name, *kind)];
                kind_json.insert(
                    (*kind).to_string(),
                    serde_json::json!({
                        "tasks": count,
                        "mrr": total[0] / count,
                        "ndcg": total[1] / count,
                        "precisionAt3": total[2] / count,
                        "recallAt3": total[3] / count,
                    }),
                );
            }
            arms_json.insert(
                arm.to_string(),
                serde_json::json!({
                    "tasksWithGold": n,
                    "mrr": total[0] / n,
                    "ndcg": total[1] / n,
                    "precisionAt3": total[2] / n,
                    "recallAt3": total[3] / n,
                    "negativeQueries": negatives,
                    "abstained": abstain[arm],
                    "latencyMillis": millis[arm],
                    "rssBytesAfter": crate::rss::process_rss_bytes().unwrap_or(0),
                    "rssBytesBefore": rss[arm],
                    "kinds": kind_json,
                }),
            );
        }
        report.insert("snapshot".to_string(), value["snapshot"].clone());
        report.insert("promoted".to_string(), serde_json::json!(false));
        report.insert("arms".to_string(), serde_json::Value::Object(arms_json));
        let text = serde_json::to_string_pretty(&report).unwrap();
        std::fs::write("/tmp/code-aware-retrieval-results.json", &text).unwrap();
        println!("{text}");
    }
}
