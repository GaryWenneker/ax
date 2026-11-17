use ax_memory::bench::{load_coding_fixture, score_coding, CodingFixture, CodingReport};
use ax_memory::embed::embed_text;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const FAMILIES: [(&str, &str); 4] = [
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
/// Deterministic and sync — no process-global bake-off cache.
pub fn challenger_admitted() -> bool {
    if !crate::runtime_built() {
        return false;
    }
    let Ok(fixture) = load_coding_fixture(crate::CODING_FIXTURE) else {
        return false;
    };
    let baseline = known_hash_v1_family_hits();
    let docs: Vec<crate::lexical::LexicalDoc> = fixture
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
        .collect();
    let questions: Vec<(String, String, String, Vec<String>)> = fixture
        .questions
        .iter()
        .filter(|q| {
            matches!(
                q.family.as_str(),
                "named_trigger" | "paraphrase" | "abstention" | "delivery_budget"
            )
        })
        .map(|q| {
            (
                q.id.clone(),
                q.family.clone(),
                q.prompt.clone(),
                q.relevant.clone(),
            )
        })
        .collect();
    let challenger = crate::lexical::score_locked_families(&docs, &questions);
    crate::lexical::beats_baseline(&baseline, &challenger)
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
    let docs: Vec<crate::lexical::LexicalDoc> = fixture
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
        .collect();
    let questions: Vec<(String, String, String, Vec<String>)> = fixture
        .questions
        .iter()
        .filter(|q| {
            matches!(
                q.family.as_str(),
                "named_trigger" | "paraphrase" | "abstention" | "delivery_budget"
            )
        })
        .map(|q| {
            (
                q.id.clone(),
                q.family.clone(),
                q.prompt.clone(),
                q.relevant.clone(),
            )
        })
        .collect();
    let challenger = crate::lexical::score_locked_families(&docs, &questions);
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
    let micros: Vec<f64> = fixture
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
        "embedMicrosMedian": median(micros),
        "scoreMillis": score_millis
    })
}

fn millis(elapsed: Duration) -> f64 {
    elapsed.as_secs_f64() * 1000.0
}

fn micros(elapsed: Duration) -> f64 {
    elapsed.as_secs_f64() * 1_000_000.0
}

