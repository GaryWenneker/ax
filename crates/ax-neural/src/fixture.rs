//! Pieces every coding-fixture harness shares: the locked question families,
//! the insertable documents, the lexical and cosine rankings, and the median.

use ax_memory::bench::{CodingFixture, CodingMemory};
use ax_memory::embed::cosine;

/// The question families whose results are locked across reports.
pub const LOCKED: &[&str] = &[
    "named_trigger",
    "paraphrase",
    "abstention",
    "delivery_budget",
];

/// True for a family in [`LOCKED`].
pub fn is_locked(family: &str) -> bool {
    LOCKED.contains(&family)
}

/// The fixture memories that are inserted, as lexical documents.
pub fn lexical_docs(fixture: &CodingFixture) -> Vec<crate::lexical::LexicalDoc> {
    fixture
        .memories
        .iter()
        .filter(|memory| memory.insert)
        .map(|memory| {
            crate::lexical::LexicalDoc::new(
                memory.key.clone(),
                &memory.title,
                &memory.body,
                &memory.triggers,
            )
        })
        .collect()
}

/// Keys ranked by the lexical challenger; empty when it abstains.
pub fn lexical_rank(fixture: &CodingFixture, prompt: &str) -> Vec<String> {
    let (ranked, abstained) = crate::lexical::rank(&lexical_docs(fixture), prompt);
    if abstained {
        Vec::new()
    } else {
        ranked.into_iter().map(|hit| hit.key).collect()
    }
}

/// The text a memory is embedded as.
pub fn doc_text(memory: &CodingMemory) -> String {
    format!(
        "{} {} {} {}",
        memory.key,
        memory.title,
        memory.body,
        memory.triggers.join(" ")
    )
}

/// Inserted memory keys by cosine similarity to `prompt`, ties broken by key.
pub fn cosine_rank(
    fixture: &CodingFixture,
    prompt: &str,
    embed: &dyn Fn(&str) -> Vec<f32>,
) -> Vec<String> {
    let query = embed(prompt);
    let mut scored: Vec<(f32, String)> = fixture
        .memories
        .iter()
        .filter(|memory| memory.insert)
        .map(|memory| {
            let vector = embed(&doc_text(memory));
            (cosine(&query, &vector), memory.key.clone())
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    scored.into_iter().map(|(_, key)| key).collect()
}

/// Locked-family questions as (id, family, prompt, relevant) for
/// `lexical::score_locked_families`.
pub fn locked_questions(fixture: &CodingFixture) -> Vec<(String, String, String, Vec<String>)> {
    fixture
        .questions
        .iter()
        .filter(|question| is_locked(&question.family))
        .map(|question| {
            (
                question.id.clone(),
                question.family.clone(),
                question.prompt.clone(),
                question.relevant.clone(),
            )
        })
        .collect()
}

/// The middle value, or the upper of the two middle values; 0 without samples.
pub fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    values.get(values.len() / 2).copied().unwrap_or(0.0)
}

/// (MRR, nDCG@10, precision@3, recall@3) of `ranked` against `gold`.
#[cfg(test)]
pub(crate) fn ranked_metrics(ranked: &[String], gold: &[String]) -> (f64, f64, f64, f64) {
    let mut reciprocal = 0.0;
    for (index, id) in ranked.iter().enumerate() {
        if gold.iter().any(|item| item == id) {
            reciprocal = 1.0 / (index as f64 + 1.0);
            break;
        }
    }
    let mut dcg = 0.0;
    let mut hits = 0.0;
    for (index, id) in ranked.iter().take(10).enumerate() {
        if gold.iter().any(|item| item == id) {
            dcg += 1.0 / ((index as f64) + 2.0).ln();
            if index < 3 {
                hits += 1.0;
            }
        }
    }
    let ideal_len = gold.len().min(10);
    let mut ideal = 0.0;
    for index in 0..ideal_len {
        ideal += 1.0 / ((index as f64) + 2.0).ln();
    }
    let ndcg = if ideal == 0.0 { 0.0 } else { dcg / ideal };
    let precision = crate::metrics::precision_at(gold, ranked, 3).unwrap_or(0.0);
    let recall = if gold.is_empty() {
        0.0
    } else {
        hits / gold.len() as f64
    };
    (reciprocal, ndcg, precision, recall)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_takes_the_middle_sample() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&mut [4.0, 1.0, 3.0, 2.0]), 3.0);
        assert_eq!(median(&mut []), 0.0);
    }

    #[test]
    fn locked_families_are_the_bake_off_families() {
        let names: Vec<&str> = crate::bakeoff::FAMILIES
            .iter()
            .map(|(family, _)| *family)
            .collect();
        assert_eq!(names, LOCKED);
        assert!(is_locked("paraphrase"));
        assert!(!is_locked("identifier"));
    }
}
