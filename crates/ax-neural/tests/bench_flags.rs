//! Memory bench ablation tests that switch `AX_MEMORY_BENCH_*` flags.
//!
//! `ax_memory` reads these flags from the process environment at call time,
//! so a test that sets one changes the result of every recall running at the
//! same moment. They live in their own test binary, away from the library
//! tests that assert exact recall metrics, and each one holds `BENCH_ENV` for
//! its whole run so they never overlap with each other either.

use tokio::sync::{Mutex, MutexGuard};

static BENCH_ENV: Mutex<()> = Mutex::const_new(());

/// Serialize environment-switching tests within this binary.
async fn bench_env() -> MutexGuard<'static, ()> {
    BENCH_ENV.lock().await
}

#[tokio::test]
async fn raw_bench_flag_ablates_the_term_coverage_drop() {
    let _env = bench_env().await;
    let _clear = ClearRawEnv;
    std::env::set_var("AX_MEMORY_BENCH_RAW", "1");
    let fixture = ax_memory::bench::load_coding_fixture(ax_neural::CODING_FIXTURE).unwrap();
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
    let _env = bench_env().await;
    let _clear = ClearSkipRelativeEnv;
    std::env::set_var("AX_MEMORY_BENCH_SKIP_RELATIVE_COVERAGE", "1");
    let fixture = ax_memory::bench::load_coding_fixture(ax_neural::CODING_FIXTURE).unwrap();
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
    let _env = bench_env().await;
    let fixture = ax_memory::bench::load_coding_fixture(ax_neural::CODING_FIXTURE).unwrap();
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
            let overlap =
                ax_memory::store::content_overlap(&question.prompt, &memory.title, &memory.body);
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
    let _env = bench_env().await;
    let fixture = ax_memory::bench::load_coding_fixture(ax_neural::CODING_FIXTURE).unwrap();
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
    let _env = bench_env().await;
    let fixture = ax_memory::bench::load_coding_fixture(ax_neural::CODING_FIXTURE).unwrap();
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
    let database_boundary = ax_db::Database::open(&dir_boundary.path().join("recall-boundary.db"))
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
    let _env = bench_env().await;
    let fixture = ax_memory::bench::load_coding_fixture(ax_neural::CODING_FIXTURE).unwrap();
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
    let _env = bench_env().await;
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
    let _env = bench_env().await;
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
                if let Some(stripped) = trimmed.strip_prefix("///") {
                    doc.push(stripped.trim().to_string());
                    continue;
                }
                if trimmed.starts_with('#') || trimmed.starts_with("//") || trimmed.is_empty() {
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
                        out.push((format!("{rel}:{}", index + 1), query, trimmed.to_string()));
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
