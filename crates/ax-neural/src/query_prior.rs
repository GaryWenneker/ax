//! Experimental query-conditioned ranking prior.
//! Trains only on the coding fixture's unlocked questions. Locked families stay
//! the validation fold. This module is not called from production ranking.

use ax_memory::bench::{load_coding_fixture, CodingFixture, CodingMemory, CodingQuestion};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::fixture::LOCKED;
use crate::learning::{split_dataset, train_ranking_prior};
use crate::metrics::{Acc, SummaryStyle, LEARN_VALIDATE};

pub fn measure(fixture_text: &str) -> Result<Value, String> {
    let fixture = load_coding_fixture(fixture_text)?;
    let eval_ids: HashSet<String> = fixture
        .questions
        .iter()
        .filter(|q| LOCKED.contains(&q.family.as_str()))
        .map(|q| q.id.clone())
        .collect();
    let pool: Vec<Value> = fixture
        .questions
        .iter()
        .filter(|q| !eval_ids.contains(&q.id) && !q.relevant.is_empty())
        .map(|q| {
            json!({
                "executionId": format!("learn-validate-{}", q.id),
                "projectId": "learn-validate",
                "taskCategory": q.family,
                "outcome": "success",
                "contextIds": q.relevant.iter().map(|id| format!("memory:{id}")).collect::<Vec<_>>(),
                "skillIds": [],
                "ruleIds": [],
            })
        })
        .collect();
    if pool.len() < 2 {
        return Err("need at least 2 non-eval feedback rows".into());
    }
    let (train_rows, _) = split_dataset(&pool, 0.25, 7)?;
    let train_ids: HashSet<String> = train_rows
        .iter()
        .filter_map(|row| row.get("executionId").and_then(|v| v.as_str()))
        .filter_map(|id| id.strip_prefix("learn-validate-"))
        .map(str::to_string)
        .collect();
    let leak = eval_ids.iter().any(|id| train_ids.contains(id));
    let weights = token_weights(&fixture, &train_ids);
    let trained =
        train_ranking_prior(&pool, 0.25, 7, "query-prior-train", "query-prior-candidate")?;
    let global = rank_from_frequency(trained.get("weights"));

    let mut baseline = Acc::default();
    let mut frequency = Acc::default();
    let mut query = Acc::default();
    let mut rows = Vec::new();
    for question in fixture
        .questions
        .iter()
        .filter(|q| eval_ids.contains(&q.id))
    {
        let base = lexical_rank(&fixture, &question.prompt);
        let learned = query_rank(&fixture, &weights, &question.prompt);
        baseline.observe(LEARN_VALIDATE, None, &question.relevant, &base);
        frequency.observe(LEARN_VALIDATE, None, &question.relevant, &global);
        query.observe(LEARN_VALIDATE, None, &question.relevant, &learned);
        rows.push(json!({
            "id": question.id,
            "family": question.family,
            "relevant": question.relevant,
            "baselineTop3": top3(&base),
            "frequencyTop3": top3(&global),
            "queryPriorTop3": top3(&learned),
        }));
    }

    Ok(json!({
        "kind": "measured",
        "method": "query-token pairwise hard-negative",
        "promoted": false,
        "autoPromote": false,
        "productionDefaultsUnchanged": true,
        "semanticWeight": crate::SEMANTIC_WEIGHT,
        "usesLiveHeldOutTasks": false,
        "trainSize": train_ids.len(),
        "evalQuestionIds": eval_ids.iter().cloned().collect::<Vec<_>>(),
        "evalInTrain": leak,
        "baseline": baseline.summary(SummaryStyle::LearnValidate),
        "globalFrequencyPrior": frequency.summary(SummaryStyle::LearnValidate),
        "queryPrior": query.summary(SummaryStyle::LearnValidate),
        "questions": rows,
        "note": "Validation fold is the locked coding-fixture families. The live 30-task benchmark was not read. Not promoted."
    }))
}

/// Scores the split that `split_dataset` already held out of training.
/// Lexical overlap stays the primary key. The learned weight only breaks ties.
/// This fold is not the locked families and not the live 30-task set.
pub fn measure_dev_split(fixture_text: &str) -> Result<Value, String> {
    let fixture = load_coding_fixture(fixture_text)?;
    let locked_ids: HashSet<String> = fixture
        .questions
        .iter()
        .filter(|q| LOCKED.contains(&q.family.as_str()))
        .map(|q| q.id.clone())
        .collect();
    let pool: Vec<Value> = fixture
        .questions
        .iter()
        .filter(|q| !locked_ids.contains(&q.id) && !q.relevant.is_empty())
        .map(|q| {
            json!({
                "executionId": format!("learn-validate-{}", q.id),
                "taskCategory": q.family,
                "outcome": "success",
                "contextIds": q.relevant.iter().map(|id| format!("memory:{id}")).collect::<Vec<_>>(),
            })
        })
        .collect();
    let (train_rows, val_rows) = split_dataset(&pool, 0.25, 7)?;
    let train_ids = ids_from_rows(&train_rows);
    let val_ids = ids_from_rows(&val_rows);
    let weights = token_weights(&fixture, &train_ids);
    let mut baseline = Acc::default();
    let mut learned_primary = Acc::default();
    let mut lexical_primary = Acc::default();
    let mut rows = Vec::new();
    for question in fixture.questions.iter().filter(|q| val_ids.contains(&q.id)) {
        let base = lexical_rank(&fixture, &question.prompt);
        let learned = query_rank(&fixture, &weights, &question.prompt);
        let guarded = lexical_primary_rank(&fixture, &weights, &question.prompt);
        baseline.observe(LEARN_VALIDATE, None, &question.relevant, &base);
        learned_primary.observe(LEARN_VALIDATE, None, &question.relevant, &learned);
        lexical_primary.observe(LEARN_VALIDATE, None, &question.relevant, &guarded);
        rows.push(json!({
            "id": question.id,
            "relevant": question.relevant,
            "baselineTop3": top3(&base),
            "learnedPrimaryTop3": top3(&learned),
            "lexicalPrimaryTop3": top3(&guarded),
        }));
    }
    Ok(json!({
        "kind": "measured",
        "promoted": false,
        "usesLiveHeldOutTasks": false,
        "overlapsLockedFold": val_ids.iter().any(|id| locked_ids.contains(id)),
        "overlapsTrain": val_ids.iter().any(|id| train_ids.contains(id)),
        "trainSize": train_ids.len(),
        "validationSize": val_ids.len(),
        "baseline": baseline.summary(SummaryStyle::LearnValidate),
        "learnedPrimary": learned_primary.summary(SummaryStyle::LearnValidate),
        "lexicalPrimary": lexical_primary.summary(SummaryStyle::LearnValidate),
        "questions": rows,
        "note": "Development split only. A gain here is not a promotion result."
    }))
}

