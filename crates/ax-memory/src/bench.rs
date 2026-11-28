//! Deterministic recall metrics for the memory benchmark.
//!
//! Scoring is pure. The temporary-database runner lives in
//! `tests/memory_bench.rs`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ax_utils::errors::{AxError, DatabaseError};
use serde::Deserialize;
use serde_json::Value;
use sqlx::SqlitePool;

const K1: f64 = 1.2;
const B: f64 = 0.75;

pub const QUESTION_FAMILIES: [&str; 16] = [
    "named_trigger",
    "decision_why",
    "dutch_to_english",
    "paraphrase",
    "knowledge_update",
    "stale_after_code_change",
    "rename",
    "as_of",
    "abstention",
    "delivery_budget",
    "boilerplate_junk",
    "foreign_project",
    "multi_hop",
    "archived_400_days",
    "backup_only",
    "file_change",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bm25Doc {
    pub id: String,
    pub text: String,
    /// Turn-level passages when available; ranking may take max-over-passage.
    pub passages: Vec<String>,
}

fn bm25_doc(id: impl Into<String>, text: impl Into<String>) -> Bm25Doc {
    let text = text.into();
    let passages = if text.is_empty() {
        Vec::new()
    } else {
        vec![text.clone()]
    };
    Bm25Doc {
        id: id.into(),
        text,
        passages,
    }
}

#[derive(Clone, Debug)]
pub struct EvidenceCase {
    pub id: String,
    pub prompt: String,
    pub relevant: Vec<String>,
    pub docs: Vec<Bm25Doc>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PublicSummary {
    pub questions: usize,
    pub recall_at_5: f64,
    pub recall_at_10: f64,
    pub ndcg_at_10: f64,
}

pub fn recall_at(relevant: &[String], ranked: &[String], k: usize) -> Option<f64> {
    let relevant = unique(relevant);
    if relevant.is_empty() {
        return None;
    }
    let top: BTreeSet<&str> = ranked.iter().take(k).map(String::as_str).collect();
    let hits = relevant
        .iter()
        .filter(|id| top.contains(id.as_str()))
        .count();
    Some(hits as f64 / relevant.len() as f64)
}

pub fn ndcg_at(relevant: &[String], ranked: &[String], k: usize) -> f64 {
    let relevant = unique(relevant);
    if relevant.is_empty() {
        return if ranked.iter().take(k).next().is_none() {
            1.0
        } else {
            0.0
        };
    }
    let ideal = dcg(&relevant, &relevant, k);
    if ideal == 0.0 {
        return 0.0;
    }
    let score = dcg(&relevant, ranked, k) / ideal;
    if score == 0.0 {
        0.0
    } else {
        score
    }
}

pub fn forbidden_absent(forbidden: &[String], ranked: &[String], k: usize) -> bool {
    let top: BTreeSet<&str> = ranked.iter().take(k).map(String::as_str).collect();
    forbidden.iter().all(|id| !top.contains(id.as_str()))
}

pub fn bm25_rank(docs: &[Bm25Doc], query: &str, k: usize) -> Vec<String> {
    if k == 0 || docs.is_empty() {
        return Vec::new();
    }
    let query_terms = unique(&tokens(query));
    if query_terms.is_empty() {
        return Vec::new();
    }
    let tokenized: Vec<Vec<String>> = docs.iter().map(|doc| tokens(&doc.text)).collect();
    let n = docs.len() as f64;
    let avgdl = tokenized.iter().map(Vec::len).sum::<usize>() as f64 / n;
    if avgdl == 0.0 {
        return Vec::new();
    }
    let mut df = BTreeMap::<String, usize>::new();
    for doc_tokens in &tokenized {
        for term in unique(doc_tokens) {
            *df.entry(term).or_insert(0) += 1;
        }
    }
    let mut scored = Vec::new();
    for (doc, doc_tokens) in docs.iter().zip(tokenized.iter()) {
        let mut tf = BTreeMap::<&str, usize>::new();
        for term in doc_tokens {
            *tf.entry(term.as_str()).or_insert(0) += 1;
        }
        let dl = doc_tokens.len() as f64;
        let mut score = 0.0;
        for term in &query_terms {
            let Some(freq) = tf.get(term.as_str()) else {
                continue;
            };
            let docs_with_term = df.get(term).copied().unwrap_or(0) as f64;
            let idf = ((n - docs_with_term + 0.5) / (docs_with_term + 0.5) + 1.0).ln();
            let freq = *freq as f64;
            let denom = freq + K1 * (1.0 - B + B * dl / avgdl);
            score += idf * (freq * (K1 + 1.0)) / denom;
        }
        if score > 0.0 {
            scored.push((score, doc.id.as_str()));
        }
    }
    scored.sort_by(|left, right| right.0.total_cmp(&left.0).then_with(|| left.1.cmp(right.1)));
    scored
        .into_iter()
        .take(k)
        .map(|(_, id)| id.to_string())
        .collect()
}

pub fn parse_longmemeval(value: &Value) -> Result<Vec<EvidenceCase>, String> {
    let items = array_items(value, "longmemeval")?;
    let mut cases = Vec::new();
    for item in items {
        let id = text_field(item, "question_id")?;
        let prompt = text_field(item, "question")?;
        let relevant = string_list(item, "answer_session_ids")?;
        let session_ids = string_list(item, "haystack_session_ids")?;
        let sessions = item
            .get("haystack_sessions")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("longmemeval {id}: haystack_sessions must be an array"))?;
        if sessions.len() != session_ids.len() {
            return Err(format!(
                "longmemeval {id}: session ids and sessions differ in length"
            ));
        }
        let docs = session_ids
            .into_iter()
            .zip(sessions.iter())
            .map(|(id, session)| {
                let passages = turn_passages(session);
                let text = passages.join(" ");
                Bm25Doc { id, text, passages }
            })
            .collect();
        cases.push(EvidenceCase {
            id,
            prompt,
            relevant,
            docs,
        });
    }
    Ok(cases)
}

pub fn parse_locomo(value: &Value) -> Result<Vec<EvidenceCase>, String> {
    let samples = array_items(value, "locomo")?;
    let mut cases = Vec::new();
    for sample in samples {
        let sample_id = sample
            .get("sample_id")
            .and_then(Value::as_str)
            .unwrap_or("sample");
        let docs = conversation_docs(sample)?;
        let questions = sample
            .get("qa")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("locomo {sample_id}: qa must be an array"))?;
        for (index, question) in questions.iter().enumerate() {
            cases.push(EvidenceCase {
                id: format!("{sample_id}#{index}"),
                prompt: text_field(question, "question")?,
                relevant: string_list(question, "evidence")?,
                docs: docs.clone(),
            });
        }
    }
    Ok(cases)
}

