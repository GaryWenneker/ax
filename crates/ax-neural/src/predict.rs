use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct Observation {
    pub task_category: String,
    pub skills: Vec<String>,
    pub rules: Vec<String>,
    pub context_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Prediction {
    pub kind: String,
    pub target_id: String,
    pub confidence: f64,
    pub label: String,
    /// Human-readable reason for this prediction.
    pub explanation: String,
    /// Supporting evidence strings (counts, edge ids, execution ids).
    pub evidence: Vec<String>,
    /// `inferred` from co-occurrence, or `verified` when a graph edge confirms the link.
    pub relationship: String,
}

/// Most predictions one call returns, highest confidence first.
pub const MAX_PREDICTIONS: usize = 50;

pub fn predict(rows: &[Observation], task: &str, minimum_confidence: f64) -> Vec<Prediction> {
    predict_with_edges(rows, task, minimum_confidence, &[])
}

/// Same as [`predict`], but marks a prediction `relationship: verified` when a graph edge
/// joins its target to another id that this task predicts or that its matching feedback
/// rows used as context. Edges never become predictions on their own. At most
/// [`MAX_PREDICTIONS`] predictions are returned.
/// True when some feedback row matches `task`, the only case in which
/// `predict_with_edges` reads its edges.
pub fn any_matches(rows: &[Observation], task: &str) -> bool {
    rows.iter().any(|row| category_matches(row, task))
}

pub fn predict_with_edges(
    rows: &[Observation],
    task: &str,
    minimum_confidence: f64,
    edges: &[(String, String)],
) -> Vec<Prediction> {
    let matched: Vec<&Observation> = rows
        .iter()
        .filter(|row| category_matches(row, task))
        .collect();
    if matched.is_empty() {
        return Vec::new();
    }
    let mut counts: HashMap<(&'static str, &str), usize> = HashMap::new();
    let mut pattern_counts: HashMap<&str, usize> = HashMap::new();
    for row in &matched {
        tally(
            &mut counts,
            row.skills.iter().map(|id| ("skill", id.as_str())),
        );
        tally(
            &mut counts,
            row.rules.iter().map(|id| ("rule", id.as_str())),
        );
        tally(
            &mut counts,
            row.context_ids.iter().map(|id| context_kind(id)),
        );
        if !row.task_category.is_empty() {
            *pattern_counts
                .entry(row.task_category.as_str())
                .or_insert(0) += 1;
        }
    }
    // Ids this task touches: every counted target plus the matching rows' context ids.
    let mut known: HashSet<&str> = counts.keys().map(|(_, id)| *id).collect();
    for row in &matched {
        known.extend(row.context_ids.iter().map(String::as_str));
    }
    // For each known id, the other known ids an edge joins it to.
    let mut linked: HashMap<&str, Vec<&str>> = HashMap::new();
    for (source, target) in edges {
        let (source, target) = (source.as_str(), target.as_str());
        if source.is_empty() || target.is_empty() || source == target {
            continue;
        }
        if known.contains(source) && known.contains(target) {
            linked.entry(source).or_default().push(target);
            linked.entry(target).or_default().push(source);
        }
    }
    let total = matched.len() as f64;
    let mut out: Vec<Prediction> = Vec::new();
    let mut push = |kind: &str, target_id: String, count: usize, confidence: f64| {
        if confidence + f64::EPSILON < minimum_confidence {
            return;
        }
        let explanation = format!(
            "{kind} `{target_id}` co-occurred in {count} of {} feedback rows matching this task (frequency prior; not neural inference)",
            matched.len()
        );
        let mut evidence = vec![format!(
            "seen in {count}/{} matching execution(s) for task category filter",
            matched.len()
        )];
        let mut relationship = "inferred";
        if kind != "pattern" {
            if let Some(others) = linked.get(target_id.as_str()) {
                relationship = "verified";
                let mut others = others.clone();
                others.sort_unstable();
                others.dedup();
                evidence.extend(
                    others
                        .iter()
                        .take(5)
                        .map(|other| format!("graph edge joins `{target_id}` and `{other}`")),
                );
            }
        }
        out.push(Prediction {
            kind: kind.to_string(),
            target_id,
            confidence,
            label: "predicted".into(),
            explanation,
            evidence,
            relationship: relationship.into(),
        });
    };
    for ((kind, target_id), count) in &counts {
        push(kind, target_id.to_string(), *count, *count as f64 / total);
    }
    for (category, count) in pattern_counts {
        push(
            "pattern",
            format!("task:{category}"),
            count,
            count as f64 / total,
        );
    }
    drop(push);
    out.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.target_id.cmp(&b.target_id))
    });
    out.truncate(MAX_PREDICTIONS);
    out
}

