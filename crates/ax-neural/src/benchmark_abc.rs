//! Phase 2 A/B/C retrieval benchmark (SPEC-PHASE2-BENCHMARK.md).
//! Benchmark-only adapters — production Neural ranking stays required.

use ax_memory::bench::{load_coding_fixture, score_coding, CodingFixture};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Instant;

const LOCKED: &[&str] = &[
    "named_trigger",
    "paraphrase",
    "abstention",
    "delivery_budget",
];

pub use crate::metrics::{mrr, precision_at};

use crate::metrics::{Acc, SummaryStyle, BENCHMARK_ABC};

fn lexical_rank_keys(fixture: &CodingFixture, prompt: &str) -> Vec<String> {
    let docs: Vec<crate::lexical::LexicalDoc> = fixture
        .memories
        .iter()
        .filter(|m| m.insert)
        .map(|m| {
            crate::lexical::LexicalDoc::new(m.key.clone(), &m.title, &m.body, &m.triggers)
        })
        .collect();
    let (ranked, abstained) = crate::lexical::rank(&docs, prompt);
    if abstained {
        Vec::new()
    } else {
        ranked.into_iter().map(|h| h.key).collect()
    }
}

fn is_locked(family: &str) -> bool {
    LOCKED.contains(&family)
}

/// Runs A (BM25), B (Ax locked fusion / hash-v1), C (Neural lexical when admitted else B).
/// Does not change production ranking configuration.
pub async fn run_abc(fixture_text: &str) -> Result<Value, String> {
    let fixture = load_coding_fixture(fixture_text)?;
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let database = ax_db::Database::open(&dir.path().join("abc.db"))
        .await
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    let scored = score_coding(database.pool(), &fixture)
        .await
        .map_err(|e| e.to_string())?;
    let score_millis = started.elapsed().as_secs_f64() * 1000.0;
    database.pool().close().await;

    let mut arm_a_locked = Acc::default();
    let mut arm_b_locked = Acc::default();
    let mut arm_c_locked = Acc::default();
    let mut arm_a_held = Acc::default();
    let mut arm_b_held = Acc::default();
    let mut arm_c_held = Acc::default();

    let lexical_active = crate::bakeoff::challenger_admitted();
    let lex_started = Instant::now();
    let mut lexical_micros = Vec::new();

    for question in &fixture.questions {
        let score = scored
            .questions
            .iter()
            .find(|s| s.id == question.id)
            .ok_or_else(|| format!("missing score for {}", question.id))?;
        let relevant = &question.relevant;
        let a = &score.bm25;
        let b = &score.ax;
        let c_started = Instant::now();
        let c = if lexical_active {
            lexical_rank_keys(&fixture, &question.prompt)
        } else {
            b.clone()
        };
        lexical_micros.push(c_started.elapsed().as_secs_f64() * 1_000_000.0);

        let (la, lb, lc) = if is_locked(&question.family) {
            (&mut arm_a_locked, &mut arm_b_locked, &mut arm_c_locked)
        } else {
            (&mut arm_a_held, &mut arm_b_held, &mut arm_c_held)
        };
        la.observe(BENCHMARK_ABC, None, relevant, a);
        lb.observe(BENCHMARK_ABC, None, relevant, b);
        lc.observe(BENCHMARK_ABC, None, relevant, &c);
    }
    let lexical_wall_millis = lex_started.elapsed().as_secs_f64() * 1000.0;
    let memory = crate::rss::memory_report();

    Ok(json!({
        "kind": "measured",
        "fixture": "benchmarks/memory/coding.json (built in)",
        "productionRankingUnchanged": true,
        "semanticWeight": crate::SEMANTIC_WEIGHT,
        "arms": {
            "A_ax_without_neural_ranking": {
                "method": "bm25 rankings from ax_memory::bench::score_coding (benchmark adapter)",
                "locked": arm_a_locked.summary(SummaryStyle::BenchmarkAbc),
                "heldOut": arm_a_held.summary(SummaryStyle::BenchmarkAbc)
            },
            "B_ax_original_ranking": {
                "method": "Ax locked fusion + hash-v1 (score_coding ax rankings)",
                "locked": arm_b_locked.summary(SummaryStyle::BenchmarkAbc),
                "heldOut": arm_b_held.summary(SummaryStyle::BenchmarkAbc)
            },
            "C_ax_neural": {
                "method": if lexical_active {
                    "lexical-semantic-cpu ranking (bake-off admitted); not neural embedding inference"
                } else {
                    "falls back to arm B until lexical challenger is admitted"
                },
                "lexicalAdmitted": lexical_active,
                "claimsNeuralInference": false,
                "locked": arm_c_locked.summary(SummaryStyle::BenchmarkAbc),
                "heldOut": arm_c_held.summary(SummaryStyle::BenchmarkAbc)
            }
        },
        "latency": {
            "kind": "measured",
            "scoreCodingMillis": score_millis,
            "lexicalRankWallMillis": lexical_wall_millis,
            "lexicalRankMicrosMedian": median(&mut lexical_micros)
        },
        "memory": memory,
        "tokens": {
            "kind": "not_measured",
            "reason": "no LLM arm in this retrieval benchmark"
        },
        "lockedFamilies": LOCKED,
        "heldOutFamilyCount": fixture
            .questions
            .iter()
            .filter(|q| !is_locked(&q.family))
            .count(),
        "database": "temporary",
        "openedLiveDatabase": false
    }))
}

fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

/// Compact comparison for scripts: locked-family recall@3 per arm.
pub fn locked_recall3(report: &Value) -> BTreeMap<&'static str, Option<f64>> {
    let mut out = BTreeMap::new();
    for (key, label) in [
        ("A", "A_ax_without_neural_ranking"),
        ("B", "B_ax_original_ranking"),
        ("C", "C_ax_neural"),
    ] {
        out.insert(
            key,
            report["arms"][label]["locked"]["recallAt3"]
                .as_f64()
                .or(None),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn re_exports_metrics_helpers() {
        let relevant = vec!["m1".into()];
        assert_eq!(precision_at(&relevant, &["m1".into()], 1), Some(1.0));
        assert_eq!(mrr(&[], &["m1".into()]), None);
    }

    #[tokio::test]
    async fn abc_report_samples_rss_once_per_run() {
        crate::rss::reset_process_rss_sample_calls();
        let report = run_abc(crate::CODING_FIXTURE).await.unwrap();
        assert_eq!(report["kind"], "measured");
        assert_eq!(crate::rss::take_process_rss_sample_calls(), 1);
    }

    #[tokio::test]
    async fn abc_report_measures_three_arms_without_raising_semantic_weight() {
        let report = run_abc(crate::CODING_FIXTURE).await.unwrap();
        assert_eq!(report["kind"], "measured");
        assert_eq!(report["productionRankingUnchanged"], true);
        assert_eq!(report["semanticWeight"], 0.0);
        assert_eq!(report["arms"]["C_ax_neural"]["claimsNeuralInference"], false);
        assert!(report["arms"]["A_ax_without_neural_ranking"]["locked"]["questionsScored"]
            .as_u64()
            .unwrap()
            >= 1);
        assert!(report["arms"]["B_ax_original_ranking"]["locked"]["questionsScored"]
            .as_u64()
            .unwrap()
            >= 1);
        assert!(report["arms"]["C_ax_neural"]["locked"]["questionsScored"]
            .as_u64()
            .unwrap()
            >= 1);
        assert!(report["heldOutFamilyCount"].as_u64().unwrap() >= 1);
        match report["memory"]["kind"].as_str() {
            Some("measured") => {
                assert!(report["memory"]["rssBytes"].as_u64().unwrap() > 0);
            }
            Some("not_measured") if cfg!(windows) => {
                // Windows RSS sampling is not wired yet; Wine/native still run the suite.
            }
            other => panic!("unexpected memory kind on this host: {other:?}"),
        }
        assert_eq!(report["tokens"]["kind"], "not_measured");
        if crate::bakeoff::challenger_admitted() {
            // Lexical should beat BM25 on locked paraphrase-influenced mean recall@3
            // when admitted (paraphrase is one locked family).
            let a = report["arms"]["A_ax_without_neural_ranking"]["locked"]["recallAt3"].as_f64();
            let c = report["arms"]["C_ax_neural"]["locked"]["recallAt3"].as_f64();
            assert!(c.is_some());
            if let (Some(a), Some(c)) = (a, c) {
                assert!(
                    c + f64::EPSILON >= a,
                    "C recall@3 {c} should be >= A {a} when lexical is admitted"
                );
            }
        }
    }

    #[tokio::test]
    async fn abc_adapter_does_not_require_disabling_production_ranking() {
        let report = run_abc(crate::CODING_FIXTURE).await.unwrap();
        assert_eq!(report["productionRankingUnchanged"], true);
        let cfg = crate::config::NeuralConfig::required();
        assert!(cfg.ranking_active());
    }
}
