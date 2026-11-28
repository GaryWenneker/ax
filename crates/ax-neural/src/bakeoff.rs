use ax_memory::bench::{load_coding_fixture, score_coding, CodingFixture, CodingReport};
use ax_memory::embed::embed_text;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub(crate) const FAMILIES: [(&str, &str); 4] = [
    ("named_trigger", "top result is the named memory"),
    ("paraphrase", "a relevant memory in the top 3"),
    ("abstention", "nothing returned"),
    (
        "delivery_budget",
        "a relevant memory in the preflight memory block",
    ),
];

/// True when `neural-runtime` is on and the lexical challenger beats the known
/// hash-v1 locked-family hits on the embedded coding fixture (ADR 002).
/// Deterministic and sync. The fixture is embedded, so the answer cannot change
/// within a process: it is computed once and kept. The bake-off report data is
/// not cached.
pub fn challenger_admitted() -> bool {
    static ADMITTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ADMITTED.get_or_init(compute_challenger_admitted)
}

#[cfg(test)]
static ADMISSION_COMPUTATIONS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn compute_challenger_admitted() -> bool {
    #[cfg(test)]
    ADMISSION_COMPUTATIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if !crate::runtime_built() {
        return false;
    }
    let Ok(fixture) = load_coding_fixture(crate::CODING_FIXTURE) else {
        return false;
    };
    let baseline = known_hash_v1_family_hits();
    let challenger = lexical_challenger_families(&fixture);
    crate::lexical::beats_baseline(&baseline, &challenger)
}

/// Locked-family scores of the lexical challenger on the fixture.
fn lexical_challenger_families(
    fixture: &CodingFixture,
) -> std::collections::HashMap<String, Value> {
    crate::lexical::score_locked_families(
        &crate::fixture::lexical_docs(fixture),
        &crate::fixture::locked_questions(fixture),
    )
}

/// Measured hash-v1 hits on the coding fixture (bakeoff-results / baseline_report).
fn known_hash_v1_family_hits() -> Value {
    json!({
        "named_trigger": { "scored": 1, "hits": 1 },
        "paraphrase": { "scored": 1, "hits": 0 },
        "abstention": { "scored": 1, "hits": 1 },
        "delivery_budget": { "scored": 1, "hits": 1 }
    })
}

/// Scores hash-v1 with the locked fusion through the memory bench, in a temporary database.
/// When `neural-runtime` is on, also scores the pure-Rust lexical challenger and may admit it.
pub async fn baseline_report(text: &str) -> Result<Value, String> {
    let fixture = load_coding_fixture(text)?;
    let dir = tempfile::tempdir().map_err(|error| error.to_string())?;
    let database = ax_db::Database::open(&dir.path().join("bakeoff.db"))
        .await
        .map_err(|error| error.to_string())?;
    let started = Instant::now();
    let scored = score_coding(database.pool(), &fixture).await;
    let score_millis = millis(started.elapsed());
    database.pool().close().await;
    let report = scored.map_err(|error| error.to_string())?;
    let mut families = serde_json::Map::new();
    let mut without_ax = serde_json::Map::new();
    for (family, rule) in FAMILIES {
        families.insert(
            family.to_string(),
            family_summary(&fixture, &report, family, rule),
        );
        without_ax.insert(
            family.to_string(),
            family_summary_rank(&fixture, &report, family, rule, RankSource::Bm25),
        );
    }
    let families_value = Value::Object(families.clone());
    let (challenger_families, admitted, winner, lexical_status) =
        evaluate_lexical_challenger(&fixture, &families_value);
    let mut candidates = serde_json::Map::new();
    candidates.insert("hash-v1".into(), json!("built"));
    candidates.insert("onnx-runtime-cpu".into(), json!("not_built"));
    candidates.insert("candle-cpu".into(), json!("not_built"));
    candidates.insert(
        "cpu-blake3".into(),
        json!(if crate::runtime_built() {
            "built-not-admissible"
        } else {
            "not_built"
        }),
    );
    candidates.insert(crate::lexical::CHALLENGER_ID.into(), json!(lexical_status));
    let candidates = Value::Object(candidates);
    Ok(json!({
        "fixture": "benchmarks/memory/coding.json (built in)",
        "baseline": "hash-v1 with the locked fusion, scored by ax_memory::bench::score_coding",
        "candidates": candidates,
        "winner": winner,
        "challengerAdmitted": admitted,
        "challengerFamilies": challenger_families,
        "withoutAx": {
            "method": "bm25 rankings from the same coding fixture (no Ax fusion)",
            "families": without_ax
        },
        "runtimeFeature": crate::runtime_built(),
        "binarySizeDeltaBytes": 0,
        "queryLatency": query_latency(&fixture, score_millis),
        "ram": {
            "kind": "not_measured",
            "reason": "the standard library has no portable resident-memory reading; a challenger must record it before admission"
        },
        "families": families,
        "database": "temporary",
        "openedLiveDatabase": false
    }))
}

