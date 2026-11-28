//! Phase 3 embedding-runtime bake-off (SPEC-PHASE3-EMBED.md).
//! Compares hash / lexical / blake3; reports ONNX and Candle as not_built until linked + configured.

#[cfg(any(test, feature = "neural-ort"))]
use crate::fixture::cosine_rank;
#[cfg(test)]
use crate::fixture::doc_text;
use crate::fixture::{doc_vectors, lexical_rank, rank_with_doc_vectors, Embed, LOCKED};
use ax_memory::bench::{load_coding_fixture, CodingFixture, CodingQuestion};
#[cfg(test)]
use ax_memory::embed::cosine;
use ax_memory::embed::embed_text;
use serde_json::{json, Value};
use std::time::Instant;

use crate::metrics::{Acc, SummaryStyle, EMBED_BAKEOFF};
use crate::runtime::Backend;

fn locked_questions(fixture: &CodingFixture) -> impl Iterator<Item = &CodingQuestion> {
    fixture
        .questions
        .iter()
        .filter(|q| LOCKED.contains(&q.family.as_str()))
}

/// Locked-family metrics and the median per-question latency of one embedding candidate.
/// Document vectors are computed before timing, so every candidate's latency is query
/// embedding plus ranking. The first embedding error is returned.
fn measure_locked(
    fixture: &CodingFixture,
    embed: Embed<'_>,
    docs: &[(String, Vec<f32>)],
) -> Result<(Acc, f64), String> {
    let mut times = Vec::new();
    let mut acc = Acc::default();
    for q in locked_questions(fixture) {
        let t0 = Instant::now();
        let ranked = rank_with_doc_vectors(&q.prompt, embed, docs)?;
        times.push(t0.elapsed().as_secs_f64() * 1_000_000.0);
        acc.observe(EMBED_BAKEOFF, Some(&q.family), &q.relevant, &ranked);
    }
    Ok((acc, crate::fixture::median(&mut times)))
}

/// One measured embedding candidate, or a `failed` candidate naming the embedding error.
fn embedding_candidate(fixture: &CodingFixture, id: &str, method: &str, embed: Embed<'_>) -> Value {
    let measured =
        doc_vectors(fixture, embed).and_then(|docs| measure_locked(fixture, embed, &docs));
    match measured {
        Ok((acc, median)) => candidate_measured(id, method, false, false, acc, median),
        Err(reason) => candidate_failed(id, method, false, &reason),
    }
}