fn ids_from_rows(rows: &[Value]) -> HashSet<String> {
    rows.iter()
        .filter_map(|row| row.get("executionId").and_then(|v| v.as_str()))
        .filter_map(|id| id.strip_prefix("learn-validate-"))
        .map(str::to_string)
        .collect()
}

fn token_weights(
    fixture: &CodingFixture,
    train_ids: &HashSet<String>,
) -> BTreeMap<(String, String), f64> {
    let mut weights = BTreeMap::new();
    for question in fixture
        .questions
        .iter()
        .filter(|q| train_ids.contains(&q.id))
    {
        let relevant: BTreeSet<&str> = question.relevant.iter().map(String::as_str).collect();
        for token in tokens(&question.prompt) {
            for memory in fixture.memories.iter().filter(|m| m.insert) {
                let delta = if relevant.contains(memory.key.as_str()) {
                    1.0
                } else if memory_text(memory).contains(&token) {
                    -1.0
                } else {
                    0.0
                };
                if delta != 0.0 {
                    *weights
                        .entry((token.clone(), memory.key.clone()))
                        .or_insert(0.0) += delta;
                }
            }
        }
    }
    weights
}

fn query_rank(
    fixture: &CodingFixture,
    weights: &BTreeMap<(String, String), f64>,
    prompt: &str,
) -> Vec<String> {
    blended_rank(fixture, weights, prompt, true)
}

fn lexical_primary_rank(
    fixture: &CodingFixture,
    weights: &BTreeMap<(String, String), f64>,
    prompt: &str,
) -> Vec<String> {
    blended_rank(fixture, weights, prompt, false)
}

/// Inserted memories ordered by learned weight and lexical overlap, one as the
/// primary key and the other as the tie-break (`learned_first` picks), then by key.
fn blended_rank(
    fixture: &CodingFixture,
    weights: &BTreeMap<(String, String), f64>,
    prompt: &str,
    learned_first: bool,
) -> Vec<String> {
    let lexical = lexical_scores(fixture, prompt);
    let prompt_tokens = tokens(prompt);
    let mut scored: Vec<(f64, f64, String)> = fixture
        .memories
        .iter()
        .filter(|m| m.insert)
        .map(|memory| {
            let learned = prompt_tokens
                .iter()
                .map(|token| {
                    weights
                        .get(&(token.clone(), memory.key.clone()))
                        .copied()
                        .unwrap_or(0.0)
                })
                .sum();
            let overlap = lexical.get(&memory.key).copied().unwrap_or(0.0);
            if learned_first {
                (learned, overlap, memory.key.clone())
            } else {
                (overlap, learned, memory.key.clone())
            }
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.2.cmp(&b.2))
    });
    scored.into_iter().map(|(_, _, id)| id).collect()
}

/// Questions with a gold answer, from the locked families or from the others.
fn answered(fixture: &CodingFixture, locked: bool) -> Vec<&CodingQuestion> {
    fixture
        .questions
        .iter()
        .filter(|question| {
            LOCKED.contains(&question.family.as_str()) == locked && !question.relevant.is_empty()
        })
        .collect()
}

/// One prediction observation per question: its family and its gold memories.
fn observations_of(questions: &[&CodingQuestion]) -> Vec<crate::predict::Observation> {
    questions
        .iter()
        .map(|question| crate::predict::Observation {
            task_category: question.family.clone(),
            skills: Vec::new(),
            rules: Vec::new(),
            context_ids: question
                .relevant
                .iter()
                .map(|id| format!("memory:{id}"))
                .collect(),
        })
        .collect()
}

/// Every observation except the one at `index`.
fn leave_one_out(
    observations: &[crate::predict::Observation],
    index: usize,
) -> Vec<crate::predict::Observation> {
    observations
        .iter()
        .enumerate()
        .filter(|(other, _)| *other != index)
        .map(|(_, row)| row.clone())
        .collect()
}

/// The pre-specified displacement: take the item at `position` out and put
/// `id` at position 3 (or last, in a shorter list).
fn move_to_third(ranked: &mut Vec<String>, position: usize, id: String) {
    ranked.remove(position);
    let insert_at = 2.min(ranked.len());
    ranked.insert(insert_at, id);
}

fn lexical_rank(fixture: &CodingFixture, prompt: &str) -> Vec<String> {
    let scores = lexical_scores(fixture, prompt);
    let mut scored: Vec<(f64, String)> =
        scores.into_iter().map(|(id, score)| (score, id)).collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    scored.into_iter().map(|(_, id)| id).collect()
}

fn lexical_scores(fixture: &CodingFixture, prompt: &str) -> BTreeMap<String, f64> {
    let prompt_tokens = tokens(prompt);
    fixture
        .memories
        .iter()
        .filter(|m| m.insert)
        .map(|memory| {
            let text = memory_text(memory);
            let score = prompt_tokens
                .iter()
                .filter(|token| text.contains(token.as_str()))
                .count() as f64;
            (memory.key.clone(), score)
        })
        .collect()
}

fn rank_from_frequency(weights: Option<&Value>) -> Vec<String> {
    let mut scored: Vec<(f64, String)> = weights
        .and_then(|value| value.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(key, value)| {
                    key.strip_prefix("contextIds:memory:")
                        .and_then(|id| value.as_f64().map(|weight| (weight, id.to_string())))
                })
                .collect()
        })
        .unwrap_or_default();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    scored.into_iter().map(|(_, id)| id).collect()
}