fn evaluate_lexical_challenger(
    fixture: &CodingFixture,
    baseline_families: &Value,
) -> (Value, bool, &'static str, &'static str) {
    if !crate::runtime_built() {
        return (Value::Null, false, "hash-v1", "not_built");
    }
    let challenger = lexical_challenger_families(fixture);
    let admitted = crate::lexical::beats_baseline(baseline_families, &challenger);
    let winner = if admitted {
        crate::lexical::CHALLENGER_ID
    } else {
        "hash-v1"
    };
    (
        Value::Object(challenger.into_iter().collect()),
        admitted,
        winner,
        "built",
    )
}

#[derive(Clone, Copy)]
enum RankSource {
    Ax,
    Bm25,
}

fn family_summary_rank(
    fixture: &CodingFixture,
    report: &CodingReport,
    family: &str,
    rule: &str,
    source: RankSource,
) -> Value {
    let relevant: HashMap<&str, &[String]> = fixture
        .questions
        .iter()
        .map(|question| (question.id.as_str(), question.relevant.as_slice()))
        .collect();
    let mut scored = 0;
    let mut hits = 0;
    let mut block_bytes = 0;
    for score in report
        .questions
        .iter()
        .filter(|score| score.family == family)
    {
        let expected = relevant.get(score.id.as_str()).copied().unwrap_or_default();
        scored += 1;
        block_bytes += score.block_bytes.unwrap_or(0);
        let ranked = match source {
            RankSource::Ax => score.ax.as_slice(),
            RankSource::Bm25 => score.bm25.as_slice(),
        };
        let abstained = match source {
            RankSource::Ax => score.abstained,
            RankSource::Bm25 => ranked.is_empty(),
        };
        let hit = match family {
            "named_trigger" => ranked
                .first()
                .is_some_and(|id| expected.first() == Some(id)),
            "abstention" => expected.is_empty() && abstained,
            "paraphrase" => expected
                .iter()
                .any(|id| ranked.iter().take(3).any(|got| got == id)),
            _ => expected.iter().any(|id| ranked.contains(id)),
        };
        if hit {
            hits += 1;
        }
    }
    let mut summary = json!({ "scored": scored, "hits": hits, "rule": rule, "measured": true });
    if family == "delivery_budget" {
        summary["blockBytes"] = json!(block_bytes);
    }
    summary
}

fn family_summary(
    fixture: &CodingFixture,
    report: &CodingReport,
    family: &str,
    rule: &str,
) -> Value {
    family_summary_rank(fixture, report, family, rule, RankSource::Ax)
}

fn query_latency(fixture: &CodingFixture, score_millis: f64) -> Value {
    let mut micros: Vec<f64> = fixture
        .questions
        .iter()
        .map(|question| {
            let started = Instant::now();
            std::hint::black_box(embed_text(&question.prompt));
            micros(started.elapsed())
        })
        .collect();
    json!({
        "kind": "measured",
        "samples": micros.len(),
        "embedMicrosMedian": crate::fixture::median(&mut micros),
        "scoreMillis": score_millis
    })
}

fn millis(elapsed: Duration) -> f64 {
    elapsed.as_secs_f64() * 1000.0
}