fn candidate_failed(id: &str, method: &str, claims_neural: bool, reason: &str) -> Value {
    json!({
        "id": id,
        "kind": "failed",
        "method": method,
        "claimsNeuralInference": claims_neural,
        "admitted": false,
        "reason": format!("embedding failed: {reason}")
    })
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
        embedding_candidate(
            &fixture,
            "hash-v1",
            "ax_memory::embed::embed_text cosine retrieval",
            &|t| Ok(embed_text(t)),
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
        candidates.push(embedding_candidate(
            &fixture,
            "cpu-blake3",
            "blake3 bag vector via neural-runtime (deterministic; not semantic)",
            &|t| backend.embed(t),
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
        const ONNX_ID: &str = "onnx-runtime-cpu";
        const ONNX_METHOD: &str =
            "ONNX Runtime CPU + local all-MiniLM-L6-v2.onnx (neural embedding inference)";
        match crate::onnx_embed::try_embed_text("warmup") {
            Ok(_) => {
                let embed: Embed<'_> = &crate::onnx_embed::try_embed_text;
                let measured = (|| -> Result<Value, String> {
                    let docs = doc_vectors(&fixture, embed)?;
                    let (acc, median) = measure_locked(&fixture, embed, &docs)?;
                    let paraphrase = acc.paraphrase_recall_at_3();
                    let mut held = Acc::default();
                    let mut hash_held = Acc::default();
                    for q in fixture
                        .questions
                        .iter()
                        .filter(|q| !LOCKED.contains(&q.family.as_str()))
                    {
                        let ranked = rank_with_doc_vectors(&q.prompt, embed, &docs)?;
                        held.observe(EMBED_BAKEOFF, Some(&q.family), &q.relevant, &ranked);
                        let ranked = cosine_rank(&fixture, &q.prompt, &embed_text);
                        hash_held.observe(EMBED_BAKEOFF, Some(&q.family), &q.relevant, &ranked);
                    }
                    let mut cand =
                        candidate_measured(ONNX_ID, ONNX_METHOD, true, true, acc, median);
                    if let Ok((path, sum)) = crate::onnx_embed::loaded_model() {
                        cand["modelPath"] = json!(path.display().to_string());
                        cand["checksum"] = json!(sum);
                    }
                    cand["heldOut"] = held.summary(SummaryStyle::EmbedBakeoff);
                    cand["heldOutHashBaseline"] = hash_held.summary(SummaryStyle::EmbedBakeoff);
                    cand["validationDocEmbedCache"] = json!({
                        "documentsEmbeddedOnce": docs.len(),
                        "persistedToMemoryColumn": false
                    });
                    onnx_paraphrase = paraphrase;
                    Ok(cand)
                })();
                candidates.push(measured.unwrap_or_else(|reason| {
                    onnx_paraphrase = None;
                    candidate_failed(ONNX_ID, ONNX_METHOD, true, &reason)
                }));
            }
            Err(e) => {
                candidates.push(json!({
                    "id": ONNX_ID,
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
        "memory": crate::rss::memory_report(),
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
        let locked = crate::fixture::LOCKED;
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
        let embed: Embed<'_> = &crate::onnx_embed::try_embed_text;
        let locked = crate::fixture::LOCKED;
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
            let extra =
                rank_with_doc_vectors(&question.prompt, embed, doc_cache.entries()).expect("rank");
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
            let cached = rank_with_doc_vectors(&q.prompt, &|t| Ok(embed_text(t)), &docs).unwrap();
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
        let embed = |text: &str| crate::onnx_embed::try_embed_text(text).expect("embed");
        for q in fixture.questions.iter().take(8) {
            let uncached = cosine_rank(&fixture, &q.prompt, &embed);
            let cached = rank_with_doc_vectors(
                &q.prompt,
                &crate::onnx_embed::try_embed_text,
                doc_cache.entries(),
            )
            .expect("rank");
            assert_eq!(uncached, cached, "family {}", q.family);
        }
    }

    #[test]
    fn an_embedding_error_marks_the_candidate_failed() {
        let fixture = load_coding_fixture(crate::CODING_FIXTURE).unwrap();
        // Fails on the longest document only, like a text over the model's sequence limit.
        let longest = fixture
            .memories
            .iter()
            .filter(|m| m.insert)
            .map(|m| doc_text(m).len())
            .max()
            .unwrap();
        let embed = move |t: &str| {
            if t.len() >= longest {
                Err("sequence too long".to_string())
            } else {
                Ok(embed_text(t))
            }
        };
        let cand = embedding_candidate(&fixture, "flaky", "test", &embed);
        assert_eq!(cand["kind"], "failed", "{cand}");
        assert!(cand["reason"]
            .as_str()
            .unwrap()
            .contains("sequence too long"));
        assert_eq!(cand["admitted"], false);

        let query_fails = |t: &str| {
            if t.len() < longest {
                // Every question prompt is shorter than the documents' maximum.
                if fixture.questions.iter().any(|q| q.prompt == t) {
                    return Err("query failed".to_string());
                }
            }
            Ok(embed_text(t))
        };
        let cand = embedding_candidate(&fixture, "query-flaky", "test", &query_fails);
        assert_eq!(cand["kind"], "failed", "{cand}");
    }

    #[test]
    fn timed_ranking_embeds_documents_once_and_each_query_once() {
        let fixture = load_coding_fixture(crate::CODING_FIXTURE).unwrap();
        let calls = std::cell::Cell::new(0usize);
        let embed = |t: &str| {
            calls.set(calls.get() + 1);
            Ok(embed_text(t))
        };
        let docs = doc_vectors(&fixture, &embed).unwrap();
        let after_docs = calls.get();
        assert_eq!(after_docs, docs.len());
        let (acc, _) = measure_locked(&fixture, &embed, &docs).unwrap();
        let questions = locked_questions(&fixture).count();
        assert_eq!(calls.get() - after_docs, questions);
        assert!(acc.summary(SummaryStyle::EmbedBakeoff)["questionsScored"].as_u64() > Some(0));
    }

    #[test]
    fn embed_bakeoff_reports_measured_memory() {
        let report = run_embed_bakeoff(crate::CODING_FIXTURE).unwrap();
        if cfg!(any(target_os = "macos", target_os = "linux")) {
            assert_eq!(report["memory"]["kind"], "measured", "{}", report["memory"]);
            assert!(report["memory"]["rssBytes"].as_u64().unwrap() > 0);
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