pub const EVALUATION_SOURCE: &str = "coding fixture, leave-one-out";

/// Holds out each fixture question in turn and predicts its memories from the others,
/// using the question family as the task category.
pub fn evaluate_fixture(text: &str, minimum_confidence: f64) -> Result<Value, String> {
    if !(0.0..=1.0).contains(&minimum_confidence) {
        return Err("minimum confidence must be between 0 and 1".into());
    }
    let fixture = ax_memory::bench::load_coding_fixture(text)?;
    let rows: Vec<Observation> = fixture
        .questions
        .iter()
        .map(|question| Observation {
            task_category: question.family.clone(),
            skills: Vec::new(),
            rules: Vec::new(),
            context_ids: question
                .relevant
                .iter()
                .map(|id| format!("memory:{id}"))
                .collect(),
        })
        .collect();
    let mut score = HeldOut::default();
    let mut predicted_ids_total = 0usize;
    let mut retrieved_ids_total = 0usize;
    let mut used_ids_total = 0usize;
    let mut predicted_and_used = 0usize;
    let mut retrieved_and_used = 0usize;
    let mut predicted_not_retrieved = 0usize;
    for (index, question) in fixture.questions.iter().enumerate() {
        let training: Vec<Observation> = rows
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .map(|(_, row)| row.clone())
            .collect();
        let predicted = predict(&training, &question.family, minimum_confidence);
        score.add(&predicted, &question.relevant);

        let predicted_keys: Vec<String> = predicted
            .iter()
            .filter(|p| p.kind == "memory")
            .map(|p| p.target_id.clone())
            .collect();
        let retrieved = hash_retrieve_top(&fixture, &question.prompt, 5);
        let used = &question.relevant;
        predicted_ids_total += predicted_keys.len();
        retrieved_ids_total += retrieved.len();
        used_ids_total += used.len();
        predicted_and_used += predicted_keys.iter().filter(|k| used.contains(k)).count();
        retrieved_and_used += retrieved.iter().filter(|k| used.contains(*k)).count();
        predicted_not_retrieved += predicted_keys
            .iter()
            .filter(|k| !retrieved.contains(k))
            .count();
    }
    let mut report = score.report(minimum_confidence);
    report["ranking"] = score.ranking_summary();
    report["calibration"] = score.calibration_summary();
    report["roles"] = json!({
        "predicted": "leave-one-out memory predictions (label=predicted)",
        "retrieved": "hash-v1 cosine top-5 for the same prompt",
        "used": "fixture gold relevant memory keys (proxy for context actually needed)",
        "counts": {
            "predictedIds": predicted_ids_total,
            "retrievedIds": retrieved_ids_total,
            "usedIds": used_ids_total,
            "predictedAndUsed": predicted_and_used,
            "retrievedAndUsed": retrieved_and_used,
            "predictedNotInRetrievedTop5": predicted_not_retrieved
        }
    });
    Ok(report)
}

