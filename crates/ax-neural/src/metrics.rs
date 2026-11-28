//! Shared retrieval metric accumulation for neural harnesses (internal only).

use ax_memory::bench::{ndcg_at, recall_at};
use serde_json::{json, Value};

/// Precision@k: relevant hits in the top k, divided by k. A ranker that returns fewer than
/// k items is scored as if the missing slots were misses. None when there is no relevant set.
pub fn precision_at(relevant: &[String], ranked: &[String], k: usize) -> Option<f64> {
    if relevant.is_empty() {
        return None;
    }
    if k == 0 {
        return None;
    }
    let want: std::collections::BTreeSet<&str> = relevant.iter().map(String::as_str).collect();
    let hits = ranked
        .iter()
        .take(k)
        .filter(|id| want.contains(id.as_str()))
        .count();
    Some(hits as f64 / k as f64)
}

/// Mean reciprocal rank of the first relevant hit. None when there is no relevant set.
pub fn mrr(relevant: &[String], ranked: &[String]) -> Option<f64> {
    if relevant.is_empty() {
        return None;
    }
    let want: std::collections::BTreeSet<&str> = relevant.iter().map(String::as_str).collect();
    for (i, id) in ranked.iter().enumerate() {
        if want.contains(id.as_str()) {
            return Some(1.0 / (i as f64 + 1.0));
        }
    }
    Some(0.0)
}

pub(crate) fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

pub(crate) fn push_opt(bucket: &mut Vec<f64>, value: Option<f64>) {
    if let Some(v) = value {
        bucket.push(v);
    }
}

/// Which per-question metrics a harness records into [`Acc`].
#[derive(Clone, Copy)]
pub(crate) struct ObserveProfile {
    pub precision_at_5: bool,
    pub recall_at_5: bool,
    pub ndcg_at_10: bool,
    pub paraphrase_recall: bool,
}

pub(crate) const EMBED_BAKEOFF: ObserveProfile = ObserveProfile {
    precision_at_5: false,
    recall_at_5: false,
    ndcg_at_10: false,
    paraphrase_recall: true,
};

pub(crate) const PATH_VALIDATE: ObserveProfile = ObserveProfile {
    precision_at_5: true,
    recall_at_5: false,
    ndcg_at_10: false,
    paraphrase_recall: false,
};

#[cfg(any(test, feature = "neural-lab"))]
pub(crate) const LEARN_VALIDATE: ObserveProfile = ObserveProfile {
    precision_at_5: false,
    recall_at_5: false,
    ndcg_at_10: false,
    paraphrase_recall: false,
};

pub(crate) const AGENT_EVAL: ObserveProfile = ObserveProfile {
    precision_at_5: false,
    recall_at_5: false,
    ndcg_at_10: false,
    paraphrase_recall: false,
};

pub(crate) const BENCHMARK_ABC: ObserveProfile = ObserveProfile {
    precision_at_5: true,
    recall_at_5: true,
    ndcg_at_10: true,
    paraphrase_recall: false,
};

#[derive(Clone, Copy)]
pub(crate) enum SummaryStyle {
    EmbedBakeoff,
    PathValidate,
    #[cfg(any(test, feature = "neural-lab"))]
    LearnValidate,
    AgentEval,
    BenchmarkAbc,
}

#[derive(Default)]
pub(crate) struct Acc {
    p3: Vec<f64>,
    p5: Vec<f64>,
    r3: Vec<f64>,
    r5: Vec<f64>,
    mrr: Vec<f64>,
    ndcg5: Vec<f64>,
    ndcg10: Vec<f64>,
    paraphrase_r3: Vec<f64>,
    scored: u64,
}

impl Acc {
    /// Mean recall@3 over every paraphrase question; None when none was scored.
    pub(crate) fn paraphrase_recall_at_3(&self) -> Option<f64> {
        mean(&self.paraphrase_r3)
    }

    pub(crate) fn observe(
        &mut self,
        profile: ObserveProfile,
        family: Option<&str>,
        relevant: &[String],
        ranked: &[String],
    ) {
        self.scored += 1;
        if relevant.is_empty() {
            return;
        }
        push_opt(&mut self.p3, precision_at(relevant, ranked, 3));
        if profile.precision_at_5 {
            push_opt(&mut self.p5, precision_at(relevant, ranked, 5));
        }
        push_opt(&mut self.r3, recall_at(relevant, ranked, 3));
        if profile.recall_at_5 {
            push_opt(&mut self.r5, recall_at(relevant, ranked, 5));
        }
        push_opt(&mut self.mrr, mrr(relevant, ranked));
        self.ndcg5.push(ndcg_at(relevant, ranked, 5));
        if profile.ndcg_at_10 {
            self.ndcg10.push(ndcg_at(relevant, ranked, 10));
        }
        if profile.paraphrase_recall && family == Some("paraphrase") {
            push_opt(&mut self.paraphrase_r3, recall_at(relevant, ranked, 3));
        }
    }

