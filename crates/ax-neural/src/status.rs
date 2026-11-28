use crate::config::{load, NeuralConfig};
use serde_json::{json, Value};
use std::path::Path;

/// The status report for `root`. The first call may score the bake-off
/// fixture and block; async callers compute the admission with
/// [`crate::bakeoff::challenger_admitted_async`] and call
/// [`status_report_with`].
pub fn status_report(root: &Path) -> Value {
    status_report_with(root, crate::bakeoff::challenger_admitted())
}

/// The status report for `root` with the lexical challenger admission
/// already known, so it never scores the fixture itself.
pub fn status_report_with(root: &Path, lexical_admitted: bool) -> Value {
    report_with(&load(root), lexical_admitted)
}

pub fn report(cfg: &NeuralConfig) -> Value {
    report_with(cfg, crate::bakeoff::challenger_admitted())
}

/// [`report`] with the lexical challenger admission already known.
pub fn report_with(cfg: &NeuralConfig, lexical_admitted: bool) -> Value {
    let capabilities = capabilities_with(cfg, lexical_admitted);
    json!({
        "enabled": cfg.enabled,
        "mode": cfg.mode,
        "backend": "hash",
        "model": "hash-v1",
        "runtimeBuilt": crate::runtime_built(),
        "runtimeStatus": crate::runtime::status_json(),
        "mlRuntimeLinked": cfg!(feature = "neural-ort"),
        "learningEnabled": cfg.learning_enabled,
        "automaticTraining": false,
        "automaticModelPromotion": false,
        "shareLearningData": cfg.share_learning_data,
        "rankingActive": cfg.ranking_active(),
        "predictionActive": cfg.prediction_active(),
        // Deprecated: must not imply neural embedding inference. Always false until
        // capabilities.neuralEmbeddingInference.status == "active".
        "semanticActive": false,
        "semanticWeight": crate::SEMANTIC_WEIGHT,
        "productionDefaultsUnchanged": crate::PRODUCTION_DEFAULTS_UNCHANGED,
        "gnn": "not_in_binary",
        "configError": cfg.config_error,
        "promotedModels": [],
        "capabilities": capabilities,
        "metrics": {
            "feedbackCount": unavailable(),
            "tokenDelta": unavailable(),
            "rankLatency": unavailable(),
            "predictionAccuracy": unavailable()
        }
    })
}

fn cap(status: &str, kind: &str, detail: &str) -> Value {
    json!({ "status": status, "kind": kind, "detail": detail })
}

/// Honest capability matrix for Phase 1. Lexical ≠ neural inference.
fn neural_embedding_capability() -> (&'static str, &'static str, String) {
    #[cfg(feature = "neural-ort")]
    {
        crate::onnx_embed::capability_status()
    }
    #[cfg(not(feature = "neural-ort"))]
    {
        (
            "not_built",
            "onnx-runtime-cpu",
            "ONNX Runtime not linked (build with --features neural-ort); Candle still not_built"
                .into(),
        )
    }
}

pub fn capabilities(cfg: &NeuralConfig) -> Value {
    capabilities_with(cfg, crate::bakeoff::challenger_admitted())
}