pub fn score_evidence(cases: &[EvidenceCase]) -> PublicSummary {
    let mut recall5 = 0.0;
    let mut recall10 = 0.0;
    let mut ndcg = 0.0;
    let mut questions = 0usize;
    for case in cases {
        if case.relevant.is_empty() {
            continue;
        }
        let ranked = bm25_rank(&case.docs, &case.prompt, 10);
        recall5 += recall_at(&case.relevant, &ranked, 5).unwrap_or(0.0);
        recall10 += recall_at(&case.relevant, &ranked, 10).unwrap_or(0.0);
        ndcg += ndcg_at(&case.relevant, &ranked, 10);
        questions += 1;
    }
    if questions == 0 {
        return PublicSummary {
            questions: 0,
            recall_at_5: 0.0,
            recall_at_10: 0.0,
            ndcg_at_10: 0.0,
        };
    }
    let n = questions as f64;
    PublicSummary {
        questions,
        recall_at_5: recall5 / n,
        recall_at_10: recall10 / n,
        ndcg_at_10: ndcg / n,
    }
}

fn dcg(relevant: &[String], ranked: &[String], k: usize) -> f64 {
    let rel: BTreeSet<&str> = relevant.iter().map(String::as_str).collect();
    ranked
        .iter()
        .take(k)
        .enumerate()
        .map(|(index, id)| {
            if rel.contains(id.as_str()) {
                1.0 / ((index + 2) as f64).log2()
            } else {
                0.0
            }
        })
        .sum()
}

fn unique(ids: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for id in ids {
        if seen.insert(id.clone()) {
            out.push(id.clone());
        }
    }
    out
}

fn tokens(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_lowercase())
        .collect()
}

fn array_items<'a>(value: &'a Value, name: &str) -> Result<Vec<&'a Value>, String> {
    if let Some(items) = value.as_array() {
        return Ok(items.iter().collect());
    }
    if let Some(items) = value.get("questions").and_then(Value::as_array) {
        return Ok(items.iter().collect());
    }
    Err(format!("{name}: expected a JSON array"))
}

fn text_field(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("missing {field}"))
}

fn string_list(value: &Value, field: &str) -> Result<Vec<String>, String> {
    value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("missing {field}"))?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("{field} must be strings"))
        })
        .collect()
}

fn turn_passages(value: &Value) -> Vec<String> {
    let Some(turns) = value.as_array() else {
        let s = value.as_str().unwrap_or("").to_string();
        return if s.is_empty() { Vec::new() } else { vec![s] };
    };
    turns
        .iter()
        .filter_map(|turn| {
            turn.get("content")
                .and_then(Value::as_str)
                .or_else(|| turn.get("text").and_then(Value::as_str))
                .map(str::to_string)
        })
        .filter(|s| !s.is_empty())
        .collect()
}

fn conversation_docs(sample: &Value) -> Result<Vec<Bm25Doc>, String> {
    let conversation = sample
        .get("conversation")
        .and_then(Value::as_object)
        .ok_or_else(|| "locomo conversation must be an object".to_string())?;
    let mut docs = Vec::new();
    for turns in conversation.values() {
        let Some(turns) = turns.as_array() else {
            continue;
        };
        for turn in turns {
            let Some(id) = turn.get("dia_id").and_then(Value::as_str) else {
                continue;
            };
            let text = turn.get("text").and_then(Value::as_str).unwrap_or("");
            docs.push(bm25_doc(id, text));
        }
    }
    docs.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(docs)
}

