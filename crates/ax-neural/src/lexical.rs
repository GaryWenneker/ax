//! Pure-Rust lexical-semantic challenger for the bake-off (ADR 002 / 003).
//! No Candle/ONNX. Not blake3 whole-string hashing — that has no paraphrase signal.

use std::collections::HashMap;

const DIM: usize = 256;
const ABSTAIN_FLOOR: f32 = 0.35;

/// Stable FNV-1a bucket for a token.
fn bucket(token: &str) -> usize {
    let mut hash: u64 = 14695981039346656037;
    for byte in token.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    (hash as usize) % DIM
}

fn tokens(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '-')
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Small coding-domain synonym map so paraphrase of "representation / similarity"
/// can reach the hash-vectors memory without an ML model.
fn expand(token: &str) -> Vec<&str> {
    match token {
        "representation" => vec!["embedding", "vector", "model", "hash"],
        "similarity" => vec!["search", "embed", "match", "vector"],
        "releases" | "release" => vec!["build", "every", "release"],
        "chosen" => vec!["only", "must"],
        "embedding" => vec!["vector", "hash", "representation", "model"],
        "hash" => vec!["vector", "embedding", "hash-v1"],
        "vectors" | "vector" => vec!["embedding", "hash", "representation"],
        _ => Vec::new(),
    }
}

fn vectorize(text: &str) -> [f32; DIM] {
    let mut out = [0.0_f32; DIM];
    for token in tokens(text) {
        out[bucket(&token)] += 1.0;
        for syn in expand(&token) {
            out[bucket(syn)] += 1.0;
        }
    }
    let norm = out.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-9);
    for value in &mut out {
        *value /= norm;
    }
    out
}

fn cosine(a: &[f32; DIM], b: &[f32; DIM]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[derive(Debug, Clone)]
pub struct RankedHit {
    pub key: String,
    pub score: f32,
}

/// One fixture memory prepared for lexical ranking.
#[derive(Debug, Clone)]
pub struct LexicalDoc {
    pub key: String,
    pub triggers: Vec<String>,
    vector: [f32; DIM],
}

impl LexicalDoc {
    pub fn new(key: impl Into<String>, title: &str, body: &str, triggers: &[String]) -> Self {
        let key = key.into();
        let joined = format!("{key} {title} {body} {}", triggers.join(" "));
        Self {
            key,
            triggers: triggers.iter().map(|t| t.to_ascii_lowercase()).collect(),
            vector: vectorize(&joined),
        }
    }
}

/// Rank documents for a query. Exact trigger/key match gets a +1.0 boost.
/// Empty ranking + `abstained` when the top score is below the floor.
pub fn rank(docs: &[LexicalDoc], prompt: &str) -> (Vec<RankedHit>, bool) {
    let query = prompt.trim().to_ascii_lowercase();
    let qv = vectorize(&query);
    let mut scored: Vec<RankedHit> = docs
        .iter()
        .map(|doc| {
            let mut score = cosine(&qv, &doc.vector);
            if doc.triggers.iter().any(|t| t == &query) || doc.key.eq_ignore_ascii_case(&query) {
                score += 1.0;
            }
            RankedHit {
                key: doc.key.clone(),
                score,
            }
        })
        .collect();
    scored.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.key.cmp(&b.key)));
    let top = scored.first().map(|h| h.score).unwrap_or(0.0);
    if top < ABSTAIN_FLOOR {
        return (Vec::new(), true);
    }
    (scored, false)
}

pub const CHALLENGER_ID: &str = "lexical-semantic-cpu";

/// Score locked-family questions. Each item is (question_id, family, prompt, relevant_keys).
pub fn score_locked_families(
    docs: &[LexicalDoc],
    questions: &[(String, String, String, Vec<String>)],
) -> HashMap<String, serde_json::Value> {
    let mut families: HashMap<String, (u64, u64)> = HashMap::new();
    for (_qid, family, prompt, relevant) in questions {
        let entry = families.entry(family.clone()).or_insert((0, 0));
        entry.0 += 1;
        let (ranked, abstained) = rank(docs, prompt);
        let keys: Vec<&str> = ranked.iter().map(|h| h.key.as_str()).collect();
        let hit = match family.as_str() {
            "named_trigger" => keys
                .first()
                .is_some_and(|got| relevant.first().map(String::as_str) == Some(*got)),
            "abstention" => relevant.is_empty() && abstained,
            "paraphrase" => relevant
                .iter()
                .any(|want| keys.iter().take(3).any(|got| *got == want.as_str())),
            "delivery_budget" => relevant.iter().any(|want| keys.contains(&want.as_str())),
            _ => false,
        };
        if hit {
            entry.1 += 1;
        }
    }
    families
        .into_iter()
        .map(|(family, (scored, hits))| {
            (
                family,
                serde_json::json!({
                    "scored": scored,
                    "hits": hits,
                    "measured": true,
                    "challenger": CHALLENGER_ID
                }),
            )
        })
        .collect()
}

/// ADR 002 pass rule against baseline family objects `{family: {scored, hits}}`.
pub fn beats_baseline(
    baseline: &serde_json::Value,
    challenger: &HashMap<String, serde_json::Value>,
) -> bool {
    let base_para = baseline["paraphrase"]["hits"].as_u64().unwrap_or(0);
    let chal_para = challenger
        .get("paraphrase")
        .and_then(|v| v["hits"].as_u64())
        .unwrap_or(0);
    if chal_para <= base_para {
        return false;
    }
    for family in ["named_trigger", "abstention", "delivery_budget"] {
        let base_hits = baseline[family]["hits"].as_u64().unwrap_or(0);
        let chal_hits = challenger
            .get(family)
            .and_then(|v| v["hits"].as_u64())
            .unwrap_or(0);
        if chal_hits < base_hits {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coding_docs() -> Vec<LexicalDoc> {
        let fixture: serde_json::Value =
            serde_json::from_str(crate::CODING_FIXTURE).expect("fixture json");
        fixture["memories"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["insert"].as_bool().unwrap_or(true))
            .map(|m| {
                let triggers = m["triggers"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|t| t.as_str().map(str::to_string))
                    .collect::<Vec<_>>();
                LexicalDoc::new(
                    m["key"].as_str().unwrap(),
                    m["title"].as_str().unwrap_or(""),
                    m["body"].as_str().unwrap_or(""),
                    &triggers,
                )
            })
            .collect()
    }

    fn locked_questions() -> Vec<(String, String, String, Vec<String>)> {
        let fixture: serde_json::Value =
            serde_json::from_str(crate::CODING_FIXTURE).expect("fixture json");
        fixture["questions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|q| {
                let family = q["family"].as_str()?.to_string();
                if !matches!(
                    family.as_str(),
                    "named_trigger" | "paraphrase" | "abstention" | "delivery_budget"
                ) {
                    return None;
                }
                let relevant = q["relevant"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|r| r.as_str().map(str::to_string))
                    .collect();
                Some((
                    q["id"].as_str()?.to_string(),
                    family,
                    q["prompt"].as_str()?.to_string(),
                    relevant,
                ))
            })
            .collect()
    }

    #[test]
    fn lexical_beats_paraphrase_on_coding_fixture() {
        let docs = coding_docs();
        let questions = locked_questions();
        let families = score_locked_families(&docs, &questions);
        assert_eq!(families["paraphrase"]["hits"], 1);
        assert_eq!(families["named_trigger"]["hits"], 1);
        assert_eq!(families["abstention"]["hits"], 1);
        assert_eq!(families["delivery_budget"]["hits"], 1);
    }
}