/// [`capabilities`] with the lexical challenger admission already known.
pub fn capabilities_with(cfg: &NeuralConfig, lexical_admitted: bool) -> Value {
    let lexical = if cfg.semantic_enabled && lexical_admitted {
        cap(
            "active",
            "lexical-semantic-cpu",
            "Bake-off-admitted lexical synonym retrieval; not neural embedding inference",
        )
    } else if lexical_admitted {
        cap(
            "disabled",
            "lexical-semantic-cpu",
            "Lexical challenger admitted but semantic.enabled is false",
        )
    } else if crate::runtime_built() {
        cap(
            "implemented",
            "lexical-semantic-cpu",
            "Runtime built but lexical challenger not admitted on the coding fixture",
        )
    } else {
        cap(
            "implemented",
            "lexical-semantic-cpu",
            "Available behind neural-runtime; not active in this build",
        )
    };

    let ranking = if cfg.ranking_active() {
        cap(
            "active",
            "deterministic_context_ranking",
            "Required Neural context ranking; not a neural-network ranker",
        )
    } else {
        cap(
            "disabled",
            "deterministic_context_ranking",
            "Ranking inactive in config (should not occur while Neural is required)",
        )
    };

    let statistical_graph = if cfg.graph_enabled {
        cap(
            "active",
            "neighbor_overlap_cooccurrence",
            "Statistical graph suggestions enabled",
        )
    } else {
        cap(
            "disabled",
            "neighbor_overlap_cooccurrence",
            "graph.enabled defaults to false",
        )
    };

    let prediction = if cfg.prediction_active() {
        cap(
            "active",
            "feedback_based_prediction",
            "Prediction block enabled in config",
        )
    } else {
        cap(
            "disabled",
            "feedback_based_prediction",
            "prediction.enabled defaults to false",
        )
    };

    let feedback = if cfg.feedback_active() {
        cap(
            "active",
            "explicit_feedback_rows",
            "feedback_collection enabled",
        )
    } else {
        cap(
            "disabled",
            "explicit_feedback_rows",
            "feedback_collection defaults to false",
        )
    };

    let learning = if cfg.learning_enabled {
        cap(
            "disabled",
            "evaluate_register_only",
            "learning_enabled is on but no training loop runs; automaticTraining stays false",
        )
    } else {
        cap(
            "disabled",
            "evaluate_register_only",
            "No automatic training; evaluate/register/promote exist for manual use",
        )
    };

    let (embed_status, embed_kind, embed_note) = neural_embedding_capability();
    let neural_embed = cap(embed_status, embed_kind, &embed_note);
    json!({
        "lexicalRetrieval": lexical,
        "hashVectorRetrieval": cap(
            "active",
            "hash-v1",
            "Memory column memories.embedding uses hash-v1"
        ),
        "neuralEmbeddingInference": neural_embed,
        "neuralRanking": ranking,
        "statisticalGraph": statistical_graph,
        "graphNeuralNetwork": cap(
            "not_built",
            "gnn",
            "GNN weights are not in the binary"
        ),
        "prediction": prediction,
        "feedbackCollection": feedback,
        "learningAndTraining": learning,
        "modelPromotion": cap(
            "implemented",
            "explicit_promote_rollback",
            "Manual promote/rollback only; automaticModelPromotion is false"
        ),
        "hardwareAcceleration": cap(
            "not_built",
            "cuda_directml_coreml",
            "CPU path available; vendor GPU backends are not linked"
        )
    })
}

