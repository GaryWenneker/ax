//! Phase 6 agent-evaluation harness (SPEC-PHASE6-AGENT.md).
//! Measures context retrieval quality across Ax / Neural arms on the coding fixture.
//! Live LLM task-success / token counts stay `not_measured` until an identical-model run is executed.

use crate::metrics::{Acc, SummaryStyle, AGENT_EVAL};
use ax_memory::bench::{load_coding_fixture, score_coding};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Instant;

/// Fixture arms A–E. JSON keys are stable via `serde(rename = …)` on each variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, serde::Deserialize)]
pub enum ArmId {
    #[serde(rename = "A_ax_without_neural_ranking")]
    A,
    #[serde(rename = "B_ax_original_ranking")]
    B,
    #[serde(rename = "C_ax_neural_ranking")]
    C,
    #[serde(rename = "D_ax_neural_prediction")]
    D,
    #[serde(rename = "E_ax_neural_learning")]
    E,
}

impl ArmId {
    pub const ALL: [Self; 5] = [Self::A, Self::B, Self::C, Self::D, Self::E];

    pub fn json_key(self) -> &'static str {
        match self {
            Self::A => "A_ax_without_neural_ranking",
            Self::B => "B_ax_original_ranking",
            Self::C => "C_ax_neural_ranking",
            Self::D => "D_ax_neural_prediction",
            Self::E => "E_ax_neural_learning",
        }
    }
}

const LOCKED: &[&str] = &[
    "named_trigger",
    "paraphrase",
    "abstention",
    "delivery_budget",
];

