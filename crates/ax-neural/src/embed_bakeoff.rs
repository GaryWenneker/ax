//! Phase 3 embedding-runtime bake-off (SPEC-PHASE3-EMBED.md).
//! Compares hash / lexical / blake3; reports ONNX and Candle as not_built until linked + configured.

use ax_memory::bench::{load_coding_fixture, CodingFixture};
use ax_memory::embed::{cosine, embed_text};
use serde_json::{json, Value};
use std::time::Instant;

use crate::metrics::{Acc, SummaryStyle, EMBED_BAKEOFF};
use crate::runtime::Backend;

const LOCKED: &[&str] = &[
    "named_trigger",
    "paraphrase",
    "abstention",
    "delivery_budget",
];

fn doc_text(m: &ax_memory::bench::CodingMemory) -> String {
    format!("{} {} {} {}", m.key, m.title, m.body, m.triggers.join(" "))
}

fn cosine_rank(
    fixture: &CodingFixture,
    prompt: &str,
    embed: &dyn Fn(&str) -> Vec<f32>,
) -> Vec<String> {
    let q = embed(prompt);
    let mut scored: Vec<(f32, String)> = fixture
        .memories
        .iter()
        .filter(|m| m.insert)
        .map(|m| {
            let v = embed(&doc_text(m));
            (cosine(&q, &v), m.key.clone())
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    scored.into_iter().map(|(_, k)| k).collect()
}

/// Same ranking as `cosine_rank` but reuses precomputed document vectors (validation-only).
#[allow(dead_code)]
fn cosine_rank_with_doc_cache(
    prompt: &str,
    embed: &dyn Fn(&str) -> Vec<f32>,
    doc_vectors: &[(String, Vec<f32>)],
) -> Vec<String> {
    let q = embed(prompt);
    let mut scored: Vec<(f32, String)> = doc_vectors
        .iter()
        .map(|(key, v)| (cosine(&q, v), key.clone()))
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    scored.into_iter().map(|(_, k)| k).collect()
}

/// Keep every positive baseline score in its original order. Only a zero-score
/// slot may be filled from `extra`. A full positive prefix is not reordered.
#[allow(dead_code)]
fn baseline_keep_positive_then_fill(
    hash_scored: &[(String, f32)],
    extra: &[String],
    limit: usize,
) -> Vec<String> {
    let mut ranked: Vec<String> = hash_scored
        .iter()
        .filter(|(_, score)| *score > 0.0)
        .map(|(key, _)| key.clone())
        .take(limit)
        .collect();
    if ranked.len() >= limit {
        return ranked;
    }
    let mut seen: std::collections::BTreeSet<String> = ranked.iter().cloned().collect();
    for key in extra {
        if ranked.len() >= limit {
            break;
        }
        if seen.insert(key.clone()) {
            ranked.push(key.clone());
        }
    }
    for (key, score) in hash_scored {
        if ranked.len() >= limit {
            break;
        }
        if *score > 0.0 {
            continue;
        }
        if seen.insert(key.clone()) {
            ranked.push(key.clone());
        }
    }
    ranked
}

fn lexical_rank(fixture: &CodingFixture, prompt: &str) -> Vec<String> {
    let docs: Vec<crate::lexical::LexicalDoc> = fixture
        .memories
        .iter()
        .filter(|m| m.insert)
        .map(|m| crate::lexical::LexicalDoc::new(m.key.clone(), &m.title, &m.body, &m.triggers))
        .collect();
    let (ranked, abstained) = crate::lexical::rank(&docs, prompt);
    if abstained {
        Vec::new()
    } else {
        ranked.into_iter().map(|h| h.key).collect()
    }
}

fn candidate_not_built(id: &str, reason: &str) -> Value {
    json!({
        "id": id,
        "kind": "not_built",
        "claimsNeuralInference": false,
        "admitted": false,
        "reason": reason
    })
}

fn candidate_measured(
    id: &str,
    method: &str,
    claims_neural: bool,
    admissible_as_semantic: bool,
    locked: Acc,
    latency_micros: f64,
) -> Value {
    let paraphrase = locked.paraphrase_recall_at_3();
    json!({
        "id": id,
        "kind": "measured",
        "method": method,
        "claimsNeuralInference": claims_neural,
        "admissibleAsSemantic": admissible_as_semantic,
        "admitted": false,
        "locked": locked.summary(SummaryStyle::EmbedBakeoff),
        "latency": {
            "kind": "measured",
            "lockedRankMicrosMedian": latency_micros
        },
        "paraphraseRecallAt3": paraphrase
    })
}

/// Runs the Phase 3 embed bake-off. Does not link ONNX/Candle; reports them `not_built`.
pub fn run_embed_bakeoff(fixture_text: &str) -> Result<Value, String> {
    let fixture = load_coding_fixture(fixture_text)?;

    let hash_started = Instant::now();
    let mut hash_times = Vec::new();
    let hash_acc = {
        let mut acc = Acc::default();
        for q in fixture
            .questions
            .iter()
            .filter(|q| LOCKED.contains(&q.family.as_str()))
        {
            let t0 = Instant::now();
            let ranked = cosine_rank(&fixture, &q.prompt, &embed_text);
            hash_times.push(t0.elapsed().as_secs_f64() * 1_000_000.0);
            acc.observe(EMBED_BAKEOFF, Some(&q.family), &q.relevant, &ranked);
        }
        let _ = hash_started;
        acc
    };
    hash_times.sort_by(f64::total_cmp);
    let hash_median = if hash_times.is_empty() {
        0.0
    } else {
        hash_times[hash_times.len() / 2]
    };

    let mut lex_times = Vec::new();
    let lex_acc = {
        let mut acc = Acc::default();
        for q in fixture
            .questions
            .iter()
            .filter(|q| LOCKED.contains(&q.family.as_str()))
        {
            let t0 = Instant::now();
            let ranked = lexical_rank(&fixture, &q.prompt);
            lex_times.push(t0.elapsed().as_secs_f64() * 1_000_000.0);
            acc.observe(EMBED_BAKEOFF, Some(&q.family), &q.relevant, &ranked);
        }
        acc
    };
    lex_times.sort_by(f64::total_cmp);
    let lex_median = if lex_times.is_empty() {
        0.0
    } else {
        lex_times[lex_times.len() / 2]
    };

    let mut candidates = vec![
        candidate_measured(
            "hash-v1",
            "ax_memory::embed::embed_text cosine retrieval",
            false,
            false,
            hash_acc,
            hash_median,
        ),
        candidate_measured(
            "lexical-semantic-cpu",
            "lexical synonym + trigger rank (not neural inference)",
            false,
            false,
            lex_acc,
            lex_median,
        ),
    ];

    if let Some(backend) = crate::runtime::active() {
        let mut times = Vec::new();
        let mut acc = Acc::default();
        for q in fixture
            .questions
            .iter()
            .filter(|q| LOCKED.contains(&q.family.as_str()))
        {
            let t0 = Instant::now();
            let ranked = cosine_rank(&fixture, &q.prompt, &|t| {
                backend.embed(t).unwrap_or_default()
            });
            times.push(t0.elapsed().as_secs_f64() * 1_000_000.0);
            acc.observe(EMBED_BAKEOFF, Some(&q.family), &q.relevant, &ranked);
        }
        times.sort_by(f64::total_cmp);
        let median = if times.is_empty() {
            0.0
        } else {
            times[times.len() / 2]
        };
        candidates.push(candidate_measured(
            "cpu-blake3",
            "blake3 bag vector via neural-runtime (deterministic; not semantic)",
            false,
            false,
            acc,
            median,
        ));
    } else {
        candidates.push(candidate_not_built(
            "cpu-blake3",
            "neural-runtime feature not enabled in this binary",
        ));
    }

    #[cfg_attr(not(feature = "neural-ort"), allow(unused_mut))]
    let mut onnx_paraphrase = None::<f64>;
    let onnx_locked_ok = false;
    #[cfg(feature = "neural-ort")]
    {
        match crate::onnx_embed::try_embed_text("warmup") {
            Ok(_) => {
                let doc_pairs: Vec<(String, String)> = fixture
                    .memories
                    .iter()
                    .filter(|m| m.insert)
                    .map(|m| (m.key.clone(), doc_text(m)))
                    .collect();
                let doc_cache = crate::onnx_embed::BakeoffDocCache::build(doc_pairs)?;
                let embed_query =
                    |text: &str| crate::onnx_embed::try_embed_text(text).unwrap_or_default();
                let rank_onnx = |prompt: &str| {
                    cosine_rank_with_doc_cache(prompt, &embed_query, doc_cache.entries())
                };
                let mut times = Vec::new();
                let mut acc = Acc::default();
                for q in fixture
                    .questions
                    .iter()
                    .filter(|q| LOCKED.contains(&q.family.as_str()))
                {
                    let t0 = Instant::now();
                    let ranked = rank_onnx(&q.prompt);
                    times.push(t0.elapsed().as_secs_f64() * 1_000_000.0);
                    acc.observe(EMBED_BAKEOFF, Some(&q.family), &q.relevant, &ranked);
                }
                times.sort_by(f64::total_cmp);
                let median = if times.is_empty() {
                    0.0
                } else {
                    times[times.len() / 2]
                };
                onnx_paraphrase = acc.paraphrase_recall_at_3();
                let mut held = Acc::default();
                for q in fixture
                    .questions
                    .iter()
                    .filter(|q| !LOCKED.contains(&q.family.as_str()))
                {
                    let ranked = rank_onnx(&q.prompt);
                    held.observe(EMBED_BAKEOFF, Some(&q.family), &q.relevant, &ranked);
                }
                let mut hash_held = Acc::default();
                for q in fixture
                    .questions
                    .iter()
                    .filter(|q| !LOCKED.contains(&q.family.as_str()))
                {
                    let ranked = cosine_rank(&fixture, &q.prompt, &embed_text);
                    hash_held.observe(EMBED_BAKEOFF, Some(&q.family), &q.relevant, &ranked);
                }
                let mut cand = candidate_measured(
                    "onnx-runtime-cpu",
                    "ONNX Runtime CPU + local all-MiniLM-L6-v2.onnx (neural embedding inference)",
                    true,
                    true,
                    acc,
                    median,
                );
                cand["modelPath"] = json!(crate::onnx_embed::model_paths().0.display().to_string());
                if let Ok(sum) =
                    crate::onnx_embed::model_checksum(&crate::onnx_embed::model_paths().0)
                {
                    cand["checksum"] = json!(sum);
                }
                cand["heldOut"] = held.summary(SummaryStyle::EmbedBakeoff);
                cand["heldOutHashBaseline"] = hash_held.summary(SummaryStyle::EmbedBakeoff);
                cand["validationDocEmbedCache"] = json!({
                    "documentsEmbeddedOnce": doc_cache.entries().len(),
                    "persistedToMemoryColumn": false
                });
                candidates.push(cand);
            }
            Err(e) => {
                candidates.push(json!({
                    "id": "onnx-runtime-cpu",
                    "kind": "not_configured",
                    "claimsNeuralInference": true,
                    "admitted": false,
                    "reason": e
                }));
            }
        }
    }
    #[cfg(not(feature = "neural-ort"))]
    {
        candidates.push(candidate_not_built(
            "onnx-runtime-cpu",
            "ONNX Runtime not linked; build ax-neural with --features neural-ort and place model under ~/.ax/models",
        ));
    }
    candidates.push(candidate_not_built(
        "candle-cpu",
        "Candle not linked; no local model configured under ~/.ax/models",
    ));

    let hash_para = candidates
        .iter()
        .find(|c| c["id"] == "hash-v1")
        .and_then(|c| c["paraphraseRecallAt3"].as_f64())
        .unwrap_or(0.0);
    let lex_para = candidates
        .iter()
        .find(|c| c["id"] == "lexical-semantic-cpu")
        .and_then(|c| c["paraphraseRecallAt3"].as_f64())
        .unwrap_or(0.0);

    // Admission for neural: paraphrase strictly above hash; do not auto-admit over lexical without held-out.
    let mut neural_admitted = false;
    if let Some(p) = onnx_paraphrase {
        let onnx = candidates.iter().find(|c| c["id"] == "onnx-runtime-cpu");
        let held_ok = onnx
            .and_then(|c| {
                let o = c["heldOut"]["recallAt3"].as_f64()?;
                let h = c["heldOutHashBaseline"]["recallAt3"].as_f64()?;
                Some(o + f64::EPSILON >= h)
            })
            .unwrap_or(false);
        if p > hash_para && held_ok {
            neural_admitted = true;
            if let Some(c) = candidates
                .iter_mut()
                .find(|c| c["id"] == "onnx-runtime-cpu")
            {
                c["admitted"] = json!(true);
                c["admissionNote"] = json!("paraphrase > hash and held-out R@3 >= hash; SEMANTIC_WEIGHT stays 0; memory column stays hash-v1");
            }
        } else if p > hash_para {
            if let Some(c) = candidates
                .iter_mut()
                .find(|c| c["id"] == "onnx-runtime-cpu")
            {
                c["admissionNote"] = json!(
                    "paraphrase beats hash but held-out gate failed or missing; not admitted"
                );
            }
        }
    }
    let _ = onnx_locked_ok;

    let winner = if neural_admitted && onnx_paraphrase.unwrap_or(0.0) + f64::EPSILON >= lex_para {
        "onnx-runtime-cpu"
    } else {
        "lexical-semantic-cpu"
    };

    Ok(json!({
        "kind": "measured",
        "fixture": "benchmarks/memory/coding.json (built in)",
        "semanticWeight": crate::SEMANTIC_WEIGHT,
        "productionDefaultsUnchanged": true,
        "winner": winner,
        "neuralEmbeddingAdmitted": neural_admitted,
        "neuralEmbeddingInferenceActive": false,
        "candidates": candidates,
        "memory": {
            "kind": "not_measured",
            "reason": "no portable RSS reader in this harness"
        },
        "selection": {
            "decision": if neural_admitted {
                "onnx MiniLM measured and paraphrase-beats-hash; keep SEMANTIC_WEIGHT=0 and memory column hash-v1 until held-out promotion gate"
            } else {
                "keep lexical-semantic-cpu as retrieval challenger; hash-v1 remains memory column"
            },
            "basis": "measured candidates on locked families; candle still not_built"
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_score_fill_inserts_before_other_zeros_without_moving_positive_scores() {
        let hash = vec![
            ("keep".into(), 0.4),
            ("zero-relevant".into(), 0.0),
            ("zero-other".into(), 0.0),
        ];
        let extra = vec!["zero-relevant".into(), "zero-other".into(), "keep".into()];
        assert_eq!(
            baseline_keep_positive_then_fill(&hash, &extra, 3),
            vec!["keep", "zero-relevant", "zero-other"]
        );
        assert_eq!(
            baseline_keep_positive_then_fill(&hash, &extra, 1),
            vec!["keep".to_string()]
        );
    }

    #[test]
    fn hash_positive_prefix_plus_lexical_fill_on_non_locked_questions() {
        let fixture = load_coding_fixture(crate::CODING_FIXTURE).unwrap();
        let locked = [
            "named_trigger",
            "paraphrase",
            "abstention",
            "delivery_budget",
        ];
        let mut questions = 0u32;
        let mut changed = 0u32;
        let mut positive_short = 0u32;
        let mut baseline_mrr = 0.0;
        let mut filled_mrr = 0.0;
        let mut baseline_recall = 0.0;
        let mut filled_recall = 0.0;
        for question in &fixture.questions {
            if locked.contains(&question.family.as_str()) || question.relevant.is_empty() {
                continue;
            }
            let query = embed_text(&question.prompt);
            let mut scored: Vec<(String, f32)> = fixture
                .memories
                .iter()
                .filter(|memory| memory.insert)
                .map(|memory| {
                    (
                        memory.key.clone(),
                        cosine(&query, &embed_text(&doc_text(memory))),
                    )
                })
                .collect();
            scored.sort_by(|left, right| {
                right
                    .1
                    .partial_cmp(&left.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.0.cmp(&right.0))
            });
            let positive = scored.iter().filter(|(_, score)| *score > 0.0).count();
            if positive < 10 {
                positive_short += 1;
            }
            let baseline: Vec<String> =
                scored.iter().map(|(key, _)| key.clone()).take(10).collect();
            let filled = baseline_keep_positive_then_fill(
                &scored,
                &lexical_rank(&fixture, &question.prompt),
                10,
            );
            if filled != baseline {
                changed += 1;
            }
            questions += 1;
            baseline_mrr += first_reciprocal(&question.relevant, &baseline);
            filled_mrr += first_reciprocal(&question.relevant, &filled);
            baseline_recall += recall_at_3(&question.relevant, &baseline);
            filled_recall += recall_at_3(&question.relevant, &filled);
        }
        let n = f64::from(questions);
        assert_eq!(questions, 12);
        assert_eq!(changed, 3);
        assert_eq!(positive_short, 6);
        assert!((baseline_mrr / n - 0.7098214285714285).abs() < 1e-12);
        assert_eq!(filled_mrr, baseline_mrr);
        assert!((baseline_recall / n - 2.0 / 3.0).abs() < 1e-12);
        assert_eq!(filled_recall, baseline_recall);
    }

    #[test]
    #[cfg(feature = "neural-ort")]
    fn hash_positive_prefix_plus_onnx_fill_on_non_locked_questions() {
        crate::onnx_embed::try_embed_text("warmup").expect("onnx model");
        let fixture = load_coding_fixture(crate::CODING_FIXTURE).unwrap();
        let doc_pairs: Vec<(String, String)> = fixture
            .memories
            .iter()
            .filter(|memory| memory.insert)
            .map(|memory| (memory.key.clone(), doc_text(memory)))
            .collect();
        let doc_cache = crate::onnx_embed::BakeoffDocCache::build(doc_pairs).expect("cache");
        let embed = |text: &str| crate::onnx_embed::try_embed_text(text).unwrap_or_default();
        let locked = [
            "named_trigger",
            "paraphrase",
            "abstention",
            "delivery_budget",
        ];
        let mut questions = 0u32;
        let mut changed = 0u32;
        let mut baseline_mrr = 0.0;
        let mut filled_mrr = 0.0;
        let mut baseline_recall = 0.0;
        let mut filled_recall = 0.0;
        for question in &fixture.questions {
            if locked.contains(&question.family.as_str()) || question.relevant.is_empty() {
                continue;
            }
            let query = embed_text(&question.prompt);
            let mut scored: Vec<(String, f32)> = fixture
                .memories
                .iter()
                .filter(|memory| memory.insert)
                .map(|memory| {
                    (
                        memory.key.clone(),
                        cosine(&query, &embed_text(&doc_text(memory))),
                    )
                })
                .collect();
            scored.sort_by(|left, right| {
                right
                    .1
                    .partial_cmp(&left.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.0.cmp(&right.0))
            });
            let baseline: Vec<String> =
                scored.iter().map(|(key, _)| key.clone()).take(10).collect();
            let extra = cosine_rank_with_doc_cache(&question.prompt, &embed, doc_cache.entries());
            let filled = baseline_keep_positive_then_fill(&scored, &extra, 10);
            if filled != baseline {
                changed += 1;
            }
            questions += 1;
            baseline_mrr += first_reciprocal(&question.relevant, &baseline);
            filled_mrr += first_reciprocal(&question.relevant, &filled);
            baseline_recall += recall_at_3(&question.relevant, &baseline);
            filled_recall += recall_at_3(&question.relevant, &filled);
        }
        let n = f64::from(questions);
        assert_eq!(questions, 12);
        assert_eq!(changed, 6);
        assert!((baseline_mrr / n - 0.7098214285714285).abs() < 1e-12);
        assert!((filled_mrr / n - 0.7083333333333334).abs() < 1e-12);
        assert!(filled_mrr < baseline_mrr);
        assert!((baseline_recall / n - 2.0 / 3.0).abs() < 1e-12);
        assert_eq!(filled_recall, baseline_recall);
    }

    fn first_reciprocal(relevant: &[String], ranked: &[String]) -> f64 {
        ranked
            .iter()
            .position(|key| relevant.iter().any(|want| want == key))
            .map(|index| 1.0 / (index as f64 + 1.0))
            .unwrap_or(0.0)
    }

    fn recall_at_3(relevant: &[String], ranked: &[String]) -> f64 {
        let hit = relevant
            .iter()
            .filter(|want| ranked.iter().take(3).any(|got| got == *want))
            .count();
        hit as f64 / relevant.len() as f64
    }

    #[test]
    fn embed_bakeoff_measures_hash_and_lexical_without_claiming_neural() {
        let report = run_embed_bakeoff(crate::CODING_FIXTURE).unwrap();
        assert_eq!(report["kind"], "measured");
        assert_eq!(report["semanticWeight"], 0.0);
        assert_eq!(report["productionDefaultsUnchanged"], true);
        assert_eq!(report["neuralEmbeddingInferenceActive"], false);
        assert_eq!(report["semanticWeight"], 0.0);
        #[cfg(not(feature = "neural-ort"))]
        assert_eq!(report["neuralEmbeddingAdmitted"], false);
        let candidates = report["candidates"].as_array().unwrap();
        let ids: Vec<&str> = candidates.iter().filter_map(|c| c["id"].as_str()).collect();
        assert!(ids.contains(&"hash-v1"));
        assert!(ids.contains(&"lexical-semantic-cpu"));
        assert!(ids.contains(&"onnx-runtime-cpu"));
        assert!(ids.contains(&"candle-cpu"));
        for c in candidates {
            if (c["id"] == "hash-v1"
                || c["id"] == "lexical-semantic-cpu"
                || c["id"] == "cpu-blake3")
                && c["kind"] == "measured"
            {
                assert_eq!(c["claimsNeuralInference"], false);
            }
            if c["id"] == "candle-cpu" {
                assert_eq!(c["kind"], "not_built");
                assert_eq!(c["admitted"], false);
            }
            if c["id"] == "onnx-runtime-cpu" {
                #[cfg(not(feature = "neural-ort"))]
                assert_eq!(c["kind"], "not_built");
                #[cfg(feature = "neural-ort")]
                assert!(
                    c["kind"] == "measured"
                        || c["kind"] == "not_configured"
                        || c["kind"] == "not_built",
                    "{:?}",
                    c["kind"]
                );
            }
        }
        // Lexical must beat hash on paraphrase when measured (ADR 002 / prior bake-off).
        let hash = candidates.iter().find(|c| c["id"] == "hash-v1").unwrap();
        let lex = candidates
            .iter()
            .find(|c| c["id"] == "lexical-semantic-cpu")
            .unwrap();
        let h = hash["paraphraseRecallAt3"].as_f64().unwrap_or(0.0);
        let l = lex["paraphraseRecallAt3"].as_f64().unwrap_or(0.0);
        assert!(
            l > h,
            "lexical paraphrase recall {l} should strictly exceed hash {h}"
        );
    }
    #[test]
    fn doc_cache_cosine_rank_matches_uncached_hash() {
        let fixture = load_coding_fixture(crate::CODING_FIXTURE).unwrap();
        let docs: Vec<(String, Vec<f32>)> = fixture
            .memories
            .iter()
            .filter(|m| m.insert)
            .map(|m| (m.key.clone(), embed_text(&doc_text(m))))
            .collect();
        for q in fixture.questions.iter().take(8) {
            let uncached = cosine_rank(&fixture, &q.prompt, &embed_text);
            let cached = cosine_rank_with_doc_cache(&q.prompt, &embed_text, &docs);
            assert_eq!(uncached, cached, "family {}", q.family);
        }
    }

    #[test]
    #[cfg(feature = "neural-ort")]
    fn doc_cache_cosine_rank_matches_uncached_onnx() {
        if crate::onnx_embed::try_embed_text("warmup").is_err() {
            eprintln!("skip: onnx model not configured");
            return;
        }
        let fixture = load_coding_fixture(crate::CODING_FIXTURE).unwrap();
        let doc_pairs: Vec<(String, String)> = fixture
            .memories
            .iter()
            .filter(|m| m.insert)
            .map(|m| (m.key.clone(), doc_text(m)))
            .collect();
        let insert_count = doc_pairs.len();
        let doc_cache = crate::onnx_embed::BakeoffDocCache::build(doc_pairs).expect("cache");
        assert_eq!(doc_cache.entries().len(), insert_count);
        let embed = |text: &str| crate::onnx_embed::try_embed_text(text).unwrap_or_default();
        for q in fixture.questions.iter().take(8) {
            let uncached = cosine_rank(&fixture, &q.prompt, &embed);
            let cached = cosine_rank_with_doc_cache(&q.prompt, &embed, doc_cache.entries());
            assert_eq!(uncached, cached, "family {}", q.family);
        }
    }

    #[test]
    fn dump_embed_bakeoff_json_when_env_set() {
        use std::path::PathBuf;
        let Some(path) = std::env::var_os("AX_DUMP_EMBED_BAKEOFF") else {
            return;
        };
        let report = run_embed_bakeoff(crate::CODING_FIXTURE).unwrap();
        std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        eprintln!("wrote {}", PathBuf::from(path).display());
    }
}
