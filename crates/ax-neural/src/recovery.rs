//! Experimental context selection. Production ranking does not call this module.
//! `RecoveryConfig::default().enabled` is false, and a disabled call returns the baseline.

use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq)]
pub struct RecoveryConfig {
    pub enabled: bool,
    pub minimum_confidence: f64,
    pub minimum_evidence: u32,
    pub token_budget: u32,
    pub model_version: String,
    pub feedback_ids: Vec<String>,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            minimum_confidence: 0.8,
            minimum_evidence: 3,
            token_budget: 0,
            model_version: String::new(),
            feedback_ids: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ContextCandidate {
    pub id: String,
    pub source: &'static str,
    pub baseline_score: f64,
    pub neural_score: Option<f64>,
    pub mandatory: bool,
    pub protected: bool,
    pub tokens: u32,
    pub project_id: String,
}

#[derive(Clone, Debug)]
pub struct PredictionProposal {
    pub id: String,
    pub confidence: f64,
    pub evidence_count: u32,
    pub calibrated_on_independent_validation: bool,
    pub tokens: u32,
    pub project_id: String,
    pub authorized: bool,
}

#[derive(Clone, Debug)]
pub struct Selection {
    pub selected: Vec<String>,
    pub used_baseline: bool,
    pub fallback_reason: Option<String>,
    pub displaced: Vec<String>,
    pub trace: Vec<Value>,
}

pub fn select_context(
    config: &RecoveryConfig,
    baseline: &[ContextCandidate],
    neural_optional: &[ContextCandidate],
    predictions: &[PredictionProposal],
    allowed_project: &str,
) -> Selection {
    let baseline_ids: Vec<String> = baseline.iter().map(|c| c.id.clone()).collect();
    if !config.enabled {
        return stamp(
            baseline_selection(baseline, "experimental recovery disabled"),
            config,
        );
    }
    if let Some(reason) = safety_breach(baseline, neural_optional, allowed_project) {
        let mut selection = baseline_selection(baseline, &reason);
        selection.fallback_reason = Some(reason);
        return stamp(selection, config);
    }

    let budget = if config.token_budget == 0 {
        baseline.iter().map(|c| c.tokens).sum()
    } else {
        config.token_budget
    };

    let mut chosen: Vec<ContextCandidate> = baseline.to_vec();
    rerank_optional_slots(&mut chosen);
    let mut displaced = Vec::new();
    let mut trace = Vec::new();

    for extra in neural_optional {
        if extra.id.is_empty() || chosen.iter().any(|c| c.id == extra.id) {
            trace.push(decision(
                &extra.id,
                "neural",
                "rejected",
                "already selected or empty",
            ));
            continue;
        }
        if extra.project_id != allowed_project {
            trace.push(decision(
                &extra.id,
                "neural",
                "rejected",
                "project isolation",
            ));
            continue;
        }
        if extra.neural_score.is_none() {
            trace.push(decision(
                &extra.id,
                "neural",
                "rejected",
                "missing neural score",
            ));
            continue;
        }
        match fit_optional(&mut chosen, extra.clone(), budget, &mut displaced) {
            Fit::Added => trace.push(decision(
                &extra.id,
                "neural",
                "accepted",
                "optional add within budget",
            )),
            Fit::NoRoom(reason) => trace.push(decision(&extra.id, "neural", "rejected", reason)),
        }
    }

    for proposal in predictions {
        match prediction_decision(config, proposal, &chosen, allowed_project) {
            Pred::Reject(reason) => {
                trace.push(decision(&proposal.id, "prediction", "rejected", reason));
            }
            Pred::Accept => {
                let candidate = ContextCandidate {
                    id: proposal.id.clone(),
                    source: "prediction",
                    baseline_score: 0.0,
                    neural_score: Some(proposal.confidence),
                    mandatory: false,
                    protected: false,
                    tokens: proposal.tokens,
                    project_id: proposal.project_id.clone(),
                };
                match fit_optional(&mut chosen, candidate, budget, &mut displaced) {
                    Fit::Added => trace.push(decision(
                        &proposal.id,
                        "prediction",
                        "accepted",
                        "calibrated, novel, authorized, score beats displaced items",
                    )),
                    Fit::NoRoom(reason) => {
                        trace.push(decision(&proposal.id, "prediction", "rejected", reason))
                    }
                }
            }
        }
    }

    for (index, item) in chosen.iter().enumerate() {
        trace.push(json!({
            "id": item.id,
            "source": item.source,
            "baselineScore": item.baseline_score,
            "neuralScore": item.neural_score,
            "position": index,
            "tokens": item.tokens,
            "mandatory": item.mandatory,
            "protected": item.protected,
            "reason": if item.mandatory || item.protected { "kept" } else { "selected" },
        }));
    }

    let selected: Vec<String> = chosen.iter().map(|c| c.id.clone()).collect();
    let changed = selected != baseline_ids || !displaced.is_empty();
    stamp(
        Selection {
            used_baseline: !changed,
            fallback_reason: None,
            selected,
            displaced,
            trace,
        },
        config,
    )
}

fn stamp(mut selection: Selection, config: &RecoveryConfig) -> Selection {
    selection.trace.insert(
        0,
        json!({
            "event": "attribution",
            "modelVersion": config.model_version,
            "feedbackIds": config.feedback_ids,
        }),
    );
    selection
}

fn baseline_selection(baseline: &[ContextCandidate], reason: &str) -> Selection {
    let mut trace = vec![json!({"event": "fallback", "reason": reason})];
    for (index, item) in baseline.iter().enumerate() {
        trace.push(json!({
            "id": item.id,
            "source": "baseline",
            "baselineScore": item.baseline_score,
            "position": index,
            "tokens": item.tokens,
            "mandatory": item.mandatory,
            "reason": "baseline",
        }));
    }
    Selection {
        selected: baseline.iter().map(|c| c.id.clone()).collect(),
        used_baseline: true,
        fallback_reason: Some(reason.to_string()),
        displaced: Vec::new(),
        trace,
    }
}

fn safety_breach(
    baseline: &[ContextCandidate],
    neural_optional: &[ContextCandidate],
    allowed_project: &str,
) -> Option<String> {
    if baseline
        .iter()
        .any(|c| (c.mandatory || c.protected) && c.project_id != allowed_project)
    {
        return Some("baseline protected context failed project isolation".into());
    }
    if neural_optional
        .iter()
        .any(|c| c.protected && c.project_id != allowed_project)
    {
        return Some("neural proposal carried protected context from another project".into());
    }
    None
}

fn rerank_optional_slots(chosen: &mut [ContextCandidate]) {
    let mut optional: Vec<(usize, ContextCandidate)> = chosen
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.mandatory && !c.protected && c.neural_score.is_some())
        .map(|(i, c)| (i, c.clone()))
        .collect();
    if optional.len() < 2 {
        return;
    }
    let mut slots: Vec<usize> = optional.iter().map(|(index, _)| *index).collect();
    slots.sort_unstable();
    optional.sort_by(|a, b| {
        b.1.neural_score
            .unwrap_or(0.0)
            .partial_cmp(&a.1.neural_score.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    for (slot, candidate) in slots
        .into_iter()
        .zip(optional.into_iter().map(|(_, item)| item))
    {
        chosen[slot] = candidate;
    }
}

enum Fit {
    Added,
    NoRoom(&'static str),
}

fn candidate_score(candidate: &ContextCandidate) -> f64 {
    candidate.neural_score.unwrap_or(candidate.baseline_score)
}

fn fit_optional(
    chosen: &mut Vec<ContextCandidate>,
    extra: ContextCandidate,
    budget: u32,
    displaced: &mut Vec<String>,
) -> Fit {
    let extra_id = extra.id.clone();
    let extra_score = candidate_score(&extra);
    let mut working = chosen.clone();
    let mut pending = Vec::new();
    working.push(extra);
    while token_sum(&working) > budget {
        let victim = working.iter().enumerate().rev().find(|(_, c)| {
            c.id != extra_id && !c.mandatory && !c.protected && candidate_score(c) < extra_score
        });
        let Some((index, _)) = victim else {
            let stronger_optional = working.iter().any(|c| {
                c.id != extra_id
                    && !c.mandatory
                    && !c.protected
                    && candidate_score(c) >= extra_score
            });
            return if stronger_optional {
                Fit::NoRoom("expected benefit does not justify displacement")
            } else {
                Fit::NoRoom("budget would displace protected context")
            };
        };
        let removed = working.remove(index);
        pending.push(removed.id);
    }
    *chosen = working;
    displaced.extend(pending);
    Fit::Added
}

fn token_sum(items: &[ContextCandidate]) -> u32 {
    items.iter().map(|c| c.tokens).sum()
}

enum Pred<'a> {
    Accept,
    Reject(&'a str),
}

fn prediction_decision<'a>(
    config: &RecoveryConfig,
    proposal: &'a PredictionProposal,
    chosen: &[ContextCandidate],
    allowed_project: &str,
) -> Pred<'a> {
    if !proposal.authorized || proposal.project_id != allowed_project {
        return Pred::Reject("unauthorized or other project");
    }
    if !proposal.calibrated_on_independent_validation {
        return Pred::Reject("confidence is not calibrated on independent validation");
    }
    if proposal.evidence_count < config.minimum_evidence {
        return Pred::Reject("insufficient supporting evidence");
    }
    if proposal.confidence < config.minimum_confidence {
        return Pred::Reject("confidence below gate");
    }
    if chosen.iter().any(|c| c.id == proposal.id) {
        return Pred::Reject("already selected");
    }
    Pred::Accept
}

fn decision(id: &str, source: &str, outcome: &str, reason: &str) -> Value {
    json!({
        "id": id,
        "source": source,
        "outcome": outcome,
        "reason": reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, score: f64, mandatory: bool, tokens: u32) -> ContextCandidate {
        ContextCandidate {
            id: id.into(),
            source: "baseline",
            baseline_score: score,
            neural_score: None,
            mandatory,
            protected: mandatory,
            tokens,
            project_id: "ax".into(),
        }
    }

    #[test]
    fn attribution_header_records_model_and_feedback_without_changing_selection() {
        let baseline = vec![item("rule", 1.0, true, 10), item("opt", 0.2, false, 5)];
        let config = RecoveryConfig {
            model_version: "recovery-experimental".into(),
            feedback_ids: vec!["fb-1".into()],
            ..RecoveryConfig::default()
        };
        let selection = select_context(&config, &baseline, &[], &[], "ax");
        assert!(selection.used_baseline);
        assert_eq!(
            selection.selected,
            vec!["rule".to_string(), "opt".to_string()]
        );
        let header = &selection.trace[0];
        assert_eq!(header["event"], "attribution");
        assert_eq!(header["modelVersion"], "recovery-experimental");
        assert_eq!(header["feedbackIds"][0], "fb-1");
    }

    #[test]
    fn disabled_returns_the_baseline_unchanged() {
        let baseline = vec![item("rule", 1.0, true, 10), item("opt", 0.2, false, 5)];
        let selection = select_context(&RecoveryConfig::default(), &baseline, &[], &[], "ax");
        assert!(selection.used_baseline);
        assert_eq!(
            selection.selected,
            vec!["rule".to_string(), "opt".to_string()]
        );
        assert!(selection.displaced.is_empty());
        assert_eq!(
            selection.fallback_reason.as_deref(),
            Some("experimental recovery disabled")
        );
    }

    #[test]
    fn mandatory_context_stays_when_optional_items_are_reranked() {
        let rule = item("rule", 1.0, true, 4);
        let mut high = item("high", 0.8, false, 4);
        high.neural_score = Some(0.2);
        high.protected = false;
        let mut low = item("low", 0.1, false, 4);
        low.neural_score = Some(0.9);
        low.protected = false;
        let config = RecoveryConfig {
            enabled: true,
            ..RecoveryConfig::default()
        };
        let selection = select_context(&config, &[rule, high, low], &[], &[], "ax");
        assert_eq!(
            selection.selected,
            vec!["rule".to_string(), "low".to_string(), "high".to_string()]
        );
    }

    #[test]
    fn cross_project_protected_proposal_falls_back_to_baseline() {
        let baseline = vec![item("rule", 1.0, true, 4)];
        let mut foreign = item("secret", 0.0, false, 4);
        foreign.source = "neural";
        foreign.protected = true;
        foreign.project_id = "other".into();
        let config = RecoveryConfig {
            enabled: true,
            ..RecoveryConfig::default()
        };
        let selection = select_context(&config, &baseline, &[foreign], &[], "ax");
        assert!(selection.used_baseline);
        assert_eq!(selection.selected, vec!["rule".to_string()]);
        assert!(selection
            .fallback_reason
            .unwrap()
            .contains("another project"));
    }

    #[test]
    fn uncalibrated_prediction_is_rejected_and_recorded() {
        let baseline = vec![item("rule", 1.0, true, 4), item("opt", 0.2, false, 4)];
        let proposal = PredictionProposal {
            id: "guess".into(),
            confidence: 0.99,
            evidence_count: 5,
            calibrated_on_independent_validation: false,
            tokens: 2,
            project_id: "ax".into(),
            authorized: true,
        };
        let config = RecoveryConfig {
            enabled: true,
            ..RecoveryConfig::default()
        };
        let selection = select_context(&config, &baseline, &[], &[proposal], "ax");
        assert!(!selection.selected.iter().any(|id| id == "guess"));
        assert!(selection
            .trace
            .iter()
            .any(|row| { row["id"] == "guess" && row["outcome"] == "rejected" }));
    }

    #[test]
    fn accepted_prediction_displaces_an_optional_baseline_item() {
        let mut optional = item("opt", 0.2, false, 5);
        optional.protected = false;
        let baseline = vec![item("rule", 1.0, true, 5), optional];
        let proposal = PredictionProposal {
            id: "extra".into(),
            confidence: 0.91,
            evidence_count: 4,
            calibrated_on_independent_validation: true,
            tokens: 5,
            project_id: "ax".into(),
            authorized: true,
        };
        let config = RecoveryConfig {
            enabled: true,
            ..RecoveryConfig::default()
        };
        let selection = select_context(&config, &baseline, &[], &[proposal], "ax");
        assert_eq!(
            selection.selected,
            vec!["rule".to_string(), "extra".to_string()]
        );
        assert_eq!(selection.displaced, vec!["opt".to_string()]);
        assert!(!selection.used_baseline);
    }

    fn scored(id: &str, neural: f64) -> ContextCandidate {
        let mut candidate = item(id, 0.0, false, 4);
        candidate.protected = false;
        candidate.neural_score = Some(neural);
        candidate
    }

    fn mean_mrr(questions: &[(&str, Vec<ContextCandidate>)], enabled: bool) -> f64 {
        let config = RecoveryConfig {
            enabled,
            ..RecoveryConfig::default()
        };
        let mut total = 0.0;
        for (relevant, baseline) in questions {
            let selection = select_context(&config, baseline, &[], &[], "ax");
            total += crate::metrics::mrr(&[(*relevant).to_string()], &selection.selected).unwrap();
        }
        total / questions.len() as f64
    }

    #[test]
    fn failed_add_does_not_record_a_displacement() {
        let mut optional = item("opt", 0.2, false, 4);
        optional.protected = false;
        let baseline = vec![item("rule", 1.0, true, 6), optional];
        let mut extra = item("wide", 0.0, false, 8);
        extra.source = "neural";
        extra.protected = false;
        extra.neural_score = Some(0.99);
        let config = RecoveryConfig {
            enabled: true,
            ..RecoveryConfig::default()
        };
        let selection = select_context(&config, &baseline, &[extra], &[], "ax");
        assert_eq!(
            selection.selected,
            vec!["rule".to_string(), "opt".to_string()]
        );
        assert!(selection.displaced.is_empty());
        assert!(selection
            .trace
            .iter()
            .any(|row| { row["id"] == "wide" && row["outcome"] == "rejected" }));
    }

    #[test]
    fn prediction_does_not_displace_an_equal_scored_optional_item() {
        let mut optional = item("opt", 0.91, false, 5);
        optional.protected = false;
        let baseline = vec![item("rule", 1.0, true, 5), optional];
        let proposal = PredictionProposal {
            id: "extra".into(),
            confidence: 0.91,
            evidence_count: 4,
            calibrated_on_independent_validation: true,
            tokens: 5,
            project_id: "ax".into(),
            authorized: true,
        };
        let config = RecoveryConfig {
            enabled: true,
            ..RecoveryConfig::default()
        };
        let selection = select_context(&config, &baseline, &[], &[proposal], "ax");
        assert_eq!(
            selection.selected,
            vec!["rule".to_string(), "opt".to_string()]
        );
        assert!(selection.displaced.is_empty());
        assert!(selection.trace.iter().any(|row| {
            row["id"] == "extra"
                && row["outcome"] == "rejected"
                && row["reason"] == "expected benefit does not justify displacement"
        }));
    }

    #[test]
    fn neural_candidate_without_a_score_is_rejected() {
        let mut optional = item("opt", 0.4, false, 4);
        optional.protected = false;
        let baseline = vec![item("rule", 1.0, true, 4), optional];
        let mut extra = item("wide", 0.0, false, 4);
        extra.source = "neural";
        extra.protected = false;
        let config = RecoveryConfig {
            enabled: true,
            ..RecoveryConfig::default()
        };
        let selection = select_context(&config, &baseline, &[extra], &[], "ax");
        assert_eq!(
            selection.selected,
            vec!["rule".to_string(), "opt".to_string()]
        );
        assert!(selection.displaced.is_empty());
        assert!(selection.trace.iter().any(|row| {
            row["id"] == "wide"
                && row["outcome"] == "rejected"
                && row["reason"] == "missing neural score"
        }));
    }

    #[test]
    fn independent_fixture_query_scores_raise_mrr_and_a_global_prior_does_not() {
        let rule = item("rule", 1.0, true, 4);
        let query_conditioned = [
            (
                "mem-a",
                vec![rule.clone(), scored("mem-b", 0.1), scored("mem-a", 0.9)],
            ),
            (
                "mem-b",
                vec![rule.clone(), scored("mem-b", 0.9), scored("mem-a", 0.1)],
            ),
        ];
        let global_prior = [
            (
                "mem-a",
                vec![rule.clone(), scored("mem-b", 0.1), scored("mem-a", 0.9)],
            ),
            (
                "mem-b",
                vec![rule.clone(), scored("mem-b", 0.1), scored("mem-a", 0.9)],
            ),
        ];
        let baseline_mrr = mean_mrr(&query_conditioned, false);
        let query_mrr = mean_mrr(&query_conditioned, true);
        let global_mrr = mean_mrr(&global_prior, true);
        assert!((baseline_mrr - (1.0 / 3.0 + 0.5) / 2.0).abs() < 1e-9);
        assert!((query_mrr - 0.5).abs() < 1e-9);
        assert!((global_mrr - baseline_mrr).abs() < 1e-9);
        assert!(query_mrr > baseline_mrr);
    }
}