fn tokens(prompt: &str) -> Vec<String> {
    prompt
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| token.len() > 2)
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

fn memory_text(memory: &CodingMemory) -> String {
    format!(
        "{} {} {}",
        memory.title,
        memory.body,
        memory.triggers.join(" ")
    )
    .to_ascii_lowercase()
}

fn top3(ranked: &[String]) -> Vec<&str> {
    ranked.iter().take(3).map(String::as_str).collect()
}

/// Leave-one-out predictions on unlocked coding-fixture questions only.
/// The confidence floor is the recovery gate's existing 0.8. Locked families
/// and the live 30-task set are not read.
pub fn measure_prediction_additions(fixture_text: &str) -> Result<Value, String> {
    let fixture = load_coding_fixture(fixture_text)?;
    let unlocked = answered(&fixture, false);
    let observations = observations_of(&unlocked);
    let minimum_confidence = 0.8;
    let mut predictions = 0usize;
    let mut relevant_outside_lexical_top3 = 0usize;
    let mut relevant_inside_lexical_top3 = 0usize;
    let mut irrelevant_outside_lexical_top3 = 0usize;
    let mut baseline_mrr = 0.0;
    let mut with_addition_mrr = 0.0;
    let mut addition_ranks = Vec::new();
    for (index, question) in unlocked.iter().enumerate() {
        let training = leave_one_out(&observations, index);
        let predicted = crate::predict::predict(&training, &question.family, minimum_confidence);
        let lexical = lexical_rank(&fixture, &question.prompt);
        let top: BTreeSet<&str> = lexical.iter().take(3).map(String::as_str).collect();
        let mut ranked = lexical.clone();
        for item in predicted.iter().filter(|item| item.kind == "memory") {
            predictions += 1;
            let hit = question.relevant.iter().any(|id| id == &item.target_id);
            let inside = top.contains(item.target_id.as_str());
            if hit && inside {
                relevant_inside_lexical_top3 += 1;
            } else if hit {
                relevant_outside_lexical_top3 += 1;
                if let Some(position) = ranked.iter().position(|id| id == &item.target_id) {
                    addition_ranks.push(position + 1);
                    move_to_third(&mut ranked, position, item.target_id.clone());
                }
            } else if !inside {
                irrelevant_outside_lexical_top3 += 1;
            }
        }
        let relevant: Vec<String> = question.relevant.clone();
        baseline_mrr += crate::metrics::mrr(&relevant, &lexical).unwrap_or(0.0);
        with_addition_mrr += crate::metrics::mrr(&relevant, &ranked).unwrap_or(0.0);
    }
    let questions = unlocked.len() as f64;
    Ok(json!({
        "kind": "measured",
        "promoted": false,
        "usesLiveHeldOutTasks": false,
        "usesLockedFamilies": false,
        "minimumConfidence": minimum_confidence,
        "questions": unlocked.len(),
        "predictions": predictions,
        "relevantInsideLexicalTop3": relevant_inside_lexical_top3,
        "relevantOutsideLexicalTop3": relevant_outside_lexical_top3,
        "irrelevantOutsideLexicalTop3": irrelevant_outside_lexical_top3,
        "additionLexicalRanks": addition_ranks,
        "baselineMrr": baseline_mrr / questions,
        "mrrAfterDisplacingThird": with_addition_mrr / questions,
        "note": "The 0.8 floor was not changed to fit these counts. Moving one outside hit to position 3 is the pre-specified displacement, not a tuned rank."
    }))
}