pub fn unavailable() -> Value {
    json!({ "value": null, "kind": "unavailable" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_project_status_names_hash_v1() {
        let dir = tempfile::tempdir().unwrap();
        let value = status_report(dir.path());
        assert_eq!(value["enabled"], true);
        assert_eq!(value["backend"], "hash");
        assert_eq!(value["model"], "hash-v1");
        assert_eq!(value["runtimeBuilt"], crate::runtime_built());
        assert_eq!(value["mlRuntimeLinked"], cfg!(feature = "neural-ort"));
        assert_eq!(value["gnn"], "not_in_binary");
        assert_eq!(value["semanticActive"], false);
        assert_eq!(value["semanticWeight"], crate::SEMANTIC_WEIGHT);
        assert_eq!(
            value["productionDefaultsUnchanged"],
            crate::PRODUCTION_DEFAULTS_UNCHANGED
        );
    }

    #[test]
    fn the_report_uses_the_admission_it_is_given() {
        let mut cfg = NeuralConfig::required();
        cfg.semantic_enabled = true;
        let admitted = report_with(&cfg, true);
        assert_eq!(
            admitted["capabilities"]["lexicalRetrieval"]["status"],
            "active"
        );
        let refused = report_with(&cfg, false);
        assert_eq!(
            refused["capabilities"]["lexicalRetrieval"]["status"],
            "implemented"
        );
        let dir = tempfile::tempdir().unwrap();
        let from_root = status_report_with(dir.path(), true);
        assert_eq!(
            from_root["capabilities"]["lexicalRetrieval"]["status"],
            "active"
        );
    }

    #[test]
    fn reports_whether_the_empty_neural_runtime_feature_is_on() {
        assert_eq!(
            crate::runtime_built(),
            cfg!(feature = "neural-runtime"),
            "runtime_built must match the neural-runtime feature for this build"
        );
    }

    #[test]
    fn semantic_active_never_implies_neural_embeddings() {
        let value = report(&NeuralConfig::required());
        assert_eq!(value["semanticActive"], false);
        assert_eq!(value["semanticWeight"], crate::SEMANTIC_WEIGHT);
        assert_eq!(
            value["productionDefaultsUnchanged"],
            crate::PRODUCTION_DEFAULTS_UNCHANGED
        );
        #[cfg(not(feature = "neural-ort"))]
        assert_eq!(
            value["capabilities"]["neuralEmbeddingInference"]["status"],
            "not_built"
        );
        #[cfg(feature = "neural-ort")]
        {
            let s = value["capabilities"]["neuralEmbeddingInference"]["status"]
                .as_str()
                .unwrap();
            assert!(
                s == "implemented" || s == "not_configured" || s == "active" || s == "not_built",
                "{s}"
            );
        }
        assert_eq!(
            value["metrics"]["predictionAccuracy"]["kind"],
            "unavailable"
        );
        assert_eq!(value["metrics"]["tokenDelta"]["kind"], "unavailable");
        assert_eq!(value["metrics"]["rankLatency"]["kind"], "unavailable");
        assert_eq!(value["promotedModels"], json!([]));
    }

    #[test]
    fn capabilities_report_distinguishes_lexical_from_neural_inference() {
        let value = report(&NeuralConfig::required());
        #[cfg(not(feature = "neural-ort"))]
        assert_eq!(
            value["capabilities"]["neuralEmbeddingInference"]["status"],
            "not_built"
        );
        assert_eq!(
            value["capabilities"]["hashVectorRetrieval"]["status"],
            "active"
        );
        assert_eq!(
            value["capabilities"]["hashVectorRetrieval"]["kind"],
            "hash-v1"
        );
        assert_eq!(
            value["capabilities"]["graphNeuralNetwork"]["status"],
            "not_built"
        );
        assert_eq!(
            value["capabilities"]["hardwareAcceleration"]["status"],
            "not_built"
        );
        let lexical = &value["capabilities"]["lexicalRetrieval"];
        if crate::bakeoff::challenger_admitted() {
            assert_eq!(lexical["status"], "active");
            assert_eq!(lexical["kind"], "lexical-semantic-cpu");
        } else {
            assert_ne!(lexical["status"], "active");
        }
        assert_eq!(value["semanticActive"], false);
    }

    #[test]
    fn capabilities_hash_vector_and_ranking_are_active() {
        let value = report(&NeuralConfig::required());
        assert_eq!(
            value["capabilities"]["hashVectorRetrieval"]["status"],
            "active"
        );
        assert_eq!(value["capabilities"]["neuralRanking"]["status"], "active");
        assert_eq!(
            value["capabilities"]["neuralRanking"]["kind"],
            "deterministic_context_ranking"
        );
        assert_eq!(value["capabilities"]["prediction"]["status"], "disabled");
        assert_eq!(
            value["capabilities"]["feedbackCollection"]["status"],
            "disabled"
        );
        assert_eq!(
            value["capabilities"]["statisticalGraph"]["status"],
            "disabled"
        );
        assert_eq!(
            value["capabilities"]["learningAndTraining"]["status"],
            "disabled"
        );
        assert_eq!(
            value["capabilities"]["modelPromotion"]["status"],
            "implemented"
        );
    }
}