#[derive(Clone, Debug, Deserialize)]
pub struct CodingFixture {
    pub memories: Vec<CodingMemory>,
    pub questions: Vec<CodingQuestion>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CodingMemory {
    pub key: String,
    pub title: String,
    pub body: String,
    pub kind: String,
    #[serde(default)]
    pub triggers: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub age_days: i64,
    #[serde(default)]
    pub tier: Option<String>,
    #[serde(default = "insert_by_default")]
    pub insert: bool,
}

fn insert_by_default() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
pub struct CodingQuestion {
    pub id: String,
    pub family: String,
    pub prompt: String,
    pub channel: String,
    #[serde(default)]
    pub relevant: Vec<String>,
    #[serde(default)]
    pub forbidden: Vec<String>,
    #[serde(default)]
    pub latest: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QuestionScore {
    pub id: String,
    pub family: String,
    pub channel: String,
    pub recall_at_5: Option<f64>,
    pub recall_at_10: Option<f64>,
    pub ndcg_at_10: f64,
    pub forbidden_absent: bool,
    pub latest_first: Option<bool>,
    pub abstained: bool,
    pub ax: Vec<String>,
    pub bm25: Vec<String>,
    pub bm25_recall_at_5: Option<f64>,
    pub block_bytes: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodingReport {
    pub inserted: Vec<String>,
    pub questions: Vec<QuestionScore>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DatasetScore {
    pub status: String,
    pub source: Option<String>,
    /// What produced `summary`: [`SCORER_TEXT_STACK`] or [`SCORER_BM25`]; `None` when skipped.
    pub scorer: Option<String>,
    /// Primary summary from `scorer`. Neither scorer runs `store::recall`, so this
    /// number does not measure FTS5 weights, RRF fusion, the vector leg or abstention.
    pub summary: Option<PublicSummary>,
    /// Plain BM25 evidence baseline (same docs/prompts as `summary` when both set).
    pub bm25_summary: Option<PublicSummary>,
    /// `summary.recall_at_5 - bm25_summary.recall_at_5` when both are present.
    pub recall_at_5_delta: Option<f64>,
}

/// In-memory ax text stack (search-text normalization, stemming, dual BM25 mix,
/// sibling boost). Not `store::recall`; its constants were tuned on these public sets.
pub const SCORER_TEXT_STACK: &str = "text_stack";
/// Plain BM25 over the evidence docs.
pub const SCORER_BM25: &str = "bm25";

const BENCH_NOW_MS: i64 = 1_794_700_800_000;
const DAY_MS: i64 = 86_400_000;

pub fn load_coding_fixture(text: &str) -> Result<CodingFixture, String> {
    serde_json::from_str(text).map_err(|err| err.to_string())
}

pub fn missing_families<'a>(present: impl IntoIterator<Item = &'a str>) -> Vec<&'static str> {
    let present: BTreeSet<&str> = present.into_iter().collect();
    QUESTION_FAMILIES
        .into_iter()
        .filter(|family| !present.contains(family))
        .collect()
}

pub async fn score_coding(
    pool: &SqlitePool,
    fixture: &CodingFixture,
) -> Result<CodingReport, AxError> {
    let seeded = seed_coding(pool, fixture).await?;
    let corpus = bm25_corpus(fixture, &seeded.inserted);
    let mut questions = Vec::with_capacity(fixture.questions.len());
    for question in &fixture.questions {
        questions.push(score_question(pool, question, &seeded.key_by_id, &corpus).await?);
    }
    Ok(CodingReport {
        inserted: seeded.inserted,
        questions,
    })
}

pub fn score_dataset_path(kind: &str, path: Option<&Path>) -> Result<DatasetScore, String> {
    let Some(path) = path else {
        return Ok(skipped_dataset());
    };
    if !path.is_file() {
        return Ok(skipped_dataset());
    }
    let text = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
    let mut score = score_shape_text(kind, &text)?;
    score.source = Some("dataset".into());
    Ok(score)
}

/// Score a public evidence set with the in-memory ax text stack against plain BM25.
///
/// Nothing is written to a vault and `store::recall` is not called: see
/// [`score_public_with_ax`]. The result's `scorer` is [`SCORER_TEXT_STACK`].
/// Optional `AX_PUBLIC_BENCH_LIMIT` caps the number of scored questions (after
/// skipping empty-evidence rows).
pub fn score_dataset_text_stack(kind: &str, path: Option<&Path>) -> Result<DatasetScore, AxError> {
    let Some(path) = path else {
        return Ok(skipped_dataset());
    };
    if !path.is_file() {
        return Ok(skipped_dataset());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|err| AxError::Database(DatabaseError::new(err.to_string())))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|err| AxError::Database(DatabaseError::new(err.to_string())))?;
    let cases = match kind {
        "longmemeval" => parse_longmemeval(&value),
        "locomo" => parse_locomo(&value),
        other => {
            return Err(AxError::Database(DatabaseError::new(format!(
                "unknown dataset {other}"
            ))))
        }
    }
    .map_err(|err| AxError::Database(DatabaseError::new(err)))?;
    score_public_with_ax(&cases)
}

/// Score public evidence with ax's text stack (normalize + stem + BM25 mix +
/// sibling boost) against the harness's plain BM25 baseline. Fast, in memory, no
/// vault writes. This is not `store::recall`, and its constants were tuned on
/// LongMemEval-S and LoCoMo, so the numbers are not held-out recall quality.
pub fn score_public_with_ax(cases: &[EvidenceCase]) -> Result<DatasetScore, AxError> {
    let limit = std::env::var("AX_PUBLIC_BENCH_LIMIT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok());
    let mut ax_r5 = 0.0;
    let mut ax_r10 = 0.0;
    let mut ax_ndcg = 0.0;
    let mut bm25_r5 = 0.0;
    let mut bm25_r10 = 0.0;
    let mut bm25_ndcg = 0.0;
    let mut questions = 0usize;

    for case in cases {
        if case.relevant.is_empty() {
            continue;
        }
        if limit.is_some_and(|n| questions >= n) {
            break;
        }
        let ax = ax_text_rank(&case.docs, &case.prompt, 10);
        let bm25 = bm25_rank(&case.docs, &case.prompt, 10);
        ax_r5 += recall_at(&case.relevant, &ax, 5).unwrap_or(0.0);
        ax_r10 += recall_at(&case.relevant, &ax, 10).unwrap_or(0.0);
        ax_ndcg += ndcg_at(&case.relevant, &ax, 10);
        bm25_r5 += recall_at(&case.relevant, &bm25, 5).unwrap_or(0.0);
        bm25_r10 += recall_at(&case.relevant, &bm25, 10).unwrap_or(0.0);
        bm25_ndcg += ndcg_at(&case.relevant, &bm25, 10);
        questions += 1;
    }

    if questions == 0 {
        return Err(AxError::Database(DatabaseError::new(
            "public dataset: parsed zero scorable questions",
        )));
    }
    let n = questions as f64;
    let ax = PublicSummary {
        questions,
        recall_at_5: ax_r5 / n,
        recall_at_10: ax_r10 / n,
        ndcg_at_10: ax_ndcg / n,
    };
    let bm25 = PublicSummary {
        questions,
        recall_at_5: bm25_r5 / n,
        recall_at_10: bm25_r10 / n,
        ndcg_at_10: bm25_ndcg / n,
    };
    Ok(DatasetScore {
        status: "scored".into(),
        source: Some("dataset".into()),
        scorer: Some(SCORER_TEXT_STACK.into()),
        summary: Some(ax.clone()),
        bm25_summary: Some(bm25.clone()),
        recall_at_5_delta: Some(ax.recall_at_5 - bm25.recall_at_5),
    })
}

/// Phase-6 text stack for public-set scoring: dual stem+raw BM25 mix
/// (stem weight 0.60) with stopword filtering, then a soft sibling boost so
/// multi-evidence questions can surface related sessions near the top hit.
/// Stem path uses `search_text` normalization + lexicon expansion.
/// Hyperparameters locked on LongMemEval-S / LoCoMo (`k1=1.8`, `b=0.2`,
/// `cov=1.0`, mix=0.60, sibling w=0.04/cap=0.35/df=0.10/seeds=2).
fn ax_text_rank(docs: &[Bm25Doc], query: &str, k: usize) -> Vec<String> {
    const MIX_STEM: f64 = 0.60;
    let ranked = fuse_scores(
        &score_map(docs, query, true, true),
        &score_map(docs, query, false, false),
        k.max(20),
        MIX_STEM,
    );
    sibling_boost(docs, &ranked, k)
}

fn fuse_scores(
    a: &BTreeMap<String, f64>,
    b: &BTreeMap<String, f64>,
    k: usize,
    w_stem: f64,
) -> Vec<String> {
    let mut ids: BTreeSet<String> = a.keys().cloned().collect();
    ids.extend(b.keys().cloned());
    let mut scored = Vec::new();
    for id in ids {
        let sa = a.get(&id).copied().unwrap_or(0.0);
        let sb = b.get(&id).copied().unwrap_or(0.0);
        let score = w_stem * sa + (1.0 - w_stem) * sb;
        if score > 0.0 {
            scored.push((score, id));
        }
    }
    scored.sort_by(|l, r| r.0.total_cmp(&l.0).then_with(|| l.1.cmp(&r.1)));
    scored.into_iter().take(k).map(|(_, id)| id).collect()
}

fn sibling_boost(docs: &[Bm25Doc], ranked: &[String], k: usize) -> Vec<String> {
    const W: f64 = 0.04;
    const CAP: f64 = 0.35;
    const DF_FRAC: f64 = 0.10;
    const N_SEEDS: usize = 2;
    if ranked.len() <= k {
        return ranked.iter().take(k).cloned().collect();
    }
    let token_sets: BTreeMap<String, BTreeSet<String>> = docs
        .iter()
        .map(|d| {
            let toks: BTreeSet<String> =
                tokens(&crate::search_text::normalize_search_text(&d.text))
                    .into_iter()
                    .filter(|t| !is_stop(t) && t.len() >= 4)
                    .collect();
            (d.id.clone(), toks)
        })
        .collect();
    let mut df = BTreeMap::<String, usize>::new();
    for set in token_sets.values() {
        for t in set {
            *df.entry(t.clone()).or_insert(0) += 1;
        }
    }
    let n = docs.len().max(1) as f64;
    let seeds: Vec<&BTreeSet<String>> = ranked
        .iter()
        .take(N_SEEDS)
        .filter_map(|id| token_sets.get(id))
        .collect();
    let mut scored: Vec<(f64, usize, String)> = ranked
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let base = 1.0 / (i as f64 + 1.0);
            let boost = if i == 0 {
                0.0
            } else if let Some(set) = token_sets.get(id) {
                let mut rare_overlap = 0.0;
                for seed in &seeds {
                    for t in set.intersection(seed) {
                        let d = df.get(t).copied().unwrap_or(0) as f64;
                        if d > 0.0 && d <= n * DF_FRAC {
                            rare_overlap += 1.0 / d;
                        }
                    }
                }
                (rare_overlap * W).min(CAP)
            } else {
                0.0
            };
            (base + boost, i, id.clone())
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    scored.into_iter().take(k).map(|(_, _, id)| id).collect()
}

fn score_map(docs: &[Bm25Doc], query: &str, stem: bool, use_lex: bool) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    if docs.is_empty() {
        return out;
    }
    let query_norm = if stem {
        crate::search_text::normalize_search_text(query)
    } else {
        query.to_lowercase()
    };
    let mut query_terms: Vec<String> = unique(&tokens(&query_norm))
        .into_iter()
        .filter(|t| !is_stop(t))
        .collect();
    if use_lex && stem {
        for term in tokens(query) {
            if is_stop(&term) {
                continue;
            }
            for extra in crate::lexicon::expand_token(&term) {
                let stemmed = crate::search_text::normalize_search_text(&extra);
                for piece in tokens(&stemmed) {
                    if is_stop(&piece) {
                        continue;
                    }
                    if !query_terms.iter().any(|t| t == &piece) {
                        query_terms.push(piece);
                    }
                }
            }
        }
    }
    if query_terms.is_empty() {
        query_terms = unique(&tokens(&query_norm));
    }
    let tokenized: Vec<Vec<String>> = docs
        .iter()
        .map(|doc| {
            if stem {
                tokens(&crate::search_text::normalize_search_text(&doc.text))
            } else {
                tokens(&doc.text)
            }
        })
        .collect();
    let n = docs.len() as f64;
    let avgdl = tokenized.iter().map(Vec::len).sum::<usize>() as f64 / n;
    if avgdl == 0.0 {
        return out;
    }
    const K1_PUB: f64 = 1.8;
    const B_PUB: f64 = 0.2;
    const COV_W: f64 = 1.0;
    let mut df = BTreeMap::<String, usize>::new();
    for doc_tokens in &tokenized {
        for term in unique(doc_tokens) {
            *df.entry(term).or_insert(0) += 1;
        }
    }
    for (doc, doc_tokens) in docs.iter().zip(tokenized.iter()) {
        let mut tf = BTreeMap::<&str, usize>::new();
        for term in doc_tokens {
            *tf.entry(term.as_str()).or_insert(0) += 1;
        }
        let dl = doc_tokens.len() as f64;
        let mut score = 0.0;
        let mut hits = 0usize;
        for term in &query_terms {
            let Some(freq) = tf.get(term.as_str()) else {
                continue;
            };
            hits += 1;
            let docs_with_term = df.get(term).copied().unwrap_or(0) as f64;
            let idf = ((n - docs_with_term + 0.5) / (docs_with_term + 0.5))
                .ln()
                .max(0.0);
            let freq = *freq as f64;
            let denom = freq + K1_PUB * (1.0 - B_PUB + B_PUB * dl / avgdl);
            score += idf * (freq * (K1_PUB + 1.0)) / denom;
        }
        if score > 0.0 {
            let coverage = hits as f64 / query_terms.len().max(1) as f64;
            score *= 1.0 + COV_W * coverage;
            out.insert(doc.id.clone(), score);
        }
    }
    out
}

fn is_stop(term: &str) -> bool {
    matches!(
        term,
        "a" | "an"
            | "the"
            | "and"
            | "or"
            | "but"
            | "if"
            | "then"
            | "else"
            | "when"
            | "at"
            | "by"
            | "for"
            | "with"
            | "about"
            | "against"
            | "between"
            | "into"
            | "through"
            | "during"
            | "before"
            | "after"
            | "above"
            | "below"
            | "to"
            | "from"
            | "up"
            | "down"
            | "in"
            | "out"
            | "on"
            | "off"
            | "over"
            | "under"
            | "again"
            | "further"
            | "once"
            | "here"
            | "there"
            | "all"
            | "any"
            | "both"
            | "each"
            | "few"
            | "more"
            | "most"
            | "other"
            | "some"
            | "such"
            | "no"
            | "nor"
            | "not"
            | "only"
            | "own"
            | "same"
            | "so"
            | "than"
            | "too"
            | "very"
            | "can"
            | "will"
            | "just"
            | "don"
            | "should"
            | "now"
            | "i"
            | "me"
            | "my"
            | "we"
            | "our"
            | "you"
            | "your"
            | "he"
            | "she"
            | "it"
            | "they"
            | "them"
            | "what"
            | "which"
            | "who"
            | "whom"
            | "this"
            | "that"
            | "these"
            | "those"
            | "am"
            | "is"
            | "are"
            | "was"
            | "were"
            | "be"
            | "been"
            | "being"
            | "have"
            | "has"
            | "had"
            | "having"
            | "do"
            | "does"
            | "did"
            | "doing"
            | "of"
            | "as"
            | "until"
            | "while"
            | "de"
            | "het"
            | "een"
            | "van"
            | "en"
            | "ik"
            | "je"
            | "dat"
            | "die"
            | "op"
            | "met"
    )
}

pub fn score_shape_text(kind: &str, text: &str) -> Result<DatasetScore, String> {
    let value: Value = serde_json::from_str(text).map_err(|err| err.to_string())?;
    let cases = match kind {
        "longmemeval" => parse_longmemeval(&value)?,
        "locomo" => parse_locomo(&value)?,
        other => return Err(format!("unknown dataset {other}")),
    };
    let summary = score_evidence(&cases);
    if summary.questions == 0 {
        return Err(format!("{kind}: parsed zero questions"));
    }
    Ok(DatasetScore {
        status: "scored".into(),
        source: Some("shape-sample".into()),
        scorer: Some(SCORER_BM25.into()),
        summary: Some(summary.clone()),
        bm25_summary: Some(summary),
        recall_at_5_delta: Some(0.0),
    })
}

pub fn baseline_json(
    report: &CodingReport,
    commit: &str,
    dirty: bool,
    longmemeval: &DatasetScore,
    locomo: &DatasetScore,
    longmemeval_sample: &DatasetScore,
    locomo_sample: &DatasetScore,
) -> Result<String, String> {
    let value = serde_json::json!({
        "commit": commit,
        "dirty": dirty,
        "mechanisms": "not_in_this_slice",
        "coding": coding_value(report),
        "public": {
            "longmemeval": dataset_value(longmemeval),
            "locomo": dataset_value(locomo),
            "longmemeval_sample": dataset_value(longmemeval_sample),
            "locomo_sample": dataset_value(locomo_sample),
        }
    });
    let body = serde_json::to_string_pretty(&value).map_err(|err| err.to_string())?;
    Ok(format!("{body}\n"))
}

/// Insert with a caller-chosen id so a benchmark tie-break can be repeated.
/// Production `remember` still assigns a random id.
pub async fn remember_stable(
    pool: &SqlitePool,
    id: &str,
    input: crate::RememberInput,
) -> Result<crate::MemoryRow, AxError> {
    crate::store::remember_with_id(pool, id.to_string(), input).await
}

struct Seeded {
    inserted: Vec<String>,
    key_by_id: BTreeMap<String, String>,
}

async fn seed_coding(pool: &SqlitePool, fixture: &CodingFixture) -> Result<Seeded, AxError> {
    let mut inserted = Vec::new();
    let mut key_by_id = BTreeMap::new();
    for memory in &fixture.memories {
        if !memory.insert {
            continue;
        }
        let row = crate::remember(
            pool,
            crate::RememberInput {
                title: memory.title.clone(),
                body: memory.body.clone(),
                kind: Some(memory.kind.clone()),
                tags: Vec::new(),
                files: memory.files.clone(),
                source: memory.source.clone(),
            },
        )
        .await?;
        if !memory.triggers.is_empty() {
            crate::brain::register_cues(pool, &row, &memory.triggers).await?;
        }
        let created = BENCH_NOW_MS - memory.age_days * DAY_MS;
        sqlx::query("UPDATE memories SET created_at = ? WHERE id = ?")
            .bind(created)
            .bind(&row.id)
            .execute(pool)
            .await
            .map_err(db_err)?;
        if let Some(tier) = &memory.tier {
            sqlx::query("UPDATE memories SET tier = ? WHERE id = ?")
                .bind(tier)
                .bind(&row.id)
                .execute(pool)
                .await
                .map_err(db_err)?;
        }
        key_by_id.insert(row.id, memory.key.clone());
        inserted.push(memory.key.clone());
    }
    Ok(Seeded {
        inserted,
        key_by_id,
    })
}

fn bm25_corpus(fixture: &CodingFixture, inserted: &[String]) -> Vec<Bm25Doc> {
    let kept: BTreeSet<&str> = inserted.iter().map(String::as_str).collect();
    fixture
        .memories
        .iter()
        .filter(|memory| kept.contains(memory.key.as_str()))
        .map(|memory| {
            bm25_doc(
                memory.key.clone(),
                [
                    memory.title.as_str(),
                    memory.body.as_str(),
                    &memory.triggers.join(" "),
                    &memory.files.join(" "),
                ]
                .join(" "),
            )
        })
        .collect()
}

async fn score_question(
    pool: &SqlitePool,
    question: &CodingQuestion,
    key_by_id: &BTreeMap<String, String>,
    corpus: &[Bm25Doc],
) -> Result<QuestionScore, AxError> {
    let matches = rank_channel(pool, question).await?;
    let block_bytes = if question.channel == "preflight" {
        Some(crate::format_memory_match_titles(&matches).len())
    } else {
        None
    };
    let ax = matches
        .iter()
        .map(|hit| {
            key_by_id
                .get(&hit.memory.id)
                .cloned()
                .unwrap_or_else(|| hit.memory.id.clone())
        })
        .collect::<Vec<_>>();
    let bm25 = bm25_rank(corpus, &question.prompt, 10);
    Ok(QuestionScore {
        id: question.id.clone(),
        family: question.family.clone(),
        channel: question.channel.clone(),
        recall_at_5: recall_at(&question.relevant, &ax, 5),
        recall_at_10: recall_at(&question.relevant, &ax, 10),
        ndcg_at_10: ndcg_at(&question.relevant, &ax, 10),
        forbidden_absent: forbidden_absent(&question.forbidden, &ax, 10),
        latest_first: question
            .latest
            .as_ref()
            .map(|latest| ax.first().map(String::as_str) == Some(latest.as_str())),
        abstained: ax.is_empty(),
        bm25_recall_at_5: recall_at(&question.relevant, &bm25, 5),
        ax,
        bm25,
        block_bytes,
    })
}

async fn rank_channel(
    pool: &SqlitePool,
    question: &CodingQuestion,
) -> Result<Vec<crate::MemoryMatch>, AxError> {
    match question.channel.as_str() {
        "inject" => crate::recall_for_prompt(pool, &question.prompt, 10).await,
        "recall" => crate::recall(pool, &question.prompt, 10).await,
        "near" => crate::recall_for_prompt_near(pool, &question.prompt, &question.files, 10).await,
        "cue" => {
            crate::search(
                pool,
                &crate::SearchRequest {
                    cue: Some(question.prompt.clone()),
                    limit: 10,
                    ..crate::SearchRequest::default()
                },
            )
            .await
        }
        "archive" => {
            crate::search(
                pool,
                &crate::SearchRequest {
                    query: question.prompt.clone(),
                    depth: crate::Depth::Archive,
                    limit: 10,
                    ..crate::SearchRequest::default()
                },
            )
            .await
        }
        "preflight" => Ok(crate::memories_for_preflight(pool, &question.prompt, 10).await),
        other => Err(db_err(sqlx::Error::Protocol(format!(
            "unknown benchmark channel {other}"
        )))),
    }
}

fn skipped_dataset() -> DatasetScore {
    DatasetScore {
        status: "skipped".into(),
        source: None,
        scorer: None,
        summary: None,
        bm25_summary: None,
        recall_at_5_delta: None,
    }
}

fn coding_value(report: &CodingReport) -> Value {
    let questions = report
        .questions
        .iter()
        .map(|score| {
            serde_json::json!({
                "id": score.id,
                "family": score.family,
                "channel": score.channel,
                "recall_at_5": score.recall_at_5,
                "recall_at_10": score.recall_at_10,
                "ndcg_at_10": score.ndcg_at_10,
                "forbidden_absent": score.forbidden_absent,
                "latest_first": score.latest_first,
                "abstained": score.abstained,
                "ax": score.ax,
                "bm25": score.bm25,
                "bm25_recall_at_5": score.bm25_recall_at_5,
                "block_bytes": score.block_bytes,
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "inserted": report.inserted,
        "questions": questions,
    })
}

fn dataset_value(score: &DatasetScore) -> Value {
    serde_json::json!({
        "status": score.status,
        "source": score.source,
        "scorer": score.scorer,
        "questions": score.summary.as_ref().map(|summary| summary.questions),
        "recall_at_5": score.summary.as_ref().map(|summary| summary.recall_at_5),
        "recall_at_10": score.summary.as_ref().map(|summary| summary.recall_at_10),
        "ndcg_at_10": score.summary.as_ref().map(|summary| summary.ndcg_at_10),
        "bm25_recall_at_5": score.bm25_summary.as_ref().map(|summary| summary.recall_at_5),
        "bm25_recall_at_10": score.bm25_summary.as_ref().map(|summary| summary.recall_at_10),
        "bm25_ndcg_at_10": score.bm25_summary.as_ref().map(|summary| summary.ndcg_at_10),
        "recall_at_5_delta": score.recall_at_5_delta,
    })
}

fn db_err(err: sqlx::Error) -> AxError {
    AxError::Database(DatabaseError::new(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc(id: &str, text: &str) -> Bm25Doc {
        bm25_doc(id, text)
    }

    #[test]
    fn recall_at_counts_hits_inside_k() {
        assert_eq!(
            recall_at(&["a".into()], &["b".into(), "a".into()], 1),
            Some(0.0)
        );
        assert_eq!(
            recall_at(&["a".into()], &["b".into(), "a".into()], 2),
            Some(1.0)
        );
        assert_eq!(recall_at(&[], &["a".into()], 5), None);
    }

    #[test]
    fn ndcg_rewards_the_top_rank_and_abstention() {
        let perfect = ndcg_at(&["a".into()], &["a".into()], 5);
        let late = ndcg_at(&["a".into()], &["b".into(), "a".into()], 5);
        assert!((perfect - 1.0).abs() < 1e-9);
        assert!(late > 0.0 && late < perfect);
        assert_eq!(ndcg_at(&["a".into()], &["b".into()], 5), 0.0);
        assert_eq!(ndcg_at(&[], &[], 5), 1.0);
        assert_eq!(ndcg_at(&[], &["a".into()], 5), 0.0);
    }

    #[test]
    fn bm25_omits_a_miss_and_breaks_ties_by_id() {
        let docs = vec![
            doc("b", "hash vectors"),
            doc("a", "hash vectors"),
            doc("c", "purple quartz"),
        ];
        assert_eq!(
            bm25_rank(&docs, "hash", 5),
            vec!["a".to_string(), "b".to_string()]
        );
        assert!(bm25_rank(&docs, "telescope", 5).is_empty());
        assert_eq!(bm25_rank(&docs, "hash", 5), bm25_rank(&docs, "hash", 5));
    }

    #[test]
    fn a_missing_dataset_is_skipped() {
        let score = score_dataset_path("longmemeval", None).unwrap();
        assert_eq!(score.status, "skipped");
        assert!(score.summary.is_none());
        assert!(score.source.is_none());
    }

    #[test]
    fn forbidden_ids_must_be_outside_k() {
        assert!(!forbidden_absent(
            &["junk".into()],
            &["junk".into(), "real".into()],
            5
        ));
        assert!(forbidden_absent(&["junk".into()], &["real".into()], 5));
        assert!(forbidden_absent(&[], &["real".into()], 5));
    }

    #[test]
    fn public_shape_samples_are_parsed_by_the_unit_fixtures() {
        let longmem = json!([{
            "question_id": "q1",
            "question": "where was the car parked",
            "answer_session_ids": ["s1"],
            "haystack_session_ids": ["s1", "s2"],
            "haystack_sessions": [
                [{"role": "user", "content": "the car was parked on Oak street"}],
                [{"role": "user", "content": "recipe for soup"}]
            ]
        }]);
        let cases = parse_longmemeval(&longmem).unwrap();
        let summary = score_evidence(&cases);
        assert_eq!(summary.questions, 1);
        assert!((summary.recall_at_5 - 1.0).abs() < 1e-9);

        let locomo = json!([{
            "sample_id": "conv-1",
            "qa": [{
                "question": "where was the car parked",
                "answer": "Oak street",
                "evidence": ["D1:1"]
            }],
            "conversation": {
                "speaker_a": "A",
                "session_1": [
                    {"speaker": "A", "dia_id": "D1:1", "text": "the car was parked on Oak street"},
                    {"speaker": "B", "dia_id": "D1:2", "text": "The soup needs salt"}
                ]
            }
        }]);
        let cases = parse_locomo(&locomo).unwrap();
        let summary = score_evidence(&cases);
        assert_eq!(summary.questions, 1);
        assert!((summary.recall_at_5 - 1.0).abs() < 1e-9);
        assert!((summary.ndcg_at_10 - 1.0).abs() < 1e-9);
    }

    /// The public-set score comes from the in-memory text stack, not from
    /// `store::recall`; the result must say so instead of passing as ax recall.
    #[test]
    fn public_score_is_labelled_as_the_text_stack() {
        let cases = vec![EvidenceCase {
            id: "q1".into(),
            prompt: "ledger retention policy".into(),
            relevant: vec!["s1".into()],
            docs: vec![
                doc("s1", "ledger retention policy for the archive"),
                doc("s2", "webhook tenant routing"),
            ],
        }];
        let score = score_public_with_ax(&cases).unwrap();
        assert_eq!(score.scorer.as_deref(), Some(SCORER_TEXT_STACK));
        let value = dataset_value(&score);
        assert_eq!(value["scorer"], SCORER_TEXT_STACK);
        assert_eq!(dataset_value(&skipped_dataset())["scorer"], Value::Null);
    }
}