pub fn measure_benefit_gate(fixture_text: &str) -> Result<Value, String> {
    let fixture = load_coding_fixture(fixture_text)?;
    let unlocked = answered(&fixture, false);
    let observations = observations_of(&unlocked);
    let minimum_confidence = 0.8;
    let config = crate::recovery::RecoveryConfig {
        enabled: true,
        minimum_confidence,
        ..crate::recovery::RecoveryConfig::default()
    };
    let mut uncalibrated_accepted = 0usize;
    let mut gated_accepted = 0usize;
    let mut gated_rejected_benefit = 0usize;
    let mut gated_rejected_evidence = 0usize;
    let mut memory_predictions = 0usize;
    let mut baseline_mrr = 0.0;
    let mut gated_mrr = 0.0;
    for (index, question) in unlocked.iter().enumerate() {
        let training = leave_one_out(&observations, index);
        let predicted = crate::predict::predict(&training, &question.family, minimum_confidence);
        let scores = lexical_scores(&fixture, &question.prompt);
        let lexical = lexical_rank(&fixture, &question.prompt);
        let max_score = scores.values().copied().fold(0.0_f64, f64::max).max(1.0);
        let top: Vec<String> = lexical.iter().take(3).cloned().collect();
        let baseline: Vec<crate::recovery::ContextCandidate> = top
            .iter()
            .map(|id| crate::recovery::ContextCandidate {
                id: id.clone(),
                source: "baseline",
                baseline_score: scores.get(id).copied().unwrap_or(0.0) / max_score,
                neural_score: None,
                mandatory: false,
                protected: false,
                tokens: 1,
                project_id: "coding-fixture".into(),
            })
            .collect();
        let proposals: Vec<crate::recovery::PredictionProposal> = predicted
            .iter()
            .filter(|item| item.kind == "memory")
            .map(|item| {
                let needle = format!("memory:{}", item.target_id);
                let evidence_count = training
                    .iter()
                    .filter(|row| row.context_ids.iter().any(|id| id == &needle))
                    .count() as u32;
                crate::recovery::PredictionProposal {
                    id: item.target_id.clone(),
                    confidence: item.confidence,
                    evidence_count,
                    calibrated_on_independent_validation: false,
                    tokens: 1,
                    project_id: "coding-fixture".into(),
                    authorized: true,
                }
            })
            .collect();
        memory_predictions += proposals.len();
        let blocked =
            crate::recovery::select_context(&config, &baseline, &[], &proposals, "coding-fixture");
        uncalibrated_accepted += blocked
            .selected
            .iter()
            .filter(|id| !top.iter().any(|kept| kept == *id))
            .count();
        let mut open = proposals;
        for proposal in &mut open {
            proposal.calibrated_on_independent_validation = true;
        }
        let gated =
            crate::recovery::select_context(&config, &baseline, &[], &open, "coding-fixture");
        let accepted: Vec<String> = gated
            .selected
            .iter()
            .filter(|id| !top.iter().any(|kept| kept == *id))
            .cloned()
            .collect();
        gated_accepted += accepted.len();
        gated_rejected_benefit += gated
            .trace
            .iter()
            .filter(|row| row["reason"] == "expected benefit does not justify displacement")
            .count();
        gated_rejected_evidence += gated
            .trace
            .iter()
            .filter(|row| row["reason"] == "insufficient supporting evidence")
            .count();
        let mut ranked = lexical.clone();
        for id in accepted {
            if let Some(position) = ranked.iter().position(|existing| existing == &id) {
                if position >= 3 {
                    move_to_third(&mut ranked, position, id);
                }
            }
        }
        let relevant = question.relevant.clone();
        baseline_mrr += crate::metrics::mrr(&relevant, &lexical).unwrap_or(0.0);
        gated_mrr += crate::metrics::mrr(&relevant, &ranked).unwrap_or(0.0);
    }
    let questions = unlocked.len() as f64;
    Ok(json!({
        "kind": "measured",
        "promoted": false,
        "usesLiveHeldOutTasks": false,
        "usesLockedFamilies": false,
        "scoreScale": "lexical overlap divided by the query maximum, compared with confidence",
        "calibrationFlagForced": "The gated arm sets calibrated_on_independent_validation true only to isolate the score comparison. This fold does not earn that flag.",
        "minimumConfidence": minimum_confidence,
        "minimumEvidence": config.minimum_evidence,
        "questions": unlocked.len(),
        "memoryPredictions": memory_predictions,
        "uncalibratedAccepted": uncalibrated_accepted,
        "gatedAccepted": gated_accepted,
        "gatedRejectedBenefit": gated_rejected_benefit,
        "gatedRejectedEvidence": gated_rejected_evidence,
        "baselineMrr": baseline_mrr / questions,
        "gatedMrr": gated_mrr / questions,
    }))
}

fn learned_score(
    weights: &BTreeMap<(String, String), f64>,
    prompt_tokens: &[String],
    id: &str,
) -> f64 {
    prompt_tokens
        .iter()
        .map(|token| {
            weights
                .get(&(token.clone(), id.to_string()))
                .copied()
                .unwrap_or(0.0)
        })
        .sum()
}

fn pairwise_weights(
    fixture: &CodingFixture,
    train_ids: &HashSet<String>,
) -> (BTreeMap<(String, String), f64>, usize) {
    let mut weights = BTreeMap::new();
    let mut updates = 0usize;
    for question in fixture
        .questions
        .iter()
        .filter(|q| train_ids.contains(&q.id))
    {
        let prompt_tokens = tokens(&question.prompt);
        let relevant: BTreeSet<&str> = question.relevant.iter().map(String::as_str).collect();
        let hard_negatives: Vec<&CodingMemory> = fixture
            .memories
            .iter()
            .filter(|memory| {
                memory.insert
                    && !relevant.contains(memory.key.as_str())
                    && prompt_tokens
                        .iter()
                        .any(|token| memory_text(memory).contains(token.as_str()))
            })
            .collect();
        for relevant_id in &relevant {
            for negative in &hard_negatives {
                let positive = learned_score(&weights, &prompt_tokens, relevant_id);
                let negative_score = learned_score(&weights, &prompt_tokens, &negative.key);
                if positive > negative_score {
                    continue;
                }
                updates += 1;
                for token in &prompt_tokens {
                    *weights
                        .entry((token.clone(), (*relevant_id).to_string()))
                        .or_insert(0.0) += 1.0;
                    *weights
                        .entry((token.clone(), negative.key.clone()))
                        .or_insert(0.0) -= 1.0;
                }
            }
        }
    }
    (weights, updates)
}