fn micros(elapsed: Duration) -> f64 {
    elapsed.as_secs_f64() * 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_memory::bench::QuestionScore;

    #[test]
    fn challenger_admission_is_computed_once_per_process() {
        let first = challenger_admitted();
        for _ in 0..3 {
            assert_eq!(challenger_admitted(), first);
        }
        assert_eq!(
            ADMISSION_COMPUTATIONS.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
    }

    #[tokio::test]
    async fn coding_fixture_keeps_hash_v1_and_admits_nobody() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/memory/coding.json");
        let text = std::fs::read_to_string(path).unwrap();
        let report = baseline_report(&text).await.unwrap();
        assert_eq!(report["candidates"]["onnx-runtime-cpu"], "not_built");
        assert_eq!(report["candidates"]["candle-cpu"], "not_built");
        assert_eq!(report["openedLiveDatabase"], false);
        assert!(
            report["families"]["named_trigger"]["scored"]
                .as_u64()
                .unwrap()
                >= 1
        );
        assert!(report["families"]["paraphrase"]["scored"].as_u64().unwrap() >= 1);
        assert!(report["families"]["abstention"]["scored"].as_u64().unwrap() >= 1);
        assert!(
            report["families"]["delivery_budget"]["scored"]
                .as_u64()
                .unwrap()
                >= 1
        );
        if crate::runtime_built() {
            assert_eq!(report["candidates"][crate::lexical::CHALLENGER_ID], "built");
            assert_eq!(report["candidates"]["cpu-blake3"], "built-not-admissible");
            assert_eq!(report["challengerAdmitted"], true);
            assert_eq!(report["winner"], crate::lexical::CHALLENGER_ID);
            assert!(
                report["challengerFamilies"]["paraphrase"]["hits"]
                    .as_u64()
                    .unwrap()
                    > report["families"]["paraphrase"]["hits"].as_u64().unwrap()
            );
        } else {
            assert_eq!(report["winner"], "hash-v1");
            assert_eq!(report["challengerAdmitted"], false);
            assert_eq!(
                report["candidates"][crate::lexical::CHALLENGER_ID],
                "not_built"
            );
        }
    }

    #[tokio::test]
    async fn baseline_report_includes_without_ax_bm25() {
        let report = baseline_report(crate::CODING_FIXTURE).await.unwrap();
        assert_eq!(
            report["withoutAx"]["method"],
            "bm25 rankings from the same coding fixture (no Ax fusion)"
        );
        assert!(
            report["withoutAx"]["families"]["paraphrase"]["scored"]
                .as_u64()
                .unwrap()
                >= 1
        );
        assert!(report["withoutAx"]["families"]["named_trigger"]["measured"]
            .as_bool()
            .unwrap());
    }

    #[tokio::test]
    async fn baseline_uses_the_fused_pipeline() {
        let report = baseline_report(crate::CODING_FIXTURE).await.unwrap();
        let families = &report["families"];
        assert_eq!(families["named_trigger"]["scored"], 1);
        assert_eq!(families["named_trigger"]["hits"], 1);
        assert_eq!(families["abstention"]["scored"], 1);
        assert_eq!(families["abstention"]["hits"], 1);
        assert_eq!(families["delivery_budget"]["scored"], 1);
        assert_eq!(families["delivery_budget"]["hits"], 1);
        assert!(families["delivery_budget"]["blockBytes"].as_u64().unwrap() > 0);
        assert_eq!(families["paraphrase"]["scored"], 1);
        assert_eq!(families["paraphrase"]["hits"], 0);
        assert_eq!(report["database"], "temporary");
        assert_eq!(report["openedLiveDatabase"], false);
        assert!(report["baseline"]
            .as_str()
            .unwrap()
            .contains("locked fusion"));
    }

    #[tokio::test]
    async fn baseline_latency_is_measured() {
        let report = baseline_report(crate::CODING_FIXTURE).await.unwrap();
        let latency = &report["queryLatency"];
        assert_eq!(latency["kind"], "measured");
        assert_eq!(latency["samples"], 19);
        assert!(latency["embedMicrosMedian"].as_f64().unwrap() > 0.0);
        assert!(latency["scoreMillis"].as_f64().unwrap() >= 0.0);
        assert_eq!(report["ram"]["kind"], "not_measured");
        assert!(!report["ram"]["reason"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn baseline_timings_are_in_their_units() {
        let started = Instant::now();
        let report = baseline_report(crate::CODING_FIXTURE).await.unwrap();
        let wall_millis = started.elapsed().as_secs_f64() * 1000.0;
        let latency = &report["queryLatency"];
        let score = latency["scoreMillis"].as_f64().unwrap();
        assert!(
            (0.1..=wall_millis).contains(&score),
            "{score} ms of {wall_millis} ms"
        );
        let embed = latency["embedMicrosMedian"].as_f64().unwrap();
        assert!(
            (0.01..=wall_millis * 1000.0).contains(&embed),
            "{embed} µs of {wall_millis} ms"
        );
    }

    #[test]
    fn durations_convert_to_milliseconds_and_microseconds() {
        assert_eq!(millis(Duration::from_micros(1_500)), 1.5);
        assert_eq!(micros(Duration::from_nanos(2_500)), 2.5);
        assert_eq!(millis(Duration::from_secs(2)), 2000.0);
        assert_eq!(micros(Duration::from_millis(3)), 3000.0);
    }

    fn score(id: &str, family: &str, ax: &[&str], abstained: bool) -> QuestionScore {
        QuestionScore {
            id: id.into(),
            family: family.into(),
            channel: "recall".into(),
            recall_at_5: None,
            recall_at_10: None,
            ndcg_at_10: 0.0,
            forbidden_absent: true,
            latest_first: None,
            abstained,
            ax: ax.iter().map(|id| id.to_string()).collect(),
            bm25: Vec::new(),
            bm25_recall_at_5: None,
            block_bytes: None,
        }
    }

    #[test]
    fn family_rules_check_the_top_result_the_top_three_and_abstention() {
        let fixture: CodingFixture = serde_json::from_str(
            r#"{
                "memories": [],
                "questions": [
                    { "id": "named-hit", "family": "named_trigger", "prompt": "p", "channel": "recall", "relevant": ["m1"] },
                    { "id": "named-second", "family": "named_trigger", "prompt": "p", "channel": "recall", "relevant": ["m1"] },
                    { "id": "para-hit", "family": "paraphrase", "prompt": "p", "channel": "recall", "relevant": ["m1"] },
                    { "id": "para-fourth", "family": "paraphrase", "prompt": "p", "channel": "recall", "relevant": ["m1"] },
                    { "id": "quiet", "family": "abstention", "prompt": "p", "channel": "recall" },
                    { "id": "answered", "family": "abstention", "prompt": "p", "channel": "recall" },
                    { "id": "expected", "family": "abstention", "prompt": "p", "channel": "recall", "relevant": ["m1"] }
                ]
            }"#,
        )
        .unwrap();
        let report = CodingReport {
            inserted: Vec::new(),
            questions: vec![
                score("named-hit", "named_trigger", &["m1", "other"], false),
                score("named-second", "named_trigger", &["other", "m1"], false),
                score("para-hit", "paraphrase", &["a", "m1"], false),
                score("para-fourth", "paraphrase", &["a", "b", "c", "m1"], false),
                score("quiet", "abstention", &[], true),
                score("answered", "abstention", &["m2"], false),
                score("expected", "abstention", &[], true),
            ],
        };
        let hits = |family: &str| -> (u64, u64) {
            let summary = family_summary(&fixture, &report, family, "rule");
            (
                summary["hits"].as_u64().unwrap(),
                summary["scored"].as_u64().unwrap(),
            )
        };
        assert_eq!(hits("named_trigger"), (1, 2));
        assert_eq!(hits("paraphrase"), (1, 2));
        assert_eq!(hits("abstention"), (1, 3));
    }

    #[tokio::test]
    async fn production_recall_versus_bm25_on_non_locked_questions() {
        let fixture = ax_memory::bench::load_coding_fixture(crate::CODING_FIXTURE).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let database = ax_db::Database::open(&dir.path().join("recall-mrr.db"))
            .await
            .unwrap();
        let scored = ax_memory::bench::score_coding(database.pool(), &fixture)
            .await
            .unwrap();
        database.pool().close().await;
        let locked = crate::fixture::LOCKED;
        let mut questions = 0u32;
        let mut ax_mrr = 0.0;
        let mut bm25_mrr = 0.0;
        let mut ax_recall = 0.0;
        let mut bm25_recall = 0.0;
        for question in &fixture.questions {
            if locked.contains(&question.family.as_str()) || question.relevant.is_empty() {
                continue;
            }
            let score = scored
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            questions += 1;
            ax_mrr += reciprocal(&question.relevant, &score.ax);
            bm25_mrr += reciprocal(&question.relevant, &score.bm25);
            ax_recall += recall_at_3(&question.relevant, &score.ax);
            bm25_recall += recall_at_3(&question.relevant, &score.bm25);
        }
        let n = f64::from(questions);
        assert_eq!(questions, 12);
        assert!((ax_mrr / n - 2.0 / 3.0).abs() < 1e-12);
        assert!((bm25_mrr / n - 0.625).abs() < 1e-12);
        assert!(ax_mrr > bm25_mrr);
        assert!((ax_recall / n - 2.0 / 3.0).abs() < 1e-12);
        assert_eq!(bm25_recall, ax_recall);
    }

    fn reciprocal(relevant: &[String], ranked: &[String]) -> f64 {
        ranked
            .iter()
            .position(|key| relevant.iter().any(|want| want == key))
            .map(|index| 1.0 / (index as f64 + 1.0))
            .unwrap_or(0.0)
    }

    fn recall_at_3(relevant: &[String], ranked: &[String]) -> f64 {
        let hits = relevant
            .iter()
            .filter(|want| ranked.iter().take(3).any(|got| got == *want))
            .count();
        hits as f64 / relevant.len() as f64
    }
}
