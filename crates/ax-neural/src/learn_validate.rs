//! §4 learning validation: before/after + held-out eval + restart stability.
//! Never auto-promotes; never trains on the evaluation fold.

use crate::fixture::LOCKED;
use ax_memory::bench::{load_coding_fixture, CodingFixture};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

use crate::metrics::{Acc, SummaryStyle, LEARN_VALIDATE};

fn feedback_from_fixture(fixture: &CodingFixture, exclude_ids: &HashSet<String>) -> Vec<Value> {
    fixture
        .questions
        .iter()
        .filter(|q| !exclude_ids.contains(&q.id))
        .filter(|q| !q.relevant.is_empty())
        .map(|q| {
            json!({
                "executionId": format!("learn-validate-{}", q.id),
                "projectId": "learn-validate",
                "taskCategory": q.family,
                "outcome": "success",
                "contextIds": q.relevant.iter().map(|id| format!("memory:{id}")).collect::<Vec<_>>(),
                "skillIds": [],
                "ruleIds": [],
                "relevance": q.relevant.iter().map(|id| json!({ "id": id, "label": "relevant" })).collect::<Vec<_>>()
            })
        })
        .collect()
}

fn rank_from_weights(weights: &BTreeMap<String, f64>) -> Vec<String> {
    let mut scored: Vec<(f64, String)> = weights
        .iter()
        .filter_map(|(k, w)| {
            k.strip_prefix("contextIds:memory:")
                .map(|id| (*w, id.to_string()))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    scored.into_iter().map(|(_, k)| k).collect()
}

fn baseline_rank(fixture: &CodingFixture, prompt: &str) -> Vec<String> {
    // Before learning: lexical order of insertable memories by title overlap (no feedback).
    let tokens: Vec<String> = prompt
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() > 2)
        .map(|t| t.to_ascii_lowercase())
        .collect();
    let mut scored: Vec<(f64, String)> = fixture
        .memories
        .iter()
        .filter(|m| m.insert)
        .map(|m| {
            let text =
                format!("{} {} {}", m.title, m.body, m.triggers.join(" ")).to_ascii_lowercase();
            let score = tokens.iter().filter(|t| text.contains(t.as_str())).count() as f64;
            (score, m.key.clone())
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    scored.into_iter().map(|(_, k)| k).collect()
}

/// Controlled learning experiment. Evaluation fold never appears in training rows.
pub fn run_learn_validate(fixture_text: &str) -> Result<Value, String> {
    let fixture = load_coding_fixture(fixture_text)?;
    let eval_ids: HashSet<String> = fixture
        .questions
        .iter()
        .filter(|q| LOCKED.contains(&q.family.as_str()))
        .map(|q| q.id.clone())
        .collect();
    let train_feedback = feedback_from_fixture(&fixture, &eval_ids);
    if train_feedback.len() < 2 {
        return Err("need at least 2 non-eval feedback rows".into());
    }

    // A. Before: no historical feedback
    let mut before = Acc::default();
    for q in fixture
        .questions
        .iter()
        .filter(|q| eval_ids.contains(&q.id))
    {
        before.observe(
            LEARN_VALIDATE,
            None,
            &q.relevant,
            &baseline_rank(&fixture, &q.prompt),
        );
    }

    // B/C. Feedback collected (explicit rows above); train candidate on non-eval only
    let trained = crate::learning::train_ranking_prior(
        &train_feedback,
        0.25,
        7,
        "learn-validate-train",
        "rank-prior-candidate",
    )?;
    assert_ne!(trained.get("promoted"), Some(&json!(true)));

    let weights: BTreeMap<String, f64> = trained
        .get("weights")
        .and_then(|w| w.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f)))
                .collect()
        })
        .unwrap_or_default();

    // D. Evaluate candidate on held-out locked questions (never in train_feedback)
    let mut after = Acc::default();
    let ranked_after = rank_from_weights(&weights);
    for q in fixture
        .questions
        .iter()
        .filter(|q| eval_ids.contains(&q.id))
    {
        after.observe(LEARN_VALIDATE, None, &q.relevant, &ranked_after);
    }

    // E. Compare candidate vs original (before)
    let compare = json!({
        "before": before.summary(SummaryStyle::LearnValidate),
        "after": after.summary(SummaryStyle::LearnValidate),
        "deltaPrecisionAt3": match (before.summary(SummaryStyle::LearnValidate)["precisionAt3"].as_f64(), after.summary(SummaryStyle::LearnValidate)["precisionAt3"].as_f64()) {
            (Some(b), Some(a)) => json!(a - b),
            _ => Value::Null
        },
        "deltaRecallAt3": match (before.summary(SummaryStyle::LearnValidate)["recallAt3"].as_f64(), after.summary(SummaryStyle::LearnValidate)["recallAt3"].as_f64()) {
            (Some(b), Some(a)) => json!(a - b),
            _ => Value::Null
        },
        "deltaMrr": match (before.summary(SummaryStyle::LearnValidate)["mrr"].as_f64(), after.summary(SummaryStyle::LearnValidate)["mrr"].as_f64()) {
            (Some(b), Some(a)) => json!(a - b),
            _ => Value::Null
        }
    });

    // F. Restart stability: retrain with same seed; reload weights from JSON; compare
    let retrained = crate::learning::train_ranking_prior(
        &train_feedback,
        0.25,
        7,
        "learn-validate-train",
        "rank-prior-candidate",
    )?;
    let weights2: BTreeMap<String, f64> = retrained
        .get("weights")
        .and_then(|w| w.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_f64().map(|f| (k.clone(), f)))
                .collect()
        })
        .unwrap_or_default();
    let serialized = serde_json::to_string(&weights).map_err(|e| e.to_string())?;
    let reloaded: BTreeMap<String, f64> =
        serde_json::from_str(&serialized).map_err(|e| e.to_string())?;
    let stable_retrain = weights == weights2;
    let stable_reload = weights == reloaded;
    let mut after_reload = Acc::default();
    let ranked_reload = rank_from_weights(&reloaded);
    for q in fixture
        .questions
        .iter()
        .filter(|q| eval_ids.contains(&q.id))
    {
        after_reload.observe(LEARN_VALIDATE, None, &q.relevant, &ranked_reload);
    }

    // Leakage check: no eval executionId in train set
    let train_exec: HashSet<String> = train_feedback
        .iter()
        .filter_map(|r| {
            r.get("executionId")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .collect();
    let leak = eval_ids
        .iter()
        .any(|id| train_exec.contains(&format!("learn-validate-{id}")));

    Ok(json!({
        "kind": "measured",
        "spec": "docs/specs/ax-neural/SPEC-PHASE5-LEARN.md",
        "scope": "controlled before/after learning on coding fixture; locked families held out of train",
        "productionDefaultsUnchanged": true,
        "semanticWeight": crate::SEMANTIC_WEIGHT,
        "autoPromote": false,
        "promoted": false,
        "trainSize": train_feedback.len(),
        "evalQuestionIds": eval_ids.iter().cloned().collect::<Vec<_>>(),
        "evalInTrain": leak,
        "candidate": {
            "modelVersion": "rank-prior-candidate",
            "weightCount": weights.len(),
            "metrics": trained.get("metrics").cloned().unwrap_or(Value::Null),
            "claimsNeuralInference": false
        },
        "beforeWithoutFeedback": before.summary(SummaryStyle::LearnValidate),
        "afterCandidate": after.summary(SummaryStyle::LearnValidate),
        "comparison": compare,
        "restart": {
            "kind": "measured",
            "method": "retrain same seed + JSON serialize/reload (simulates Ax restart without live daemon)",
            "weightsIdenticalAfterRetrain": stable_retrain,
            "weightsIdenticalAfterReload": stable_reload,
            "afterReload": after_reload.summary(SummaryStyle::LearnValidate),
            "rankingUnchangedAfterReload": after.summary(SummaryStyle::LearnValidate) == after_reload.summary(SummaryStyle::LearnValidate)
        },
        "note": "Candidate not promoted. Production ranking unchanged."
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learn_validate_never_promotes_and_keeps_eval_out_of_train() {
        let report = run_learn_validate(crate::CODING_FIXTURE).unwrap();
        assert_eq!(report["promoted"], false);
        assert_eq!(report["autoPromote"], false);
        assert_eq!(report["evalInTrain"], false);
        assert_eq!(report["restart"]["weightsIdenticalAfterRetrain"], true);
        assert_eq!(report["restart"]["weightsIdenticalAfterReload"], true);
    }

    #[test]
    fn dump_learn_validate_json_when_env_set() {
        use std::path::PathBuf;
        let Some(path) = std::env::var_os("AX_DUMP_LEARN_VALIDATE") else {
            return;
        };
        let report = run_learn_validate(crate::CODING_FIXTURE).unwrap();
        std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        eprintln!("wrote {}", PathBuf::from(path).display());
    }
}