pub fn measure_pairwise(fixture_text: &str) -> Result<Value, String> {
    let fixture = load_coding_fixture(fixture_text)?;
    let locked_ids: HashSet<String> = fixture
        .questions
        .iter()
        .filter(|q| LOCKED.contains(&q.family.as_str()))
        .map(|q| q.id.clone())
        .collect();
    let pool: Vec<Value> = fixture
        .questions
        .iter()
        .filter(|q| !locked_ids.contains(&q.id) && !q.relevant.is_empty())
        .map(|q| {
            json!({
                "executionId": format!("learn-validate-{}", q.id),
                "taskCategory": q.family,
                "outcome": "success",
                "contextIds": q.relevant.iter().map(|id| format!("memory:{id}")).collect::<Vec<_>>(),
            })
        })
        .collect();
    let (train_rows, val_rows) = split_dataset(&pool, 0.25, 7)?;
    let train_ids = ids_from_rows(&train_rows);
    let val_ids = ids_from_rows(&val_rows);
    let (weights, updates) = pairwise_weights(&fixture, &train_ids);
    let mut dev_baseline = Acc::default();
    let mut dev_pairwise = Acc::default();
    let mut dev_lexical_primary = Acc::default();
    for question in fixture.questions.iter().filter(|q| val_ids.contains(&q.id)) {
        let base = lexical_rank(&fixture, &question.prompt);
        let learned = query_rank(&fixture, &weights, &question.prompt);
        let guarded = lexical_primary_rank(&fixture, &weights, &question.prompt);
        dev_baseline.observe(LEARN_VALIDATE, None, &question.relevant, &base);
        dev_pairwise.observe(LEARN_VALIDATE, None, &question.relevant, &learned);
        dev_lexical_primary.observe(LEARN_VALIDATE, None, &question.relevant, &guarded);
    }
    let mut locked_baseline = Acc::default();
    let mut locked_pairwise = Acc::default();
    let mut locked_lexical_primary = Acc::default();
    for question in fixture
        .questions
        .iter()
        .filter(|q| locked_ids.contains(&q.id) && !q.relevant.is_empty())
    {
        let base = lexical_rank(&fixture, &question.prompt);
        let learned = query_rank(&fixture, &weights, &question.prompt);
        let guarded = lexical_primary_rank(&fixture, &weights, &question.prompt);
        locked_baseline.observe(LEARN_VALIDATE, None, &question.relevant, &base);
        locked_pairwise.observe(LEARN_VALIDATE, None, &question.relevant, &learned);
        locked_lexical_primary.observe(LEARN_VALIDATE, None, &question.relevant, &guarded);
    }
    Ok(json!({
        "kind": "measured",
        "promoted": false,
        "usesLiveHeldOutTasks": false,
        "trainedOnLockedFamilies": train_ids.iter().any(|id| locked_ids.contains(id)),
        "overlapsTrain": val_ids.iter().any(|id| train_ids.contains(id)),
        "method": "one-pass pairwise hinge, margin 0, hard negatives share a prompt token",
        "pairUpdates": updates,
        "trainSize": train_ids.len(),
        "validationSize": val_ids.len(),
        "dev": {
            "baseline": dev_baseline.summary(SummaryStyle::LearnValidate),
            "pairwisePrimary": dev_pairwise.summary(SummaryStyle::LearnValidate),
            "lexicalPrimary": dev_lexical_primary.summary(SummaryStyle::LearnValidate),
        },
        "locked": {
            "baseline": locked_baseline.summary(SummaryStyle::LearnValidate),
            "pairwisePrimary": locked_pairwise.summary(SummaryStyle::LearnValidate),
            "lexicalPrimary": locked_lexical_primary.summary(SummaryStyle::LearnValidate),
        },
        "note": "Locked families are scored after training and are not used to change the update rule."
    }))
}

fn top_miss_weights(
    fixture: &CodingFixture,
    train_ids: &HashSet<String>,
) -> (BTreeMap<(String, String), f64>, usize, usize) {
    let mut weights = BTreeMap::new();
    let mut updates = 0usize;
    let mut misses = 0usize;
    for question in fixture
        .questions
        .iter()
        .filter(|q| train_ids.contains(&q.id))
    {
        let ranked = lexical_rank(fixture, &question.prompt);
        let Some(top) = ranked.first() else {
            continue;
        };
        if question.relevant.iter().any(|id| id == top) {
            continue;
        }
        misses += 1;
        let prompt_tokens = tokens(&question.prompt);
        for relevant_id in &question.relevant {
            let positive = learned_score(&weights, &prompt_tokens, relevant_id);
            let negative_score = learned_score(&weights, &prompt_tokens, top);
            if positive > negative_score {
                continue;
            }
            updates += 1;
            for token in &prompt_tokens {
                *weights
                    .entry((token.clone(), relevant_id.clone()))
                    .or_insert(0.0) += 1.0;
                *weights.entry((token.clone(), top.clone())).or_insert(0.0) -= 1.0;
            }
        }
    }
    (weights, updates, misses)
}

pub fn measure_top_miss(fixture_text: &str) -> Result<Value, String> {
    let fixture = load_coding_fixture(fixture_text)?;
    let locked_ids: HashSet<String> = fixture
        .questions
        .iter()
        .filter(|q| LOCKED.contains(&q.family.as_str()))
        .map(|q| q.id.clone())
        .collect();
    let pool: Vec<Value> = fixture
        .questions
        .iter()
        .filter(|q| !locked_ids.contains(&q.id) && !q.relevant.is_empty())
        .map(|q| {
            json!({
                "executionId": format!("learn-validate-{}", q.id),
                "taskCategory": q.family,
                "outcome": "success",
                "contextIds": q.relevant.iter().map(|id| format!("memory:{id}")).collect::<Vec<_>>(),
            })
        })
        .collect();
    let (train_rows, val_rows) = split_dataset(&pool, 0.25, 7)?;
    let train_ids = ids_from_rows(&train_rows);
    let val_ids = ids_from_rows(&val_rows);
    let (weights, updates, misses) = top_miss_weights(&fixture, &train_ids);
    let mut dev_baseline = Acc::default();
    let mut dev_primary = Acc::default();
    let mut dev_lexical_primary = Acc::default();
    for question in fixture.questions.iter().filter(|q| val_ids.contains(&q.id)) {
        let base = lexical_rank(&fixture, &question.prompt);
        let learned = query_rank(&fixture, &weights, &question.prompt);
        let guarded = lexical_primary_rank(&fixture, &weights, &question.prompt);
        dev_baseline.observe(LEARN_VALIDATE, None, &question.relevant, &base);
        dev_primary.observe(LEARN_VALIDATE, None, &question.relevant, &learned);
        dev_lexical_primary.observe(LEARN_VALIDATE, None, &question.relevant, &guarded);
    }
    let mut locked_baseline = Acc::default();
    let mut locked_primary = Acc::default();
    let mut locked_lexical_primary = Acc::default();
    for question in fixture
        .questions
        .iter()
        .filter(|q| locked_ids.contains(&q.id) && !q.relevant.is_empty())
    {
        let base = lexical_rank(&fixture, &question.prompt);
        let learned = query_rank(&fixture, &weights, &question.prompt);
        let guarded = lexical_primary_rank(&fixture, &weights, &question.prompt);
        locked_baseline.observe(LEARN_VALIDATE, None, &question.relevant, &base);
        locked_primary.observe(LEARN_VALIDATE, None, &question.relevant, &learned);
        locked_lexical_primary.observe(LEARN_VALIDATE, None, &question.relevant, &guarded);
    }
    Ok(json!({
        "kind": "measured",
        "promoted": false,
        "usesLiveHeldOutTasks": false,
        "trainedOnLockedFamilies": train_ids.iter().any(|id| locked_ids.contains(id)),
        "overlapsTrain": val_ids.iter().any(|id| train_ids.contains(id)),
        "method": "one-pass hinge against the lexical top hit when that hit is not relevant",
        "topMisses": misses,
        "pairUpdates": updates,
        "trainSize": train_ids.len(),
        "validationSize": val_ids.len(),
        "dev": {
            "baseline": dev_baseline.summary(SummaryStyle::LearnValidate),
            "topMissPrimary": dev_primary.summary(SummaryStyle::LearnValidate),
            "lexicalPrimary": dev_lexical_primary.summary(SummaryStyle::LearnValidate),
        },
        "locked": {
            "baseline": locked_baseline.summary(SummaryStyle::LearnValidate),
            "topMissPrimary": locked_primary.summary(SummaryStyle::LearnValidate),
            "lexicalPrimary": locked_lexical_primary.summary(SummaryStyle::LearnValidate),
        },
        "note": "The hard negative is fixed as the lexical top miss. The rule is not changed after scoring."
    }))
}