    pub(crate) fn summary(&self, style: SummaryStyle) -> Value {
        match style {
            SummaryStyle::EmbedBakeoff => json!({
                "questionsScored": self.scored,
                "precisionAt3": mean(&self.p3),
                "recallAt3": mean(&self.r3),
                "mrr": mean(&self.mrr),
                "ndcgAt5": mean(&self.ndcg5),
                "paraphraseRecallAt3": self.paraphrase_recall_at_3()
            }),
            SummaryStyle::PathValidate => json!({
                "questionsScored": self.scored,
                "precisionAt3": mean(&self.p3),
                "precisionAt5": mean(&self.p5),
                "recallAt3": mean(&self.r3),
                "mrr": mean(&self.mrr),
                "ndcgAt5": mean(&self.ndcg5),
                "samples": {
                    "precisionAt3": self.p3.len(),
                    "recallAt3": self.r3.len(),
                    "mrr": self.mrr.len()
                }
            }),
            #[cfg(any(test, feature = "neural-lab"))]
            SummaryStyle::LearnValidate => json!({
                "questionsScored": self.scored,
                "precisionAt3": mean(&self.p3),
                "recallAt3": mean(&self.r3),
                "mrr": mean(&self.mrr),
                "ndcgAt5": mean(&self.ndcg5),
                "samples": {
                    "precisionAt3": self.p3.len(),
                    "recallAt3": self.r3.len(),
                    "mrr": self.mrr.len()
                }
            }),
            SummaryStyle::AgentEval => {
                let mean_json = |v: &[f64]| {
                    if v.is_empty() {
                        Value::Null
                    } else {
                        json!(v.iter().sum::<f64>() / v.len() as f64)
                    }
                };
                json!({
                    "questionsScored": self.scored,
                    "contextPrecisionAt3": mean_json(&self.p3),
                    "contextRecallAt3": mean_json(&self.r3),
                    "mrr": mean_json(&self.mrr),
                    "ndcgAt5": mean_json(&self.ndcg5)
                })
            }
            SummaryStyle::BenchmarkAbc => json!({
                "questionsScored": self.scored,
                "precisionAt3": mean(&self.p3),
                "precisionAt5": mean(&self.p5),
                "recallAt3": mean(&self.r3),
                "recallAt5": mean(&self.r5),
                "mrr": mean(&self.mrr),
                "ndcgAt5": mean(&self.ndcg5),
                "ndcgAt10": mean(&self.ndcg10),
                "samples": {
                    "precisionAt3": self.p3.len(),
                    "recallAt3": self.r3.len(),
                    "mrr": self.mrr.len()
                }
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precision_and_mrr_examples() {
        let relevant = vec!["m1".into()];
        assert_eq!(
            precision_at(&relevant, &["m1".into(), "x".into(), "y".into()], 3),
            Some(1.0 / 3.0)
        );
        assert_eq!(
            precision_at(&relevant, &["x".into(), "m1".into()], 2),
            Some(0.5)
        );
        assert_eq!(mrr(&relevant, &["x".into(), "m1".into()]), Some(0.5));
        assert_eq!(mrr(&relevant, &["m1".into()]), Some(1.0));
        assert_eq!(mrr(&[], &["m1".into()]), None);
        assert_eq!(precision_at(&[], &["m1".into()], 3), None);
    }

    #[test]
    fn precision_divides_by_k_even_for_short_lists() {
        let relevant = vec!["m1".into()];
        // A short list does not earn a higher score than a full list with the same hit.
        assert_eq!(precision_at(&relevant, &["m1".into()], 3), Some(1.0 / 3.0));
        assert_eq!(
            precision_at(&relevant, &["m1".into(), "x".into(), "y".into()], 3),
            precision_at(&relevant, &["m1".into()], 3)
        );
        assert_eq!(precision_at(&relevant, &[], 3), Some(0.0));
    }

    #[test]
    fn paraphrase_recall_is_the_mean_over_all_paraphrase_questions() {
        let mut acc = Acc::default();
        let miss = vec!["m1".to_string()];
        let hit = vec!["m2".to_string()];
        let ranked = vec!["m2".to_string(), "x".to_string(), "y".to_string()];
        acc.observe(EMBED_BAKEOFF, Some("paraphrase"), &miss, &ranked);
        acc.observe(EMBED_BAKEOFF, Some("paraphrase"), &hit, &ranked);
        acc.observe(EMBED_BAKEOFF, Some("exact"), &hit, &ranked);
        assert_eq!(acc.paraphrase_recall_at_3(), Some(0.5));
        assert_eq!(
            acc.summary(SummaryStyle::EmbedBakeoff)["paraphraseRecallAt3"],
            json!(0.5)
        );
    }
}