fn hash_retrieve_top(
    fixture: &ax_memory::bench::CodingFixture,
    prompt: &str,
    k: usize,
) -> Vec<String> {
    use ax_memory::embed::{cosine, embed_text};
    let q = embed_text(prompt);
    let mut scored: Vec<(f32, String)> = fixture
        .memories
        .iter()
        .filter(|m| m.insert)
        .map(|m| {
            let text = crate::fixture::doc_text(m);
            (cosine(&q, &embed_text(&text)), m.key.clone())
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    scored.into_iter().take(k).map(|(_, key)| key).collect()
}

#[derive(Default)]
struct HeldOut {
    questions: usize,
    expected: usize,
    answered: usize,
    covered: usize,
    predictions: usize,
    true_positives: usize,
    p3: Vec<f64>,
    p5: Vec<f64>,
    r3: Vec<f64>,
    mrr: Vec<f64>,
    /// (confidence, hit 0/1) for calibration
    conf_hits: Vec<(f64, f64)>,
}

impl HeldOut {
    fn add(&mut self, predicted: &[Prediction], relevant: &[String]) {
        // Context-usage evaluation scores memory predictions only (skills/rules/patterns/graph are separate).
        let memory: Vec<&Prediction> = predicted
            .iter()
            .filter(|item| item.kind == "memory")
            .collect();
        let hits = memory
            .iter()
            .filter(|item| relevant.contains(&item.target_id))
            .count();
        self.questions += 1;
        self.expected += usize::from(!relevant.is_empty());
        self.answered += usize::from(!memory.is_empty());
        self.covered += usize::from(hits > 0);
        self.predictions += memory.len();
        self.true_positives += hits;

        let ranked: Vec<String> = memory.iter().map(|item| item.target_id.clone()).collect();
        if !relevant.is_empty() {
            if let Some(v) = crate::benchmark_abc::precision_at(relevant, &ranked, 3) {
                self.p3.push(v);
            }
            if let Some(v) = crate::benchmark_abc::precision_at(relevant, &ranked, 5) {
                self.p5.push(v);
            }
            if let Some(v) = ax_memory::bench::recall_at(relevant, &ranked, 3) {
                self.r3.push(v);
            }
            if let Some(v) = crate::benchmark_abc::mrr(relevant, &ranked) {
                self.mrr.push(v);
            }
        }
        for item in &memory {
            let hit = if relevant.contains(&item.target_id) {
                1.0
            } else {
                0.0
            };
            self.conf_hits.push((item.confidence, hit));
        }
    }

    fn mean(values: &[f64]) -> Option<f64> {
        if values.is_empty() {
            None
        } else {
            Some(values.iter().sum::<f64>() / values.len() as f64)
        }
    }

    fn ranking_summary(&self) -> Value {
        json!({
            "kind": "measured",
            "method": "memory prediction ranking on leave-one-out (P@3/P@5/R@3/MRR)",
            "precisionAt3": Self::mean(&self.p3),
            "precisionAt5": Self::mean(&self.p5),
            "recallAt3": Self::mean(&self.r3),
            "mrr": Self::mean(&self.mrr),
            "samples": {
                "precisionAt3": self.p3.len(),
                "precisionAt5": self.p5.len(),
                "recallAt3": self.r3.len(),
                "mrr": self.mrr.len()
            }
        })
    }

    fn calibration_summary(&self) -> Value {
        // Fixed buckets [0.5,0.6), … [0.9,1.0]
        let edges = [0.5, 0.6, 0.7, 0.8, 0.9, 1.000_000_1];
        let mut buckets = Vec::new();
        for i in 0..edges.len() - 1 {
            let lo = edges[i];
            let hi = edges[i + 1];
            let pts: Vec<f64> = self
                .conf_hits
                .iter()
                .filter(|(c, _)| *c >= lo && *c < hi)
                .map(|(_, h)| *h)
                .collect();
            buckets.push(json!({
                "lo": lo,
                "hi": if hi > 1.0 { 1.0 } else { hi },
                "n": pts.len(),
                "meanConfidence": if pts.is_empty() {
                    Value::Null
                } else {
                    json!(lo + (hi.min(1.0) - lo) / 2.0)
                },
                "empiricalAccuracy": Self::mean(&pts)
            }));
        }
        json!({
            "kind": "measured",
            "method": "confidence buckets vs memory hit rate on leave-one-out",
            "buckets": buckets,
            "n": self.conf_hits.len()
        })
    }

    fn report(&self, minimum_confidence: f64) -> Value {
        let false_positives = self.predictions - self.true_positives;
        json!({
            "kind": "measured",
            "method": "leave-one-out",
            "source": EVALUATION_SOURCE,
            "questions": self.questions,
            "expected": self.expected,
            "answered": self.answered,
            "predictions": self.predictions,
            "truePositives": self.true_positives,
            "falsePositives": false_positives,
            "precision": ratio(self.true_positives, self.predictions),
            "falsePositiveRate": ratio(false_positives, self.predictions),
            "coverage": ratio(self.covered, self.expected),
            "minimumConfidence": minimum_confidence
        })
    }
}

fn ratio(part: usize, whole: usize) -> Value {
    if whole == 0 {
        Value::Null
    } else {
        json!(part as f64 / whole as f64)
    }
}

fn context_kind(id: &str) -> (&'static str, &str) {
    match id.strip_prefix("memory:") {
        Some(memory) => ("memory", memory),
        None => ("file", id),
    }
}

fn tally<'a>(
    counts: &mut HashMap<(&'static str, &'a str), usize>,
    items: impl Iterator<Item = (&'static str, &'a str)>,
) {
    let mut seen = HashSet::new();
    for (kind, id) in items {
        if id.is_empty() || !seen.insert((kind, id)) {
            continue;
        }
        *counts.entry((kind, id)).or_insert(0) += 1;
    }
}

/// Lowercased alphanumeric words of `text`.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// True when every word of the row's category is a whole word of the task.
fn category_matches(row: &Observation, task: &str) -> bool {
    let category = words(&row.task_category);
    if category.is_empty() {
        return false;
    }
    let task: HashSet<String> = words(task).into_iter().collect();
    category.iter().all(|word| task.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_confidence_is_dropped_and_the_label_is_predicted() {
        let rows = vec![
            Observation {
                task_category: "memory".into(),
                skills: vec!["recall".into()],
                rules: vec![],
                context_ids: vec![],
            },
            Observation {
                task_category: "memory".into(),
                skills: vec!["index".into()],
                rules: vec![],
                context_ids: vec![],
            },
        ];
        let predicted = predict(&rows, "memory search", 0.6);
        assert!(predicted.iter().all(|item| item.label == "predicted"));
        assert!(predicted.iter().all(|item| item.target_id != "recall"));
        assert!(predicted.iter().all(|item| item.target_id != "index"));
    }

    #[test]
    fn an_empty_task_predicts_nothing() {
        let row = Observation {
            task_category: "parser".into(),
            skills: vec!["parse-errors".into()],
            rules: vec![],
            context_ids: vec![],
        };
        assert!(predict(&[row], "", 0.0).is_empty());
    }

    #[test]
    fn an_id_repeated_in_one_row_counts_once_and_empty_ids_never() {
        let rows = vec![
            Observation {
                task_category: "parser".into(),
                skills: vec!["parse-errors".into(), "parse-errors".into(), "".into()],
                rules: vec![],
                context_ids: vec![],
            },
            Observation {
                task_category: "parser".into(),
                skills: vec![],
                rules: vec![],
                context_ids: vec![],
            },
        ];
        let predicted = predict(&rows, "parser", 0.0);
        assert!(predicted.iter().any(|p| {
            p.kind == "skill"
                && p.target_id == "parse-errors"
                && (p.confidence - 0.5).abs() < 1e-9
                && p.label == "predicted"
                && !p.explanation.is_empty()
                && !p.evidence.is_empty()
                && p.relationship == "inferred"
        }));
        assert!(predicted.iter().any(|p| p.kind == "pattern"));
    }

    #[test]
    fn memory_context_ids_are_predicted_as_memories() {
        let row = Observation {
            task_category: "release".into(),
            skills: vec![],
            rules: vec![],
            context_ids: vec!["memory:m1".into(), "src/lib.rs".into()],
        };
        let predicted = predict(&[row.clone(), row], "release", 0.5);
        assert!(predicted.len() >= 2);
        assert!(predicted.iter().any(|p| p.kind == "pattern"));
        assert!(predicted.iter().any(|p| {
            p.kind == "memory"
                && p.target_id == "m1"
                && (p.confidence - 1.0).abs() < 1e-9
                && !p.explanation.is_empty()
                && !p.evidence.is_empty()
                && p.relationship == "inferred"
        }));
        assert!(predicted.iter().any(|p| {
            p.kind == "file" && p.target_id == "src/lib.rs" && (p.confidence - 1.0).abs() < 1e-9
        }));
    }

    #[test]
    fn held_out_evaluation_scores_the_coding_fixture() {
        let report = evaluate_fixture(crate::CODING_FIXTURE, 0.75).unwrap();
        assert_eq!(report["kind"], "measured");
        assert_eq!(report["method"], "leave-one-out");
        assert_eq!(report["questions"], 19);
        assert_eq!(report["expected"], 15);
        assert_eq!(report["answered"], 5);
        assert_eq!(report["predictions"], 5);
        assert_eq!(report["truePositives"], 5);
        assert_eq!(report["falsePositives"], 0);
        assert_eq!(report["precision"], 1.0);
        assert_eq!(report["falsePositiveRate"], 0.0);
        assert_eq!(report["minimumConfidence"], 0.75);
        let coverage = report["coverage"].as_f64().unwrap();
        assert!((coverage - 5.0 / 15.0).abs() < 1e-9, "{coverage}");
        let ranking = &report["ranking"];
        assert_eq!(ranking["kind"], "measured");
        assert!(ranking.get("precisionAt3").is_some());
        assert!(ranking.get("precisionAt5").is_some());
        assert!(ranking.get("recallAt3").is_some());
        assert!(ranking.get("mrr").is_some());
        let calibration = &report["calibration"];
        assert_eq!(calibration["kind"], "measured");
        assert!(calibration["buckets"].is_array());
    }

    #[test]
    fn held_out_evaluation_counts_false_positives() {
        let fixture = r#"{
            "memories": [],
            "questions": [
                { "id": "q1", "family": "release", "prompt": "p", "channel": "recall", "relevant": ["m1"] },
                { "id": "q2", "family": "release", "prompt": "p", "channel": "recall", "relevant": ["m2"] }
            ]
        }"#;
        let report = evaluate_fixture(fixture, 0.5).unwrap();
        assert_eq!(report["predictions"], 2);
        assert_eq!(report["truePositives"], 0);
        assert_eq!(report["falsePositives"], 2);
        assert_eq!(report["precision"], 0.0);
        assert_eq!(report["falsePositiveRate"], 1.0);
        assert_eq!(report["coverage"], 0.0);
        assert_eq!(report["answered"], 2);
    }

    #[test]
    fn held_out_evaluation_keeps_the_minimum_inclusive_and_rejects_bad_minimums() {
        let report = evaluate_fixture(crate::CODING_FIXTURE, 1.0).unwrap();
        assert_eq!(report["predictions"], 5);
        let above = evaluate_fixture(crate::CODING_FIXTURE, 1.0 + f64::EPSILON * 4.0);
        assert!(above.is_err(), "minimum confidence above 1 is rejected");
        assert!(evaluate_fixture(crate::CODING_FIXTURE, -0.1).is_err());
    }

    #[test]
    fn predictions_include_explanation_evidence_and_relationship() {
        let row = Observation {
            task_category: "parser".into(),
            skills: vec!["parse-errors".into()],
            rules: vec!["rust-errors".into()],
            context_ids: vec!["memory:m1".into()],
        };
        let predicted = predict(&[row.clone(), row], "parser", 0.5);
        assert!(!predicted.is_empty());
        for item in &predicted {
            assert!(!item.explanation.is_empty(), "{item:?}");
            assert!(!item.evidence.is_empty(), "{item:?}");
            assert!(
                item.relationship == "inferred" || item.relationship == "verified",
                "{}",
                item.relationship
            );
            assert_eq!(item.label, "predicted");
        }
    }

    #[test]
    fn graph_edges_mark_relationship_verified() {
        let row = Observation {
            task_category: "graph".into(),
            skills: vec![],
            rules: vec![],
            context_ids: vec!["src/a.rs".into(), "src/b.rs".into(), "src/c.rs".into()],
        };
        let edges = vec![
            ("src/a.rs".into(), "src/b.rs".into()),
            ("src/c.rs".into(), "src/elsewhere.rs".into()),
        ];
        let predicted = predict_with_edges(&[row], "graph", 0.0, &edges);
        let relationship = |id: &str| {
            predicted
                .iter()
                .find(|p| p.kind == "file" && p.target_id == id)
                .map(|p| p.relationship.clone())
                .expect("file prediction")
        };
        assert_eq!(relationship("src/a.rs"), "verified");
        assert_eq!(relationship("src/b.rs"), "verified");
        // An edge to an id outside the predicted context does not verify.
        assert_eq!(relationship("src/c.rs"), "inferred");
        let a = predicted
            .iter()
            .find(|p| p.target_id == "src/a.rs")
            .unwrap();
        assert!(a.evidence.iter().any(|e| e.contains("src/b.rs")), "{a:?}");
    }

    #[test]
    fn project_edges_are_never_predictions_by_themselves() {
        let row = Observation {
            task_category: "parser".into(),
            skills: vec!["parse-errors".into()],
            rules: vec![],
            context_ids: vec![],
        };
        let edges: Vec<(String, String)> = (0..5_000)
            .map(|i| (format!("n{i}"), format!("m{i}")))
            .collect();
        let predicted = predict_with_edges(&[row], "fix the parser", 0.0, &edges);
        assert!(predicted.iter().all(|p| p.kind != "graph"), "{predicted:?}");
        assert_eq!(predicted.len(), 2, "skill + pattern only: {predicted:?}");
    }

    #[test]
    fn predictions_are_capped() {
        let row = Observation {
            task_category: "parser".into(),
            skills: (0..1_000).map(|i| format!("skill-{i}")).collect(),
            rules: vec![],
            context_ids: vec![],
        };
        let predicted = predict(&[row], "parser", 0.0);
        assert_eq!(predicted.len(), MAX_PREDICTIONS);
    }

    #[test]
    fn category_match_needs_whole_tokens() {
        let row = |category: &str| Observation {
            task_category: category.into(),
            skills: vec!["s".into()],
            rules: vec![],
            context_ids: vec![],
        };
        // "ui" hides inside "build"; that is not a match.
        assert!(!category_matches(&row("ui"), "build the release pipeline"));
        // A one-letter task is not a substring match for every category.
        assert!(!category_matches(&row("parser"), "a"));
        assert!(!category_matches(&row("parser"), "  "));
        assert!(category_matches(&row("ui"), "fix the UI layout"));
        assert!(category_matches(&row("parser"), "fix the parser"));
        assert!(category_matches(&row("named_trigger"), "named_trigger"));
        assert!(category_matches(
            &row("release pipeline"),
            "build the release pipeline"
        ));
        assert!(!category_matches(
            &row("release notes"),
            "build the release pipeline"
        ));
    }

    #[test]
    fn dump_predict_validate_json_when_env_set() {
        use std::path::PathBuf;
        let Some(path) = std::env::var_os("AX_DUMP_PREDICT_VALIDATE") else {
            return;
        };
        let report = evaluate_fixture(crate::CODING_FIXTURE, 0.75).unwrap();
        assert!(report.get("ranking").is_some());
        assert!(report.get("calibration").is_some());
        assert!(report.get("roles").is_some());
        std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        eprintln!("wrote {}", PathBuf::from(path).display());
    }
}