/// The middle value, or the upper of the two middle values; 0 without samples.
fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values.get(values.len() / 2).copied().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_memory::bench::QuestionScore;

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

    #[test]
    fn median_takes_the_middle_sample() {
        assert_eq!(median(vec![3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(vec![4.0, 1.0, 3.0, 2.0]), 3.0);
        assert_eq!(median(Vec::new()), 0.0);
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
        let locked = [
            "named_trigger",
            "paraphrase",
            "abstention",
            "delivery_budget",
        ];
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

    #[tokio::test]
    async fn raw_bench_flag_ablates_the_term_coverage_drop() {
        let _clear = ClearRawEnv;
        std::env::set_var("AX_MEMORY_BENCH_RAW", "1");
        let fixture = ax_memory::bench::load_coding_fixture(crate::CODING_FIXTURE).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let database = ax_db::Database::open(&dir.path().join("recall-raw.db"))
            .await
            .unwrap();
        let scored = ax_memory::bench::score_coding(database.pool(), &fixture)
            .await
            .unwrap();
        database.pool().close().await;
        let locked = [
            "named_trigger",
            "paraphrase",
            "abstention",
            "delivery_budget",
        ];
        let mut questions = 0u32;
        let mut ax_mrr = 0.0;
        let mut ax_recall = 0.0;
        let mut ax_precision = 0.0;
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
            ax_recall += recall_at_3(&question.relevant, &score.ax);
            ax_precision += precision_at_3(&question.relevant, &score.ax);
        }
        let n = f64::from(questions);
        std::env::remove_var("AX_MEMORY_BENCH_RAW");
        let dir_prod = tempfile::tempdir().unwrap();
        let database_prod = ax_db::Database::open(&dir_prod.path().join("recall-prod.db"))
            .await
            .unwrap();
        let scored_prod = ax_memory::bench::score_coding(database_prod.pool(), &fixture)
            .await
            .unwrap();
        database_prod.pool().close().await;
        let mut prod_precision = 0.0;
        for question in &fixture.questions {
            if locked.contains(&question.family.as_str()) || question.relevant.is_empty() {
                continue;
            }
            let score = scored_prod
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            prod_precision += precision_at_3(&question.relevant, &score.ax);
        }
        assert_eq!(questions, 12);
        assert!((ax_mrr / n - 0.75).abs() < 1e-12);
        assert!((ax_recall / n - 0.75).abs() < 1e-12);
        assert!((ax_precision / n - 0.25).abs() < 1e-12);
        assert!((prod_precision / n - 0.2222222222222222).abs() < 1e-12);
        assert!(ax_precision > prod_precision);
    }

    fn precision_at_3(relevant: &[String], ranked: &[String]) -> f64 {
        let hits = ranked
            .iter()
            .take(3)
            .filter(|got| relevant.iter().any(|want| want == *got))
            .count();
        hits as f64 / 3.0
    }

    #[tokio::test]
    async fn skipping_relative_coverage_on_non_locked_questions() {
        let _clear = ClearSkipRelativeEnv;
        std::env::set_var("AX_MEMORY_BENCH_SKIP_RELATIVE_COVERAGE", "1");
        let fixture = ax_memory::bench::load_coding_fixture(crate::CODING_FIXTURE).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let database = ax_db::Database::open(&dir.path().join("recall-relative.db"))
            .await
            .unwrap();
        let scored = ax_memory::bench::score_coding(database.pool(), &fixture)
            .await
            .unwrap();
        database.pool().close().await;
        let locked = [
            "named_trigger",
            "paraphrase",
            "abstention",
            "delivery_budget",
        ];
        let mut questions = 0u32;
        let mut ax_mrr = 0.0;
        let mut ax_recall = 0.0;
        let mut ax_precision = 0.0;
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
            ax_recall += recall_at_3(&question.relevant, &score.ax);
            ax_precision += precision_at_3(&question.relevant, &score.ax);
        }
        let n = f64::from(questions);
        assert_eq!(questions, 12);
        assert!((ax_mrr / n - 2.0 / 3.0).abs() < 1e-12);
        assert!((ax_recall / n - 2.0 / 3.0).abs() < 1e-12);
        assert!((ax_precision / n - 0.2222222222222222).abs() < 1e-12);
    }

    #[tokio::test]
    async fn raw_gain_overlap_classes_on_non_locked_questions() {
        let fixture = ax_memory::bench::load_coding_fixture(crate::CODING_FIXTURE).unwrap();
        let locked = [
            "named_trigger",
            "paraphrase",
            "abstention",
            "delivery_budget",
        ];
        let dir = tempfile::tempdir().unwrap();
        let database = ax_db::Database::open(&dir.path().join("recall-class-prod.db"))
            .await
            .unwrap();
        let scored = ax_memory::bench::score_coding(database.pool(), &fixture)
            .await
            .unwrap();
        database.pool().close().await;

        let _clear = ClearRawEnv;
        std::env::set_var("AX_MEMORY_BENCH_RAW", "1");
        let dir_raw = tempfile::tempdir().unwrap();
        let database_raw = ax_db::Database::open(&dir_raw.path().join("recall-class-raw.db"))
            .await
            .unwrap();
        let scored_raw = ax_memory::bench::score_coding(database_raw.pool(), &fixture)
            .await
            .unwrap();
        database_raw.pool().close().await;

        let mut by_key = std::collections::BTreeMap::new();
        for memory in &fixture.memories {
            by_key.insert(memory.key.as_str(), memory);
        }
        let mut questions = 0u32;
        let mut prod_mrr = 0.0;
        let mut raw_mrr = 0.0;
        let mut counts = std::collections::BTreeMap::<String, u32>::new();
        let mut raw_only_zero = Vec::new();
        for question in &fixture.questions {
            if locked.contains(&question.family.as_str()) || question.relevant.is_empty() {
                continue;
            }
            questions += 1;
            let prod = scored
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            let raw = scored_raw
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            prod_mrr += reciprocal(&question.relevant, &prod.ax);
            raw_mrr += reciprocal(&question.relevant, &raw.ax);
            for key in &question.relevant {
                let memory = by_key.get(key.as_str()).unwrap();
                let overlap = ax_memory::store::content_overlap(
                    &question.prompt,
                    &memory.title,
                    &memory.body,
                );
                let class = overlap_class(overlap.hits, overlap.terms);
                let in_prod = prod.ax.iter().take(3).any(|got| got == key);
                let in_raw = raw.ax.iter().take(3).any(|got| got == key);
                let side = match (in_prod, in_raw) {
                    (true, true) => "both",
                    (false, true) => "raw_only",
                    (true, false) => "prod_only",
                    (false, false) => "neither",
                };
                if side == "raw_only" && class == "zero" {
                    raw_only_zero.push(format!("{}:{key}", question.id));
                }
                *counts.entry(format!("{side}:{class}")).or_insert(0) += 1;
            }
        }
        let n = f64::from(questions);
        assert_eq!(questions, 12);
        assert!((prod_mrr / n - 2.0 / 3.0).abs() < 1e-12);
        assert!((raw_mrr / n - 0.75).abs() < 1e-12);
        assert_eq!(counts.get("both:kept_by_ratio").copied(), Some(7));
        assert_eq!(counts.get("both:zero").copied(), Some(1));
        assert_eq!(counts.get("neither:kept_by_ratio").copied(), Some(1));
        assert_eq!(counts.get("neither:zero").copied(), Some(2));
        assert_eq!(counts.get("raw_only:zero").copied(), Some(1));
        assert_eq!(counts.len(), 5);
        assert_eq!(raw_only_zero, vec!["rename:renamed-symbol".to_string()]);
    }

    fn overlap_class(hits: usize, terms: usize) -> &'static str {
        if hits == 0 {
            "zero"
        } else if hits == 1 && terms >= 4 {
            "one_long"
        } else if hits == 1 {
            "one_short"
        } else if terms > 0 && (hits as f64) / (terms as f64) < 0.25 {
            "two_below_ratio"
        } else {
            "kept_by_ratio"
        }
    }

    #[tokio::test]
    async fn identifier_stem_on_non_locked_questions() {
        let fixture = ax_memory::bench::load_coding_fixture(crate::CODING_FIXTURE).unwrap();
        let locked = [
            "named_trigger",
            "paraphrase",
            "abstention",
            "delivery_budget",
        ];
        let dir = tempfile::tempdir().unwrap();
        let database = ax_db::Database::open(&dir.path().join("recall-stem-prod.db"))
            .await
            .unwrap();
        let scored = ax_memory::bench::score_coding(database.pool(), &fixture)
            .await
            .unwrap();
        database.pool().close().await;

        let _clear = ClearStemEnv;
        std::env::set_var("AX_MEMORY_BENCH_IDENTIFIER_STEM", "1");
        let dir_stem = tempfile::tempdir().unwrap();
        let database_stem = ax_db::Database::open(&dir_stem.path().join("recall-stem.db"))
            .await
            .unwrap();
        let scored_stem = ax_memory::bench::score_coding(database_stem.pool(), &fixture)
            .await
            .unwrap();
        database_stem.pool().close().await;

        let mut questions = 0u32;
        let mut prod_mrr = 0.0;
        let mut prod_recall = 0.0;
        let mut prod_precision = 0.0;
        let mut stem_mrr = 0.0;
        let mut stem_recall = 0.0;
        let mut stem_precision = 0.0;
        for question in &fixture.questions {
            if locked.contains(&question.family.as_str()) || question.relevant.is_empty() {
                continue;
            }
            questions += 1;
            let prod = scored
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            let stem = scored_stem
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            prod_mrr += reciprocal(&question.relevant, &prod.ax);
            prod_recall += recall_at_3(&question.relevant, &prod.ax);
            prod_precision += precision_at_3(&question.relevant, &prod.ax);
            stem_mrr += reciprocal(&question.relevant, &stem.ax);
            stem_recall += recall_at_3(&question.relevant, &stem.ax);
            stem_precision += precision_at_3(&question.relevant, &stem.ax);
        }
        let n = f64::from(questions);
        let rename_prod = scored
            .questions
            .iter()
            .find(|row| row.id == "rename")
            .unwrap();
        let rename_stem = scored_stem
            .questions
            .iter()
            .find(|row| row.id == "rename")
            .unwrap();
        let abstain_prod = scored
            .questions
            .iter()
            .find(|row| row.id == "abstention")
            .unwrap();
        let abstain_stem = scored_stem
            .questions
            .iter()
            .find(|row| row.id == "abstention")
            .unwrap();
        assert_eq!(questions, 12);
        assert!((prod_mrr / n - 2.0 / 3.0).abs() < 1e-12);
        assert!((prod_recall / n - 2.0 / 3.0).abs() < 1e-12);
        assert!((prod_precision / n - 0.2222222222222222).abs() < 1e-12);
        assert!((stem_mrr / n - 0.75).abs() < 1e-12);
        assert!((stem_recall / n - 0.75).abs() < 1e-12);
        assert!((stem_precision / n - 0.25).abs() < 1e-12);
        assert!(stem_mrr > prod_mrr);
        assert!(rename_prod.ax.is_empty());
        assert_eq!(
            rename_stem.ax,
            vec!["renamed-symbol".to_string(), "hop-writer".to_string()]
        );
        assert!(abstain_prod.ax.is_empty());
        assert!(abstain_stem.ax.is_empty());
        for question in &fixture.questions {
            if !locked.contains(&question.family.as_str()) {
                continue;
            }
            let prod = scored
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            let stem = scored_stem
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            assert_eq!(prod.ax, stem.ax, "{}", question.id);
        }
        let mut changed = Vec::new();
        for question in &fixture.questions {
            if locked.contains(&question.family.as_str()) || question.relevant.is_empty() {
                continue;
            }
            let prod = scored
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            let stem = scored_stem
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            if prod.ax != stem.ax {
                changed.push(question.id.clone());
            }
        }
        assert_eq!(changed, vec!["rename".to_string()]);
    }

    #[tokio::test]
    async fn identifier_boundary_on_non_locked_questions() {
        let fixture = ax_memory::bench::load_coding_fixture(crate::CODING_FIXTURE).unwrap();
        let locked = [
            "named_trigger",
            "paraphrase",
            "abstention",
            "delivery_budget",
        ];
        let dir = tempfile::tempdir().unwrap();
        let database = ax_db::Database::open(&dir.path().join("recall-boundary-prod.db"))
            .await
            .unwrap();
        let scored = ax_memory::bench::score_coding(database.pool(), &fixture)
            .await
            .unwrap();
        database.pool().close().await;

        let _clear = ClearBoundaryEnv;
        std::env::set_var("AX_MEMORY_BENCH_IDENTIFIER_BOUNDARY", "1");
        let dir_boundary = tempfile::tempdir().unwrap();
        let database_boundary =
            ax_db::Database::open(&dir_boundary.path().join("recall-boundary.db"))
                .await
                .unwrap();
        let scored_boundary = ax_memory::bench::score_coding(database_boundary.pool(), &fixture)
            .await
            .unwrap();
        database_boundary.pool().close().await;

        let mut questions = 0u32;
        let mut prod_mrr = 0.0;
        let mut prod_recall = 0.0;
        let mut prod_precision = 0.0;
        let mut boundary_mrr = 0.0;
        let mut boundary_recall = 0.0;
        let mut boundary_precision = 0.0;
        let mut changed = Vec::new();
        for question in &fixture.questions {
            if locked.contains(&question.family.as_str()) || question.relevant.is_empty() {
                continue;
            }
            questions += 1;
            let prod = scored
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            let boundary = scored_boundary
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            prod_mrr += reciprocal(&question.relevant, &prod.ax);
            prod_recall += recall_at_3(&question.relevant, &prod.ax);
            prod_precision += precision_at_3(&question.relevant, &prod.ax);
            boundary_mrr += reciprocal(&question.relevant, &boundary.ax);
            boundary_recall += recall_at_3(&question.relevant, &boundary.ax);
            boundary_precision += precision_at_3(&question.relevant, &boundary.ax);
            if prod.ax != boundary.ax {
                changed.push(question.id.clone());
            }
        }
        let n = f64::from(questions);
        let rename_boundary = scored_boundary
            .questions
            .iter()
            .find(|row| row.id == "rename")
            .unwrap();
        let abstain_boundary = scored_boundary
            .questions
            .iter()
            .find(|row| row.id == "abstention")
            .unwrap();
        assert_eq!(questions, 12);
        assert!((prod_mrr / n - 2.0 / 3.0).abs() < 1e-12);
        assert!((prod_recall / n - 2.0 / 3.0).abs() < 1e-12);
        assert!((prod_precision / n - 0.2222222222222222).abs() < 1e-12);
        assert!((boundary_mrr / n - 0.75).abs() < 1e-12);
        assert!((boundary_recall / n - 0.75).abs() < 1e-12);
        assert!((boundary_precision / n - 0.25).abs() < 1e-12);
        assert!(boundary_mrr > prod_mrr);
        assert_eq!(changed, vec!["rename".to_string()]);
        assert_eq!(
            rename_boundary.ax,
            vec!["renamed-symbol".to_string(), "hop-writer".to_string()]
        );
        assert!(abstain_boundary.ax.is_empty());
        for question in &fixture.questions {
            if !locked.contains(&question.family.as_str()) {
                continue;
            }
            let prod = scored
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            let boundary = scored_boundary
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            assert_eq!(prod.ax, boundary.ax, "{}", question.id);
        }
    }

    #[tokio::test]
    async fn identifier_affix_on_non_locked_questions() {
        let fixture = ax_memory::bench::load_coding_fixture(crate::CODING_FIXTURE).unwrap();
        let locked = [
            "named_trigger",
            "paraphrase",
            "abstention",
            "delivery_budget",
        ];
        let dir = tempfile::tempdir().unwrap();
        let database = ax_db::Database::open(&dir.path().join("recall-affix-prod.db"))
            .await
            .unwrap();
        let scored = ax_memory::bench::score_coding(database.pool(), &fixture)
            .await
            .unwrap();
        database.pool().close().await;

        let _clear = ClearAffixEnv;
        std::env::set_var("AX_MEMORY_BENCH_IDENTIFIER_AFFIX", "1");
        let dir_affix = tempfile::tempdir().unwrap();
        let database_affix = ax_db::Database::open(&dir_affix.path().join("recall-affix.db"))
            .await
            .unwrap();
        let scored_affix = ax_memory::bench::score_coding(database_affix.pool(), &fixture)
            .await
            .unwrap();
        database_affix.pool().close().await;

        let mut questions = 0u32;
        let mut prod_mrr = 0.0;
        let mut affix_mrr = 0.0;
        let mut affix_recall = 0.0;
        let mut affix_precision = 0.0;
        let mut changed = Vec::new();
        for question in &fixture.questions {
            if locked.contains(&question.family.as_str()) || question.relevant.is_empty() {
                continue;
            }
            questions += 1;
            let prod = scored
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            let affix = scored_affix
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            prod_mrr += reciprocal(&question.relevant, &prod.ax);
            affix_mrr += reciprocal(&question.relevant, &affix.ax);
            affix_recall += recall_at_3(&question.relevant, &affix.ax);
            affix_precision += precision_at_3(&question.relevant, &affix.ax);
            if prod.ax != affix.ax {
                changed.push(question.id.clone());
            }
        }
        let n = f64::from(questions);
        let rename = scored_affix
            .questions
            .iter()
            .find(|row| row.id == "rename")
            .unwrap();
        let abstain = scored_affix
            .questions
            .iter()
            .find(|row| row.id == "abstention")
            .unwrap();
        assert_eq!(questions, 12);
        assert!(
            (affix_mrr / n - 0.75).abs() < 1e-12,
            "mrr {} recall {} precision {} changed {changed:?} rename {:?} abstain {}",
            affix_mrr / n,
            affix_recall / n,
            affix_precision / n,
            rename.ax,
            abstain.ax.len()
        );
        assert!(
            (affix_recall / n - 0.75).abs() < 1e-12,
            "recall {}",
            affix_recall / n
        );
        assert!(
            (affix_precision / n - 0.25).abs() < 1e-12,
            "precision {}",
            affix_precision / n
        );
        assert!((prod_mrr / n - 2.0 / 3.0).abs() < 1e-12);
        assert!(affix_mrr > prod_mrr);
        assert_eq!(changed, vec!["rename".to_string()]);
        assert_eq!(
            rename.ax,
            vec!["renamed-symbol".to_string(), "hop-writer".to_string()]
        );
        assert!(abstain.ax.is_empty());
        for question in &fixture.questions {
            if !locked.contains(&question.family.as_str()) {
                continue;
            }
            let prod = scored
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            let affix = scored_affix
                .questions
                .iter()
                .find(|row| row.id == question.id)
                .unwrap();
            assert_eq!(prod.ax, affix.ax, "{}", question.id);
        }
    }

    struct ClearAffixEnv;

    impl Drop for ClearAffixEnv {
        fn drop(&mut self) {
            std::env::remove_var("AX_MEMORY_BENCH_IDENTIFIER_AFFIX");
        }
    }

    /// Production `recall_for_prompt` on the existing LoCoMo file.
    /// This does not call `ax_text_rank` and does not change the affix rule.
    #[tokio::test]
    #[ignore = "independent locomo measurement; run explicitly once"]
    async fn locomo_recall_affix_against_production() {
        let _clear = ClearAffixEnv;
        for name in [
            "AX_MEMORY_BENCH_RAW",
            "AX_MEMORY_BENCH_SKIP_RELATIVE_COVERAGE",
            "AX_MEMORY_BENCH_IDENTIFIER_STEM",
            "AX_MEMORY_BENCH_IDENTIFIER_BOUNDARY",
            "AX_MEMORY_BENCH_IDENTIFIER_AFFIX",
        ] {
            std::env::remove_var(name);
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/memory/datasets/locomo10.json");
        let text = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let cases = ax_memory::bench::parse_locomo(&value).unwrap();
        let mut by_sample: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        for (index, case) in cases.iter().enumerate() {
            let sample = case
                .id
                .split_once('#')
                .map(|(sample, _)| sample)
                .unwrap_or(case.id.as_str());
            by_sample.entry(sample.to_string()).or_default().push(index);
        }

        let mut questions = 0u32;
        let mut prod_mrr = 0.0;
        let mut affix_mrr = 0.0;
        let mut prod_recall = 0.0;
        let mut affix_recall = 0.0;
        let mut prod_precision = 0.0;
        let mut affix_precision = 0.0;
        let mut prod_ndcg = 0.0;
        let mut affix_ndcg = 0.0;
        let mut lists_changed = 0u32;
        let mut mrr_up = 0u32;
        let mut mrr_down = 0u32;
        let started = std::time::Instant::now();

        for indexes in by_sample.values() {
            let docs = &cases[indexes[0]].docs;
            for index in indexes {
                assert_eq!(cases[*index].docs.len(), docs.len());
            }
            let dir = tempfile::tempdir().unwrap();
            let database = ax_db::Database::open(&dir.path().join("locomo.db"))
                .await
                .unwrap();
            let pool = database.pool();
            let mut id_to_dia = std::collections::BTreeMap::new();
            for doc in docs {
                let row = ax_memory::remember(
                    pool,
                    ax_memory::RememberInput {
                        title: String::new(),
                        body: doc.text.clone(),
                        kind: Some("note".into()),
                        tags: Vec::new(),
                        files: Vec::new(),
                        source: None,
                    },
                )
                .await
                .unwrap();
                id_to_dia.insert(row.id, doc.id.clone());
            }
            for index in indexes {
                let case = &cases[*index];
                if case.relevant.is_empty() {
                    continue;
                }
                let prod = rank_locomo(pool, &case.prompt, &id_to_dia).await;
                std::env::set_var("AX_MEMORY_BENCH_IDENTIFIER_AFFIX", "1");
                let affix = rank_locomo(pool, &case.prompt, &id_to_dia).await;
                std::env::remove_var("AX_MEMORY_BENCH_IDENTIFIER_AFFIX");
                questions += 1;
                let prod_one = reciprocal(&case.relevant, &prod);
                let affix_one = reciprocal(&case.relevant, &affix);
                prod_mrr += prod_one;
                affix_mrr += affix_one;
                prod_recall += recall_at_3(&case.relevant, &prod);
                affix_recall += recall_at_3(&case.relevant, &affix);
                prod_precision += precision_at_3(&case.relevant, &prod);
                affix_precision += precision_at_3(&case.relevant, &affix);
                prod_ndcg += ax_memory::bench::ndcg_at(&case.relevant, &prod, 10);
                affix_ndcg += ax_memory::bench::ndcg_at(&case.relevant, &affix, 10);
                if prod != affix {
                    lists_changed += 1;
                }
                if affix_one > prod_one {
                    mrr_up += 1;
                } else if affix_one < prod_one {
                    mrr_down += 1;
                }
            }
            pool.close().await;
        }

        let n = f64::from(questions);
        let report = serde_json::json!({
            "dataset": "locomo10",
            "path": "benchmarks/memory/datasets/locomo10.json",
            "ranker": "recall_for_prompt",
            "publicTextRankUnchanged": true,
            "affixRuleEdited": false,
            "questions": questions,
            "samples": by_sample.len(),
            "production": {
                "mrr": prod_mrr / n,
                "recallAt3": prod_recall / n,
                "precisionAt3": prod_precision / n,
                "ndcgAt10": prod_ndcg / n,
            },
            "affix": {
                "mrr": affix_mrr / n,
                "recallAt3": affix_recall / n,
                "precisionAt3": affix_precision / n,
                "ndcgAt10": affix_ndcg / n,
            },
            "listsChanged": lists_changed,
            "mrrUp": mrr_up,
            "mrrDown": mrr_down,
            "elapsedMillis": started.elapsed().as_millis(),
        });
        let out = std::path::Path::new("/tmp/locomo-affix.json");
        std::fs::write(out, report.to_string()).unwrap();
        assert_eq!(questions, 1982);
        assert_eq!(by_sample.len(), 10);
        assert_eq!(lists_changed, 0);
        assert_eq!(mrr_up, 0);
        assert_eq!(mrr_down, 0);
        assert!((prod_mrr / n - 0.28768439767430687).abs() < 1e-12);
        assert!((affix_mrr / n - 0.28768439767430687).abs() < 1e-12);
        assert!((prod_recall / n - 0.3118159030116649).abs() < 1e-12);
        assert!((affix_recall / n - 0.3118159030116649).abs() < 1e-12);
        assert!((prod_precision / n - 0.1177262024890693).abs() < 1e-12);
        assert!((affix_precision / n - 0.1177262024890693).abs() < 1e-12);
        assert!((prod_ndcg / n - 0.3125073748573058).abs() < 1e-12);
        assert!((affix_ndcg / n - 0.3125073748573058).abs() < 1e-12);
    }

    async fn rank_locomo(
        pool: &sqlx::SqlitePool,
        prompt: &str,
        id_to_dia: &std::collections::BTreeMap<String, String>,
    ) -> Vec<String> {
        let matches = ax_memory::recall_for_prompt(pool, prompt, 10)
            .await
            .unwrap();
        matches
            .iter()
            .filter_map(|hit| id_to_dia.get(&hit.memory.id).cloned())
            .collect()
    }

    /// Doc comment to the function it documents, outside the crates this work edited.
    /// The extraction rule was fixed before any score. Signature line only.
    #[tokio::test]
    #[ignore = "independent doc-comment measurement; run explicitly once"]
    async fn doc_comment_affix_against_production() {
        let _clear = ClearAffixEnv;
        for name in [
            "AX_MEMORY_BENCH_RAW",
            "AX_MEMORY_BENCH_SKIP_RELATIVE_COVERAGE",
            "AX_MEMORY_BENCH_IDENTIFIER_STEM",
            "AX_MEMORY_BENCH_IDENTIFIER_BOUNDARY",
            "AX_MEMORY_BENCH_IDENTIFIER_AFFIX",
        ] {
            std::env::remove_var(name);
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pairs = documented_functions(&root);
        assert_eq!(pairs.len(), 617, "extracted {}", pairs.len());
        let dir = tempfile::tempdir().unwrap();
        let database = ax_db::Database::open(&dir.path().join("docfn.db"))
            .await
            .unwrap();
        let pool = database.pool();
        let mut id_to_dia = std::collections::BTreeMap::new();
        for (id, _query, signature) in &pairs {
            let row = ax_memory::bench::remember_stable(
                pool,
                id,
                ax_memory::RememberInput {
                    title: String::new(),
                    body: signature.clone(),
                    kind: Some("note".into()),
                    tags: Vec::new(),
                    files: Vec::new(),
                    source: None,
                },
            )
            .await
            .unwrap();
            id_to_dia.insert(row.id, id.clone());
        }
        let mut questions = 0u32;
        let mut prod_mrr = 0.0;
        let mut affix_mrr = 0.0;
        let mut prod_recall = 0.0;
        let mut affix_recall = 0.0;
        let mut prod_precision = 0.0;
        let mut affix_precision = 0.0;
        let mut prod_ndcg = 0.0;
        let mut affix_ndcg = 0.0;
        let mut lists_changed = 0u32;
        let mut mrr_up = 0u32;
        let mut mrr_down = 0u32;
        let signatures: std::collections::BTreeMap<String, String> = pairs
            .iter()
            .map(|(id, _, signature)| (id.clone(), signature.clone()))
            .collect();
        let mut mrr_changes = Vec::new();
        let started = std::time::Instant::now();
        for (id, query, signature) in &pairs {
            let prod = rank_locomo(pool, query, &id_to_dia).await;
            std::env::set_var("AX_MEMORY_BENCH_IDENTIFIER_AFFIX", "1");
            let affix = rank_locomo(pool, query, &id_to_dia).await;
            std::env::remove_var("AX_MEMORY_BENCH_IDENTIFIER_AFFIX");
            questions += 1;
            let relevant = [id.clone()];
            let prod_one = reciprocal(&relevant, &prod);
            let affix_one = reciprocal(&relevant, &affix);
            prod_mrr += prod_one;
            affix_mrr += affix_one;
            prod_recall += recall_at_3(&relevant, &prod);
            affix_recall += recall_at_3(&relevant, &affix);
            prod_precision += precision_at_3(&relevant, &prod);
            affix_precision += precision_at_3(&relevant, &affix);
            prod_ndcg += ax_memory::bench::ndcg_at(&relevant, &prod, 10);
            affix_ndcg += ax_memory::bench::ndcg_at(&relevant, &affix, 10);
            if prod != affix {
                lists_changed += 1;
            }
            if affix_one != prod_one {
                let describe = |ranked: &[String]| {
                    ranked
                        .iter()
                        .map(|item| {
                            serde_json::json!({
                                "id": item,
                                "signature": signatures.get(item).cloned().unwrap_or_default(),
                            })
                        })
                        .collect::<Vec<_>>()
                };
                mrr_changes.push(serde_json::json!({
                    "id": id,
                    "direction": if affix_one > prod_one { "gain" } else { "loss" },
                    "productionRank": prod.iter().position(|item| item == id).map(|index| index + 1),
                    "affixRank": affix.iter().position(|item| item == id).map(|index| index + 1),
                    "productionReciprocal": prod_one,
                    "affixReciprocal": affix_one,
                    "query": query,
                    "signature": signature,
                    "production": describe(&prod),
                    "affix": describe(&affix),
                }));
            }
            if affix_one > prod_one {
                mrr_up += 1;
            } else if affix_one < prod_one {
                mrr_down += 1;
            }
        }
        pool.close().await;
        let n = f64::from(questions);
        let report = serde_json::json!({
            "dataset": "doc-comments",
            "excludedCrates": ["ax-memory", "ax-neural"],
            "document": "signature line",
            "minimumDocChars": 40,
            "ranker": "recall_for_prompt",
            "affixRuleEdited": false,
            "questions": questions,
            "listsChanged": lists_changed,
            "mrrUp": mrr_up,
            "mrrDown": mrr_down,
            "production": {
                "mrr": prod_mrr / n,
                "recallAt3": prod_recall / n,
                "precisionAt3": prod_precision / n,
                "ndcgAt10": prod_ndcg / n,
            },
            "affix": {
                "mrr": affix_mrr / n,
                "recallAt3": affix_recall / n,
                "precisionAt3": affix_precision / n,
                "ndcgAt10": affix_ndcg / n,
            },
            "elapsedMillis": started.elapsed().as_millis(),
        });
        std::fs::write("/tmp/docfn-affix.json", report.to_string()).unwrap();
        std::fs::write(
            "/tmp/doc-comment-mrr-changes.json",
            serde_json::to_string_pretty(&mrr_changes).unwrap(),
        )
        .unwrap();
        assert_eq!(mrr_changes.len(), 15);
        assert_eq!(questions, 617);
        assert_eq!(lists_changed, 55);
        assert_eq!(mrr_up, 12);
        assert_eq!(mrr_down, 3);
        assert!(
            (prod_mrr / n - 0.17747420956497137).abs() < 1e-12,
            "production mrr {}",
            prod_mrr / n
        );
        assert!(
            (affix_mrr / n - 0.182903707133853).abs() < 1e-12,
            "affix mrr {}",
            affix_mrr / n
        );
        assert!((prod_recall / n - 0.22204213938411668).abs() < 1e-12);
        assert!((affix_recall / n - 0.23014586709886548).abs() < 1e-12);
        assert!((prod_precision / n - 0.0740140464613723).abs() < 1e-12);
        assert!((affix_precision / n - 0.07671528903295526).abs() < 1e-12);
        assert!((prod_ndcg / n - 0.22491833056902782).abs() < 1e-12);
        assert!((affix_ndcg / n - 0.23293687541000846).abs() < 1e-12);
    }

    fn documented_functions(root: &std::path::Path) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        let crates_dir = root.join("crates");
        let mut stack = vec![crates_dir.clone()];
        while let Some(dir) = stack.pop() {
            let name = dir.file_name().and_then(|item| item.to_str()).unwrap_or("");
            if dir.parent() == Some(crates_dir.as_path())
                && (name == "ax-memory" || name == "ax-neural")
            {
                continue;
            }
            let entries = match std::fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let mut doc: Vec<String> = Vec::new();
                for (index, line) in text.lines().enumerate() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("///") {
                        doc.push(trimmed[3..].trim().to_string());
                        continue;
                    }
                    if trimmed.starts_with('#') || trimmed.starts_with("//") || trimmed.is_empty()
                    {
                        if trimmed.starts_with("//") && !trimmed.starts_with("///") {
                            doc.clear();
                        }
                        continue;
                    }
                    if !doc.is_empty()
                        && (trimmed.starts_with("pub fn ") || trimmed.starts_with("pub async fn "))
                    {
                        let query = doc.join(" ");
                        if query.chars().count() >= 40 {
                            let rel = path
                                .strip_prefix(root)
                                .unwrap_or(&path)
                                .to_string_lossy()
                                .replace('\\', "/");
                            out.push((
                                format!("{rel}:{}", index + 1),
                                query,
                                trimmed.to_string(),
                            ));
                        }
                    }
                    doc.clear();
                }
            }
        }
        out.sort_by(|left, right| left.0.cmp(&right.0));
        out
    }

    struct ClearBoundaryEnv;

    impl Drop for ClearBoundaryEnv {
        fn drop(&mut self) {
            std::env::remove_var("AX_MEMORY_BENCH_IDENTIFIER_BOUNDARY");
        }
    }

    struct ClearStemEnv;

    impl Drop for ClearStemEnv {
        fn drop(&mut self) {
            std::env::remove_var("AX_MEMORY_BENCH_IDENTIFIER_STEM");
        }
    }

    struct ClearSkipRelativeEnv;

    impl Drop for ClearSkipRelativeEnv {
        fn drop(&mut self) {
            std::env::remove_var("AX_MEMORY_BENCH_SKIP_RELATIVE_COVERAGE");
        }
    }

    struct ClearRawEnv;

    impl Drop for ClearRawEnv {
        fn drop(&mut self) {
            std::env::remove_var("AX_MEMORY_BENCH_RAW");
        }
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