/// Trains on unlocked questions only and scores locked families once.
/// Same 0.8 floor and same displacement. Locked labels are not training rows.
pub fn measure_locked_confirmation(fixture_text: &str) -> Result<Value, String> {
    let fixture = load_coding_fixture(fixture_text)?;
    let unlocked = answered(&fixture, false);
    let locked = answered(&fixture, true);
    let training = observations_of(&unlocked);
    let minimum_confidence = 0.8;
    let mut predictions = 0usize;
    let mut relevant_outside_lexical_top3 = 0usize;
    let mut irrelevant_outside_lexical_top3 = 0usize;
    let mut baseline_mrr = 0.0;
    let mut with_addition_mrr = 0.0;
    for question in &locked {
        let predicted = crate::predict::predict(&training, &question.family, minimum_confidence);
        let lexical = lexical_rank(&fixture, &question.prompt);
        let top: BTreeSet<&str> = lexical.iter().take(3).map(String::as_str).collect();
        let mut ranked = lexical.clone();
        for item in predicted.iter().filter(|item| item.kind == "memory") {
            predictions += 1;
            let hit = question.relevant.iter().any(|id| id == &item.target_id);
            if hit && !top.contains(item.target_id.as_str()) {
                relevant_outside_lexical_top3 += 1;
                if let Some(position) = ranked.iter().position(|id| id == &item.target_id) {
                    move_to_third(&mut ranked, position, item.target_id.clone());
                }
            } else if !hit && !top.contains(item.target_id.as_str()) {
                irrelevant_outside_lexical_top3 += 1;
            }
        }
        let relevant = question.relevant.clone();
        baseline_mrr += crate::metrics::mrr(&relevant, &lexical).unwrap_or(0.0);
        with_addition_mrr += crate::metrics::mrr(&relevant, &ranked).unwrap_or(0.0);
    }
    let questions = locked.len() as f64;
    Ok(json!({
        "kind": "measured",
        "promoted": false,
        "usesLiveHeldOutTasks": false,
        "trainedOnLockedFamilies": false,
        "minimumConfidence": minimum_confidence,
        "questions": locked.len(),
        "predictions": predictions,
        "relevantOutsideLexicalTop3": relevant_outside_lexical_top3,
        "irrelevantOutsideLexicalTop3": irrelevant_outside_lexical_top3,
        "baselineMrr": if questions == 0.0 { Value::Null } else { json!(baseline_mrr / questions) },
        "mrrAfterDisplacingThird": if questions == 0.0 { Value::Null } else { json!(with_addition_mrr / questions) },
        "note": "Confirmation only. The rule was not changed after this fold was scored."
    }))
}

/// One neighbor of the lexical top hit, linked by a shared file or trigger.
/// The graph is memory metadata only. Question labels are not used to build it.
/// At most one neighbor is moved to position 3. Nothing is placed above the top hit.
pub fn measure_relationship_expansion(fixture_text: &str) -> Result<Value, String> {
    let fixture = load_coding_fixture(fixture_text)?;
    let edges = relationship_edges(&fixture);
    let locked = answered(&fixture, true);
    let mut baseline_mrr = 0.0;
    let mut expanded_mrr = 0.0;
    let mut moved = 0usize;
    let mut moved_relevant = 0usize;
    for question in &locked {
        let lexical = lexical_rank(&fixture, &question.prompt);
        let mut ranked = lexical.clone();
        if let Some(top) = lexical.first() {
            let top3: BTreeSet<&str> = lexical.iter().take(3).map(String::as_str).collect();
            if let Some(neighbor) = edges
                .get(top)
                .and_then(|neighbors| neighbors.iter().find(|id| !top3.contains(id.as_str())))
            {
                if let Some(position) = ranked.iter().position(|id| id == neighbor) {
                    moved += 1;
                    if question.relevant.iter().any(|id| id == neighbor) {
                        moved_relevant += 1;
                    }
                    move_to_third(&mut ranked, position, neighbor.clone());
                }
            }
        }
        let relevant = question.relevant.clone();
        baseline_mrr += crate::metrics::mrr(&relevant, &lexical).unwrap_or(0.0);
        expanded_mrr += crate::metrics::mrr(&relevant, &ranked).unwrap_or(0.0);
    }
    let questions = locked.len() as f64;
    Ok(json!({
        "kind": "measured",
        "promoted": false,
        "usesLiveHeldOutTasks": false,
        "usesQuestionLabelsToBuildGraph": false,
        "questions": locked.len(),
        "edges": edges.values().map(|neighbors| neighbors.len()).sum::<usize>(),
        "moved": moved,
        "movedRelevant": moved_relevant,
        "baselineMrr": if questions == 0.0 { Value::Null } else { json!(baseline_mrr / questions) },
        "expandedMrr": if questions == 0.0 { Value::Null } else { json!(expanded_mrr / questions) },
        "note": "One-shot relationship rule. Not adjusted after the locked families were scored."
    }))
}