fn lexical_rank(fixture: &ax_memory::bench::CodingFixture, prompt: &str) -> Vec<String> {
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

fn feedback_rows_excluding(
    fixture: &ax_memory::bench::CodingFixture,
    exclude_id: &str,
) -> Vec<Value> {
    fixture
        .questions
        .iter()
        .filter(|q| q.id != exclude_id && !q.relevant.is_empty())
        .map(|q| {
            json!({
                "taskCategory": q.family,
                "outcome": "success",
                "contextIds": q.relevant.iter().map(|id| format!("memory:{id}")).collect::<Vec<_>>(),
                "skillIds": [],
                "ruleIds": []
            })
        })
        .collect()
}

fn rank_from_prior_weights(weights: &serde_json::Map<String, Value>) -> Vec<String> {
    let mut scored: Vec<(String, f64)> = weights
        .iter()
        .filter_map(|(k, v)| {
            let id = k.strip_prefix("contextIds:memory:")?;
            let w = v.as_f64()?;
            Some((id.to_string(), w))
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(id, _)| id).collect()
}

/// Fixture arms A–E. LLM task metrics are explicitly not_measured.
pub async fn run_agent_eval(fixture_text: &str) -> Result<Value, String> {
    let fixture = load_coding_fixture(fixture_text)?;
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let database = ax_db::Database::open(&dir.path().join("agent-eval.db"))
        .await
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    let scored = score_coding(database.pool(), &fixture)
        .await
        .map_err(|e| e.to_string())?;
    let score_millis = started.elapsed().as_secs_f64() * 1000.0;
    database.pool().close().await;

    let mut arm_ax = Acc::default(); // B: original Ax fusion
    let mut arm_bm25 = Acc::default(); // A: without neural ranking adapter
    let mut arm_neural = Acc::default(); // C: lexical neural retrieval when admitted
    let mut arm_predict = Acc::default(); // D: prediction leave-one-out memories as ranked list
    let mut arm_learn = Acc::default(); // E: ranking-prior after leave-one-out feedback

    let lexical_on = crate::bakeoff::challenger_admitted();
    let pred_report =
        crate::predict::evaluate_fixture(fixture_text, 0.75).unwrap_or_else(|_| json!({}));
    let mut learn_train_reports = Vec::new();

    for question in fixture
        .questions
        .iter()
        .filter(|q| LOCKED.contains(&q.family.as_str()))
    {
        let score = scored
            .questions
            .iter()
            .find(|s| s.id == question.id)
            .ok_or_else(|| format!("missing score {}", question.id))?;
        arm_bm25.observe(AGENT_EVAL, None, &question.relevant, &score.bm25);
        arm_ax.observe(AGENT_EVAL, None, &question.relevant, &score.ax);
        let neural_ranked = if lexical_on {
            lexical_rank(&fixture, &question.prompt)
        } else {
            score.ax.clone()
        };
        arm_neural.observe(AGENT_EVAL, None, &question.relevant, &neural_ranked);

        // Prediction arm: frequency prior from other fixture families as a ranked memory list.
        let rows: Vec<crate::predict::Observation> = fixture
            .questions
            .iter()
            .filter(|q| q.id != question.id)
            .map(|q| crate::predict::Observation {
                task_category: q.family.clone(),
                skills: Vec::new(),
                rules: Vec::new(),
                context_ids: q.relevant.iter().map(|id| format!("memory:{id}")).collect(),
            })
            .collect();
        let predicted = crate::predict::predict(&rows, &question.family, 0.5);
        let ranked: Vec<String> = predicted
            .into_iter()
            .filter(|p| p.kind == "memory")
            .map(|p| p.target_id)
            .collect();
        arm_predict.observe(AGENT_EVAL, None, &question.relevant, &ranked);

        // Learning arm: train ranking-prior on leave-one-out feedback (not promoted).
        let feedback = feedback_rows_excluding(&fixture, &question.id);
        if feedback.len() >= 2 {
            let trained = crate::learning::train_ranking_prior(
                &feedback,
                0.25,
                42,
                "agent-eval-loo",
                "rank-prior-agent-eval",
            )?;
            learn_train_reports.push(json!({
                "questionId": question.id,
                "promoted": trained.get("promoted").cloned().unwrap_or(json!(false)),
                "weightCount": trained.get("weightCount").cloned().unwrap_or(json!(0)),
                "metrics": trained.get("metrics").cloned().unwrap_or(Value::Null)
            }));
            let ranked_learn = trained
                .get("weights")
                .and_then(|w| w.as_object())
                .map(rank_from_prior_weights)
                .unwrap_or_default();
            arm_learn.observe(AGENT_EVAL, None, &question.relevant, &ranked_learn);
        } else {
            arm_learn.observe(AGENT_EVAL, None, &question.relevant, &[]);
        }
    }

    let memory = crate::rss::memory_report();

    let mut arms: HashMap<ArmId, Value> = HashMap::new();
    arms.insert(
        ArmId::A,
        json!({
            "method": "bm25 rankings (benchmark adapter)",
            "locked": arm_bm25.summary(SummaryStyle::AgentEval)
        }),
    );
    arms.insert(
        ArmId::B,
        json!({
            "method": "Ax locked fusion + hash-v1",
            "locked": arm_ax.summary(SummaryStyle::AgentEval)
        }),
    );
    arms.insert(
        ArmId::C,
        json!({
            "method": if lexical_on {
                "lexical-semantic-cpu when admitted"
            } else {
                "falls back to Ax fusion"
            },
            "lexicalAdmitted": lexical_on,
            "locked": arm_neural.summary(SummaryStyle::AgentEval)
        }),
    );
    arms.insert(
        ArmId::D,
        json!({
            "method": "feedback-style leave-one-out memory predictions as ranked context",
            "evaluateFixture": {
                "precision": pred_report.get("precision").cloned().unwrap_or(Value::Null),
                "coverage": pred_report.get("coverage").cloned().unwrap_or(Value::Null),
                "kind": pred_report.get("kind").cloned().unwrap_or(json!("unavailable"))
            },
            "locked": arm_predict.summary(SummaryStyle::AgentEval)
        }),
    );
    arms.insert(
        ArmId::E,
        json!({
            "method": "leave-one-out ranking-prior from fixture feedback (promoted=false)",
            "trainReports": learn_train_reports,
            "locked": arm_learn.summary(SummaryStyle::AgentEval)
        }),
    );
    let arms_json =
        serde_json::to_value(arms).map_err(|e| format!("serialize agent-eval arms: {e}"))?;

    Ok(json!({
        "kind": "measured",
        "scope": "coding-fixture context selection (proxy for agent context quality)",
        "liveLlmTaskSuccess": {
            "kind": "not_measured",
            "reason": "identical live agent models were not executed in this harness run"
        },
        "tokens": {
            "kind": "not_measured",
            "reason": "no LLM turn in this harness"
        },
        "retries": {
            "kind": "not_measured",
            "reason": "no agent loop"
        },
        "semanticWeight": crate::SEMANTIC_WEIGHT,
        "productionDefaultsUnchanged": true,
        "latency": {
            "kind": "measured",
            "scoreCodingMillis": score_millis
        },
        "memory": memory,
        "arms": arms_json,
        "attribution": "Context P/R differences are attributable to ranking/prediction/learning adapters on a fixed fixture, not to an underlying LLM",
        "database": "temporary",
        "openedLiveDatabase": false
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm_id_serde_map_keys_match_stable_json_names() {
        for arm in ArmId::ALL {
            let key = arm.json_key();
            let mut arms = HashMap::new();
            arms.insert(arm, json!(null));
            let serialized = serde_json::to_value(arms).expect("serialize arms map");
            assert!(
                serialized.get(key).is_some(),
                "serde map key missing for {key:?}"
            );
        }
    }

    #[tokio::test]
    async fn agent_eval_samples_rss_once_per_run() {
        crate::rss::reset_process_rss_sample_calls();
        let report = run_agent_eval(crate::CODING_FIXTURE).await.unwrap();
        assert_eq!(report["kind"], "measured");
        assert_eq!(crate::rss::take_process_rss_sample_calls(), 1);
    }

    #[tokio::test]
    async fn agent_eval_measures_four_arms_without_fabricating_llm_metrics() {
        let report = run_agent_eval(crate::CODING_FIXTURE).await.unwrap();
        assert_eq!(report["kind"], "measured");
        assert_eq!(report["liveLlmTaskSuccess"]["kind"], "not_measured");
        assert_eq!(report["tokens"]["kind"], "not_measured");
        assert_eq!(report["semanticWeight"], 0.0);
        assert_eq!(report["productionDefaultsUnchanged"], true);
        match report["memory"]["kind"].as_str() {
            Some("measured") => {
                assert!(report["memory"]["rssBytes"].as_u64().unwrap() > 0);
            }
            Some("not_measured") if cfg!(windows) => {}
            other => panic!("unexpected memory kind on this host: {other:?}"),
        }
        for arm in ArmId::ALL {
            assert!(
                report["arms"][arm.json_key()]["locked"]["questionsScored"]
                    .as_u64()
                    .unwrap()
                    >= 1,
                "{}",
                arm.json_key()
            );
        }
        assert_eq!(
            report["arms"][ArmId::E.json_key()]["trainReports"][0]["promoted"],
            false
        );
    }
}
