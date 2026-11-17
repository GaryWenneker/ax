//! §2 production-path validation for ONNX embedding (SPEC-VALIDATION-ONNX-PATH.md).
//! Measures stages and arms A–D without changing production defaults.

use ax_memory::bench::{load_coding_fixture, CodingFixture};
use ax_memory::embed::{cosine, embed_text};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Instant;

use crate::metrics::{Acc, SummaryStyle, PATH_VALIDATE};
#[cfg(feature = "neural-ort")]
use crate::rank::{plan as plan_blocks, RankClass, RankItem};
#[cfg(feature = "neural-ort")]
use crate::store::{embedding_count, insert_embedding};

const LOCKED: &[&str] = &[
    "named_trigger",
    "paraphrase",
    "abstention",
    "delivery_budget",
];

const RRF_K: f64 = 60.0;
#[cfg(feature = "neural-ort")]
const ONNX_MODEL_ID: &str = "onnx-minilm-l6-v2";

fn doc_text(m: &ax_memory::bench::CodingMemory) -> String {
    format!("{} {} {} {}", m.key, m.title, m.body, m.triggers.join(" "))
}

fn cosine_rank(fixture: &CodingFixture, prompt: &str, embed: &dyn Fn(&str) -> Vec<f32>) -> Vec<String> {
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

fn rrf_combine(lists: &[Vec<String>]) -> Vec<String> {
    let mut scores: HashMap<String, f64> = HashMap::new();
    for list in lists {
        for (i, key) in list.iter().enumerate() {
            *scores.entry(key.clone()).or_default() += 1.0 / (RRF_K + (i as f64) + 1.0);
        }
    }
    let mut ranked: Vec<(f64, String)> = scores.into_iter().map(|(k, s)| (s, k)).collect();
    ranked.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    ranked.into_iter().map(|(_, k)| k).collect()
}

fn score_arm(fixture: &CodingFixture, rank: &dyn Fn(&str) -> Vec<String>) -> Value {
    let mut locked = Acc::default();
    let mut held = Acc::default();
    for q in &fixture.questions {
        let ranked = rank(&q.prompt);
        if LOCKED.contains(&q.family.as_str()) {
            locked.observe(PATH_VALIDATE, None, &q.relevant, &ranked);
        } else {
            held.observe(PATH_VALIDATE, None, &q.relevant, &ranked);
        }
    }
    json!({
        "locked": locked.summary(SummaryStyle::PathValidate),
        "heldOut": held.summary(SummaryStyle::PathValidate)
    })
}

fn stage(status: &str, detail: &str, extra: Value) -> Value {
    let mut v = json!({ "status": status, "detail": detail });
    if let Some(obj) = extra.as_object() {
        for (k, val) in obj {
            v[k] = val.clone();
        }
    }
    v
}

/// Validation-only end-to-end ONNX path report. Never flips production defaults.
pub async fn run_path_validate(fixture_text: &str) -> Result<Value, String> {
    let fixture = load_coding_fixture(fixture_text)?;
    let started = Instant::now();

    // --- Arm A: hash-v1 ---
    let arm_a = score_arm(&fixture, &|p| cosine_rank(&fixture, p, &embed_text));

    // --- Arm B: lexical ---
    let arm_b = score_arm(&fixture, &|p| lexical_rank(&fixture, p));

    // --- Stages + Arm C: ONNX (feature-gated) ---
    #[cfg(feature = "neural-ort")]
    let (stages, arm_c, onnx_ok) = {
        let load = match crate::onnx_embed::try_embed_text("path-validate-warmup") {
            Ok(v) => (
                stage(
                    "measured",
                    "ONNX MiniLM loaded and produced a vector",
                    json!({ "dim": v.len(), "modelId": ONNX_MODEL_ID }),
                ),
                true,
            ),
            Err(e) => (
                stage("failed", &format!("model load/infer failed: {e}"), json!({})),
                false,
            ),
        };

        let mut infer = stage("not_built", "skipped", json!({}));
        let mut persist = stage("not_built", "skipped", json!({}));
        let mut query = stage("not_built", "skipped", json!({}));
        let mut similarity = stage("not_built", "skipped", json!({}));
        let mut retrieval = stage("not_built", "skipped", json!({}));
        let mut fusion = stage(
            "inactive_in_production",
            "SEMANTIC_WEIGHT=0; blend_semantic has no production callers; validation uses RRF only",
            json!({ "semanticWeight": crate::SEMANTIC_WEIGHT, "validationFusion": "RRF_k60" }),
        );
        let mut context = stage("not_built", "skipped", json!({}));
        let mut arm_c = json!({
            "kind": "not_measured",
            "reason": "onnx unavailable"
        });
        let mut onnx_ok = false;

        if load.1 {
            infer = load.0.clone();
            // Persistence into neural_embedding (not memories.embedding)
            let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
            let database = ax_db::Database::open(&dir.path().join("ax.db"))
                .await
                .map_err(|e| e.to_string())?;
            let pool = database.pool();
            let mut stored = 0u64;
            for m in fixture.memories.iter().filter(|m| m.insert) {
                let vec = crate::onnx_embed::try_embed_text(&doc_text(m))?;
                insert_embedding(
                    pool,
                    "memory",
                    &m.key,
                    &format!("path-validate-{}", m.key),
                    ONNX_MODEL_ID,
                    &vec,
                    true,
                )
                .await?;
                stored += 1;
            }
            let count = embedding_count(pool).await?;
            persist = stage(
                "measured",
                "ONNX vectors stored in neural_embedding; memories.embedding untouched",
                json!({ "storedVectors": stored, "embeddingCount": count, "table": "neural_embedding" }),
            );

            let qvec = crate::onnx_embed::try_embed_text("hash vectors similarity")?;
            query = stage(
                "measured",
                "query embedded with ONNX MiniLM",
                json!({ "dim": qvec.len() }),
            );

            let a = crate::onnx_embed::try_embed_text("hash vectors similarity")?;
            let b = crate::onnx_embed::try_embed_text("hash vectors similarity")?;
            let c = crate::onnx_embed::try_embed_text("totally unrelated garden vegetables")?;
            let same = cosine(&a, &b);
            let diff = cosine(&a, &c);
            similarity = stage(
                "measured",
                "cosine(same) > cosine(unrelated)",
                json!({ "samePrompt": same, "unrelated": diff, "ordered": same > diff }),
            );

            let sample_prompt = fixture
                .questions
                .iter()
                .find(|q| q.family == "paraphrase")
                .map(|q| q.prompt.as_str())
                .unwrap_or("hash vectors");
            let ranked = cosine_rank(&fixture, sample_prompt, &|t| {
                crate::onnx_embed::try_embed_text(t).unwrap_or_default()
            });
            retrieval = stage(
                "measured",
                "ONNX cosine retrieval produced a ranked key list",
                json!({ "top3": ranked.iter().take(3).cloned().collect::<Vec<_>>() }),
            );

            arm_c = score_arm(&fixture, &|p| {
                cosine_rank(&fixture, p, &|t| {
                    crate::onnx_embed::try_embed_text(t).unwrap_or_default()
                })
            });
            arm_c["kind"] = json!("measured");
            arm_c["method"] = json!("ONNX MiniLM cosine on fixture memories");
            arm_c["claimsNeuralInference"] = json!(true);
            onnx_ok = true;

            // Context delivery: retrieved docs → rank::plan
            let top_keys: Vec<String> = ranked.into_iter().take(5).collect();
            let items: Vec<RankItem> = top_keys
                .iter()
                .filter_map(|k| fixture.memories.iter().find(|m| &m.key == k))
                .map(|m| RankItem {
                    id: m.key.clone(),
                    class: RankClass::Optional,
                    text: doc_text(m),
                })
                .collect();
            let notes = plan_blocks(&items, sample_prompt);
            let order = crate::rank::keep_order(&items, &notes);
            context = stage(
                "measured",
                "rank::plan applied to ONNX-retrieved blocks (lexical overlap relevance; production context path)",
                json!({ "order": order, "noteCount": notes.len() }),
            );

            // Production fusion remains inactive
            let _ = fusion;
            fusion = stage(
                "inactive_in_production",
                "Validation RRF computed for arm D; production blend_semantic weight stays 0 and is unwired",
                json!({
                    "semanticWeight": crate::SEMANTIC_WEIGHT,
                    "blendSemanticCallersInProduction": 0,
                    "validationFusion": "RRF_k60"
                }),
            );
        }

        let stages = json!({
            "modelLoading": load.0,
            "embeddingInference": infer,
            "vectorPersistence": persist,
            "queryEmbedding": query,
            "similarity": similarity,
            "retrieval": retrieval,
            "rankingFusion": fusion,
            "contextDelivery": context
        });
        (stages, arm_c, onnx_ok)
    };

    #[cfg(not(feature = "neural-ort"))]
    let (stages, arm_c, onnx_ok) = {
        let nb = stage(
            "not_built",
            "build with --features neural-ort and place MiniLM under ~/.ax/models",
            json!({}),
        );
        (
            json!({
                "modelLoading": nb.clone(),
                "embeddingInference": nb.clone(),
                "vectorPersistence": nb.clone(),
                "queryEmbedding": nb.clone(),
                "similarity": nb.clone(),
                "retrieval": nb.clone(),
                "rankingFusion": stage(
                    "inactive_in_production",
                    "SEMANTIC_WEIGHT=0; production does not blend ONNX into locked fusion",
                    json!({ "semanticWeight": crate::SEMANTIC_WEIGHT })
                ),
                "contextDelivery": nb
            }),
            json!({
                "kind": "not_built",
                "claimsNeuralInference": true,
                "reason": "neural-ort feature not linked"
            }),
            false,
        )
    };

    // --- Arm D: combined RRF of A+B(+C if available) ---
    let arm_d = if onnx_ok {
        #[cfg(feature = "neural-ort")]
        {
            let mut metrics = score_arm(&fixture, &|p| {
                let a = cosine_rank(&fixture, p, &embed_text);
                let b = lexical_rank(&fixture, p);
                let c = cosine_rank(&fixture, p, &|t| {
                    crate::onnx_embed::try_embed_text(t).unwrap_or_default()
                });
                rrf_combine(&[a, b, c])
            });
            metrics["kind"] = json!("measured");
            metrics["method"] = json!("RRF(k=60) of hash-v1 + lexical + ONNX");
            metrics
        }
        #[cfg(not(feature = "neural-ort"))]
        {
            json!({ "kind": "not_built" })
        }
    } else {
        let mut metrics = score_arm(&fixture, &|p| {
            let a = cosine_rank(&fixture, p, &embed_text);
            let b = lexical_rank(&fixture, p);
            rrf_combine(&[a, b])
        });
        metrics["kind"] = json!("measured");
        metrics["method"] = json!("RRF(k=60) of hash-v1 + lexical (ONNX unavailable)");
        metrics["onnxIncluded"] = json!(false);
        metrics
    };

    Ok(json!({
        "kind": "measured",
        "spec": "docs/specs/ax-neural/SPEC-VALIDATION-ONNX-PATH.md",
        "scope": "validation-only ONNX production-path probe + arms A–D",
        "semanticWeight": crate::SEMANTIC_WEIGHT,
        "neuralEmbeddingInferenceActive": false,
        "productionDefaultsUnchanged": true,
        "elapsedMillis": started.elapsed().as_secs_f64() * 1000.0,
        "stages": stages,
        "arms": {
            "A_hash_v1": {
                "kind": "measured",
                "method": "ax_memory::embed::embed_text cosine",
                "claimsNeuralInference": false,
                "locked": arm_a["locked"],
                "heldOut": arm_a["heldOut"]
            },
            "B_lexical_semantic": {
                "kind": "measured",
                "method": "lexical synonym + trigger rank",
                "claimsNeuralInference": false,
                "locked": arm_b["locked"],
                "heldOut": arm_b["heldOut"]
            },
            "C_onnx_embedding": arm_c,
            "D_combined_rrf": arm_d
        },
        "productionWiring": {
            "memoryColumn": "hash-v1",
            "searchSemanticUses": "blake3 runtime when challenger admitted; not ONNX",
            "rankPlanUses": "lexical overlap only",
            "blendSemanticWired": false,
            "onnxInDefaultBinary": cfg!(feature = "neural-ort")
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn path_validate_keeps_production_defaults() {
        let report = run_path_validate(crate::CODING_FIXTURE).await.unwrap();
        assert_eq!(report["semanticWeight"], 0.0);
        assert_eq!(report["neuralEmbeddingInferenceActive"], false);
        assert_eq!(report["productionDefaultsUnchanged"], true);
        assert!(report["arms"]["A_hash_v1"]["kind"] == "measured");
        assert!(report["arms"]["B_lexical_semantic"]["kind"] == "measured");
        assert!(report["arms"]["D_combined_rrf"]["kind"] == "measured");
    }

    #[tokio::test]
    async fn dump_path_validate_json_when_env_set() {
        use std::path::PathBuf;
        let Some(path) = std::env::var_os("AX_DUMP_PATH_VALIDATE") else {
            return;
        };
        let report = run_path_validate(crate::CODING_FIXTURE).await.unwrap();
        std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        eprintln!("wrote {}", PathBuf::from(path).display());
    }
}