fn relationship_edges(fixture: &CodingFixture) -> BTreeMap<String, Vec<String>> {
    let memories: Vec<_> = fixture
        .memories
        .iter()
        .filter(|memory| memory.insert)
        .collect();
    let mut edges: BTreeMap<String, Vec<(usize, usize, String)>> = BTreeMap::new();
    for (left_index, left) in memories.iter().enumerate() {
        for right in memories.iter().skip(left_index + 1) {
            let shared_files = left
                .files
                .iter()
                .filter(|file| right.files.contains(file))
                .count();
            let left_triggers = trigger_tokens(&left.triggers);
            let right_triggers = trigger_tokens(&right.triggers);
            let shared_triggers = left_triggers.intersection(&right_triggers).count();
            if shared_files == 0 && shared_triggers == 0 {
                continue;
            }
            edges.entry(left.key.clone()).or_default().push((
                shared_files,
                shared_triggers,
                right.key.clone(),
            ));
            edges.entry(right.key.clone()).or_default().push((
                shared_files,
                shared_triggers,
                left.key.clone(),
            ));
        }
    }
    edges
        .into_iter()
        .map(|(key, mut neighbors)| {
            neighbors.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
            (key, neighbors.into_iter().map(|(_, _, id)| id).collect())
        })
        .collect()
}

fn trigger_tokens(triggers: &[String]) -> BTreeSet<String> {
    triggers
        .iter()
        .flat_map(|trigger| trigger.split(|c: char| !c.is_ascii_alphanumeric()))
        .filter(|token| token.len() > 2)
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::measure;

    #[test]
    fn query_prior_keeps_locked_questions_out_of_train_and_does_not_promote() {
        let report = measure(crate::CODING_FIXTURE).expect("fixture");
        assert_eq!(report["promoted"], serde_json::json!(false));
        assert_eq!(report["evalInTrain"], serde_json::json!(false));
        assert_eq!(report["usesLiveHeldOutTasks"], serde_json::json!(false));
        assert_eq!(report["semanticWeight"], serde_json::json!(0.0));
        assert!(report["trainSize"].as_u64().unwrap() >= 2);
        assert!(report["baseline"]["mrr"].is_number());
        assert!(report["globalFrequencyPrior"]["mrr"].is_number());
        assert!(report["queryPrior"]["mrr"].is_number());
        let baseline = report["baseline"]["mrr"].as_f64().unwrap();
        let frequency = report["globalFrequencyPrior"]["mrr"].as_f64().unwrap();
        let query = report["queryPrior"]["mrr"].as_f64().unwrap();
        assert!((baseline - 0.7142857142857143).abs() < 1e-12);
        assert!((frequency - 0.5).abs() < 1e-12);
        assert!((query - 0.7).abs() < 1e-9);
        assert!(query < baseline);
        assert!(frequency < baseline);
    }

    #[test]
    fn dev_split_stays_out_of_train_and_the_locked_fold() {
        let report = super::measure_dev_split(crate::CODING_FIXTURE).expect("fixture");
        assert_eq!(report["promoted"], serde_json::json!(false));
        assert_eq!(report["usesLiveHeldOutTasks"], serde_json::json!(false));
        assert_eq!(report["overlapsLockedFold"], serde_json::json!(false));
        assert_eq!(report["overlapsTrain"], serde_json::json!(false));
        assert_eq!(report["validationSize"], serde_json::json!(3));
        let baseline = report["baseline"]["mrr"].as_f64().unwrap();
        let learned = report["learnedPrimary"]["mrr"].as_f64().unwrap();
        let guarded = report["lexicalPrimary"]["mrr"].as_f64().unwrap();
        assert!((baseline - 1.0).abs() < 1e-12);
        assert!((learned - 0.75).abs() < 1e-12);
        assert!((guarded - 1.0).abs() < 1e-12);
        assert!(learned < baseline);
        assert!((guarded - baseline).abs() < 1e-12);
    }

    #[test]
    fn top_miss_hinge_trains_outside_the_locked_fold() {
        let report = super::measure_top_miss(crate::CODING_FIXTURE).expect("fixture");
        assert_eq!(report["promoted"], serde_json::json!(false));
        assert_eq!(report["usesLiveHeldOutTasks"], serde_json::json!(false));
        assert_eq!(report["trainedOnLockedFamilies"], serde_json::json!(false));
        assert_eq!(report["overlapsTrain"], serde_json::json!(false));
        assert_eq!(report["validationSize"], serde_json::json!(3));
        assert_eq!(report["trainSize"], serde_json::json!(9));
        assert_eq!(report["topMisses"], serde_json::json!(5));
        assert_eq!(report["pairUpdates"], serde_json::json!(5));
        let dev_base = report["dev"]["baseline"]["mrr"].as_f64().unwrap();
        let dev_primary = report["dev"]["topMissPrimary"]["mrr"].as_f64().unwrap();
        let dev_lex = report["dev"]["lexicalPrimary"]["mrr"].as_f64().unwrap();
        let locked_base = report["locked"]["baseline"]["mrr"].as_f64().unwrap();
        let locked_primary = report["locked"]["topMissPrimary"]["mrr"].as_f64().unwrap();
        let locked_lex = report["locked"]["lexicalPrimary"]["mrr"].as_f64().unwrap();
        assert!((dev_base - 1.0).abs() < 1e-12);
        assert!((dev_primary - dev_base).abs() < 1e-12);
        assert!((dev_lex - dev_base).abs() < 1e-12);
        assert!((locked_base - 0.7142857142857143).abs() < 1e-12);
        assert!((locked_primary - locked_base).abs() < 1e-12);
        assert!((locked_lex - locked_base).abs() < 1e-12);
    }

    #[test]
    fn pairwise_hinge_trains_outside_the_locked_fold() {
        let report = super::measure_pairwise(crate::CODING_FIXTURE).expect("fixture");
        assert_eq!(report["promoted"], serde_json::json!(false));
        assert_eq!(report["usesLiveHeldOutTasks"], serde_json::json!(false));
        assert_eq!(report["trainedOnLockedFamilies"], serde_json::json!(false));
        assert_eq!(report["overlapsTrain"], serde_json::json!(false));
        assert_eq!(report["validationSize"], serde_json::json!(3));
        assert_eq!(report["trainSize"], serde_json::json!(9));
        assert_eq!(report["pairUpdates"], serde_json::json!(4));
        let dev_base = report["dev"]["baseline"]["mrr"].as_f64().unwrap();
        let dev_pair = report["dev"]["pairwisePrimary"]["mrr"].as_f64().unwrap();
        let dev_lex = report["dev"]["lexicalPrimary"]["mrr"].as_f64().unwrap();
        let locked_base = report["locked"]["baseline"]["mrr"].as_f64().unwrap();
        let locked_pair = report["locked"]["pairwisePrimary"]["mrr"].as_f64().unwrap();
        let locked_lex = report["locked"]["lexicalPrimary"]["mrr"].as_f64().unwrap();
        assert!((dev_base - 1.0).abs() < 1e-12);
        assert!((dev_pair - 0.8333333333333334).abs() < 1e-12);
        assert!((dev_lex - 1.0).abs() < 1e-12);
        assert!((locked_base - 0.7142857142857143).abs() < 1e-12);
        assert!((locked_pair - 0.6923076923076922).abs() < 1e-12);
        assert!((locked_lex - 0.6969696969696969).abs() < 1e-12);
        assert!(dev_pair < dev_base);
        assert!(locked_pair < locked_base);
        assert!(locked_lex < locked_base);
    }

    #[test]
    fn benefit_gate_rejects_uncalibrated_predictions_on_the_unlocked_fixture() {
        let report = super::measure_benefit_gate(crate::CODING_FIXTURE).expect("fixture");
        assert_eq!(report["promoted"], serde_json::json!(false));
        assert_eq!(report["usesLiveHeldOutTasks"], serde_json::json!(false));
        assert_eq!(report["usesLockedFamilies"], serde_json::json!(false));
        assert_eq!(report["questions"], serde_json::json!(12));
        assert_eq!(report["minimumConfidence"], serde_json::json!(0.8));
        assert_eq!(report["uncalibratedAccepted"], serde_json::json!(0));
        assert_eq!(report["memoryPredictions"], serde_json::json!(5));
        assert_eq!(report["gatedAccepted"], serde_json::json!(0));
        assert_eq!(report["gatedRejectedEvidence"], serde_json::json!(5));
        assert_eq!(report["gatedRejectedBenefit"], serde_json::json!(0));
        let baseline = report["baselineMrr"].as_f64().unwrap();
        let gated = report["gatedMrr"].as_f64().unwrap();
        assert!((baseline - 0.6257936507936509).abs() < 1e-12);
        assert!((gated - baseline).abs() < 1e-12);
    }

    #[test]
    fn prediction_additions_at_the_existing_gate_do_not_use_locked_questions() {
        let report = super::measure_prediction_additions(crate::CODING_FIXTURE).expect("fixture");
        assert_eq!(report["promoted"], serde_json::json!(false));
        assert_eq!(report["usesLiveHeldOutTasks"], serde_json::json!(false));
        assert_eq!(report["usesLockedFamilies"], serde_json::json!(false));
        assert_eq!(report["minimumConfidence"], serde_json::json!(0.8));
        assert_eq!(report["questions"], serde_json::json!(12));
        assert_eq!(report["predictions"], serde_json::json!(5));
        assert_eq!(report["relevantInsideLexicalTop3"], serde_json::json!(4));
        assert_eq!(report["relevantOutsideLexicalTop3"], serde_json::json!(1));
        assert_eq!(report["irrelevantOutsideLexicalTop3"], serde_json::json!(0));
        assert_eq!(report["additionLexicalRanks"], serde_json::json!([6]));
        let baseline = report["baselineMrr"].as_f64().unwrap();
        let after = report["mrrAfterDisplacingThird"].as_f64().unwrap();
        assert!((baseline - 0.6257936507936509).abs() < 1e-12);
        assert!((after - 0.6396825396825397).abs() < 1e-12);
        assert!(after > baseline);
    }

    #[test]
    fn locked_confirmation_trains_only_on_unlocked_questions() {
        let report = super::measure_locked_confirmation(crate::CODING_FIXTURE).expect("fixture");
        assert_eq!(report["promoted"], serde_json::json!(false));
        assert_eq!(report["trainedOnLockedFamilies"], serde_json::json!(false));
        assert_eq!(report["usesLiveHeldOutTasks"], serde_json::json!(false));
        assert_eq!(report["minimumConfidence"], serde_json::json!(0.8));
        assert_eq!(report["questions"], serde_json::json!(3));
        assert_eq!(report["predictions"], serde_json::json!(0));
        assert_eq!(report["relevantOutsideLexicalTop3"], serde_json::json!(0));
        let baseline = report["baselineMrr"].as_f64().unwrap();
        let after = report["mrrAfterDisplacingThird"].as_f64().unwrap();
        assert!((baseline - 0.7142857142857143).abs() < 1e-12);
        assert!((after - baseline).abs() < 1e-12);
    }

    #[test]
    fn relationship_expansion_does_not_use_question_labels() {
        let report = super::measure_relationship_expansion(crate::CODING_FIXTURE).expect("fixture");
        assert_eq!(report["promoted"], serde_json::json!(false));
        assert_eq!(
            report["usesQuestionLabelsToBuildGraph"],
            serde_json::json!(false)
        );
        assert_eq!(report["usesLiveHeldOutTasks"], serde_json::json!(false));
        assert_eq!(report["questions"], serde_json::json!(3));
        assert_eq!(report["edges"], serde_json::json!(2));
        assert_eq!(report["moved"], serde_json::json!(0));
        assert_eq!(report["movedRelevant"], serde_json::json!(0));
        let baseline = report["baselineMrr"].as_f64().unwrap();
        let expanded = report["expandedMrr"].as_f64().unwrap();
        assert!((baseline - 0.7142857142857143).abs() < 1e-12);
        assert!((expanded - baseline).abs() < 1e-12);
    }
}
