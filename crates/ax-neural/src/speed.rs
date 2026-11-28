//! Neural modules against the old hash and recall path. No language model.
//!
//! Test-only measurement harness: every front takes the same number of samples
//! on both sides, and each front states whether the two sides do the same work.
//! Only same-work fronts are asserted as a speedup.

use crate::fixture::median;
use crate::graph::{suggest_report, InputEdge};
use crate::learning::{validate_feedback, FeedbackInput};
use crate::lexical::{rank, LexicalDoc};
use crate::predict::{predict, Observation};
use crate::rank::{keep_order, plan, RankClass, RankItem};
use ax_memory::embed::{cosine, embed_text};
use serde_json::{json, Value};
use std::hint::black_box;
use std::time::Instant;

const DOCS: usize = 80;
const QUERIES: usize = 40;
const EMBEDS: usize = 200;
const PASSES: usize = 40;
/// Samples per side for every front.
pub const SAMPLES: usize = 5;

/// What the two sides of a front compare.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Basis {
    /// Both sides do the same work (both in memory, or both writing SQLite).
    SameWork,
    /// The old side reads or writes SQLite while the neural side works in memory.
    DifferentWork,
    /// The old code had no path for this task; the old side is a stand-in
    /// (hash embedding plus cosine).
    NoOldEquivalent,
}

impl Basis {
    pub fn label(self) -> &'static str {
        match self {
            Basis::SameWork => "same_work",
            Basis::DifferentWork => "different_work",
            Basis::NoOldEquivalent => "no_old_equivalent",
        }
    }
}

/// One paired measurement with its raw samples in milliseconds.
pub struct Front {
    pub name: &'static str,
    pub basis: Basis,
    pub note: &'static str,
    pub old_ms: Vec<f64>,
    pub neural_ms: Vec<f64>,
}

impl Front {
    pub fn old_median(&self) -> f64 {
        median(&mut self.old_ms.clone())
    }

    pub fn neural_median(&self) -> f64 {
        median(&mut self.neural_ms.clone())
    }
}

/// The JSON report: medians, sample counts, basis and note per front.
pub fn report(fronts: &[Front]) -> Value {
    let mut out = serde_json::Map::new();
    for front in fronts {
        out.insert(
            front.name.to_string(),
            json!({
                "old": front.old_median(),
                "neural": front.neural_median(),
                "oldSamples": front.old_ms.len(),
                "neuralSamples": front.neural_ms.len(),
                "basis": front.basis.label(),
                "note": front.note,
            }),
        );
    }
    Value::Object(out)
}

fn text_for(index: usize) -> String {
    format!("module alpha{index} keeps ledger row {index} stable under replay")
}

fn time_ms(mut work: impl FnMut()) -> f64 {
    let started = Instant::now();
    work();
    started.elapsed().as_secs_f64() * 1000.0
}

fn memory_input(title: String, body: String) -> ax_memory::RememberInput {
    ax_memory::RememberInput {
        title,
        body,
        kind: Some("note".into()),
        tags: Vec::new(),
        files: Vec::new(),
        source: None,
    }
}

/// Runs every front. Paired fronts: embed, index, query, block rank, predict, graph, learn.
pub async fn measure_fronts() -> Vec<Front> {
    let mut embed_old = Vec::new();
    let mut embed_neural = Vec::new();
    for _ in 0..SAMPLES {
        embed_old.push(time_ms(|| {
            for index in 0..EMBEDS {
                black_box(embed_text(&text_for(index % DOCS)));
            }
        }));
        embed_neural.push(time_ms(|| {
            for index in 0..EMBEDS {
                black_box(LexicalDoc::new("m", "", &text_for(index % DOCS), &[]));
            }
        }));
    }

    let dir = tempfile::tempdir().unwrap();
    let database = ax_db::Database::open(&dir.path().join("speed.db"))
        .await
        .unwrap();
    let pool = database.pool();
    // Index samples write into their own database so the query front sees DOCS rows.
    let index_database = ax_db::Database::open(&dir.path().join("index.db"))
        .await
        .unwrap();
    let mut index_old = Vec::new();
    let mut index_neural = Vec::new();
    for sample in 0..SAMPLES {
        let started = Instant::now();
        for index in 0..DOCS {
            ax_memory::bench::remember_stable(
                index_database.pool(),
                &format!("s{sample}m{index}"),
                memory_input(String::new(), text_for(index)),
            )
            .await
            .unwrap();
        }
        index_old.push(started.elapsed().as_secs_f64() * 1000.0);
        index_neural.push(time_ms(|| {
            let docs: Vec<LexicalDoc> = (0..DOCS)
                .map(|index| LexicalDoc::new(format!("m{index}"), "", &text_for(index), &[]))
                .collect();
            black_box(docs.last().map(|doc| doc.key.clone()));
        }));
    }
    for index in 0..DOCS {
        ax_memory::bench::remember_stable(
            pool,
            &format!("m{index}"),
            memory_input(String::new(), text_for(index)),
        )
        .await
        .unwrap();
    }

    let docs: Vec<LexicalDoc> = (0..DOCS)
        .map(|index| LexicalDoc::new(format!("m{index}"), "", &text_for(index), &[]))
        .collect();
    let mut query_old = Vec::new();
    let mut query_neural = Vec::new();
    for _ in 0..SAMPLES {
        let started = Instant::now();
        for index in 0..QUERIES {
            let hits = ax_memory::recall_for_prompt(pool, &text_for(index % DOCS), 5)
                .await
                .unwrap();
            black_box(hits.len());
        }
        query_old.push(started.elapsed().as_secs_f64() * 1000.0);
        query_neural.push(time_ms(|| {
            for index in 0..QUERIES {
                let (hits, abstained) = rank(&docs, &text_for(index % DOCS));
                black_box((hits.len(), abstained));
            }
        }));
    }

    let block_query = "module alpha keeps ledger row stable";
    let block_items: Vec<RankItem> = (0..DOCS)
        .map(|index| RankItem {
            id: format!("m{index}"),
            class: if index % 5 == 0 {
                RankClass::HardRequired
            } else {
                RankClass::HighValue
            },
            text: text_for(index),
        })
        .collect();
    let observations: Vec<Observation> = (0..DOCS)
        .map(|index| Observation {
            task_category: "fix".into(),
            skills: vec![format!("skill{index}")],
            rules: vec![format!("rule{index}")],
            context_ids: vec![format!("mem{index}")],
        })
        .collect();
    let edges: Vec<InputEdge> = (0..DOCS)
        .map(|index| InputEdge {
            source: format!("n{index}"),
            target: format!("n{}", (index + 1) % DOCS),
        })
        .collect();
    let co_occurrence: Vec<Vec<String>> = (0..20)
        .map(|group| {
            (0..4)
                .map(|slot| format!("n{}", group * 4 + slot))
                .collect()
        })
        .collect();

    let mut blocks_old = Vec::new();
    let mut blocks_neural = Vec::new();
    let mut predict_old = Vec::new();
    let mut predict_neural = Vec::new();
    let mut graph_old = Vec::new();
    let mut graph_neural = Vec::new();
    for _ in 0..SAMPLES {
        blocks_old.push(time_ms(|| {
            for _ in 0..PASSES {
                let query = embed_text(block_query);
                for item in &block_items {
                    black_box(cosine(&query, &embed_text(&item.text)));
                }
            }
        }));
        blocks_neural.push(time_ms(|| {
            for _ in 0..PASSES {
                let notes = plan(&block_items, block_query);
                black_box(keep_order(&block_items, &notes));
            }
        }));
        predict_old.push(time_ms(|| {
            for _ in 0..PASSES {
                let query = embed_text("fix the parser");
                for row in &observations {
                    let text = format!(
                        "{} {} {}",
                        row.task_category,
                        row.skills.join(" "),
                        row.context_ids.join(" ")
                    );
                    black_box(cosine(&query, &embed_text(&text)));
                }
            }
        }));
        predict_neural.push(time_ms(|| {
            for _ in 0..PASSES {
                black_box(predict(&observations, "fix the parser", 0.2));
            }
        }));
        graph_old.push(time_ms(|| {
            for _ in 0..PASSES {
                for edge in &edges {
                    black_box(cosine(&embed_text(&edge.source), &embed_text(&edge.target)));
                }
            }
        }));
        graph_neural.push(time_ms(|| {
            for _ in 0..PASSES {
                black_box(suggest_report(&edges, &co_occurrence));
            }
        }));
    }

    // Learn: both sides persist one SQLite row per feedback item.
    let mut learn_old = Vec::new();
    let mut learn_neural = Vec::new();
    for sample in 0..SAMPLES {
        let feedback: Vec<FeedbackInput> = (0..QUERIES)
            .map(|index| FeedbackInput {
                project_id: "speed".into(),
                execution_id: format!("exec{sample}x{index}"),
                task_category: "fix".into(),
                context_ids: vec![format!("mem{index}")],
                skill_ids: vec![format!("skill{index}")],
                rule_ids: vec![format!("rule{index}")],
                outcome: "success".into(),
                retry_count: 0,
                input_tokens: 12,
                output_tokens: 4,
                source_text: String::new(),
            })
            .collect();
        let started = Instant::now();
        for (index, input) in feedback.iter().enumerate() {
            ax_memory::bench::remember_stable(
                pool,
                &format!("s{sample}f{index}"),
                memory_input(
                    input.task_category.clone(),
                    format!("{} {}", input.execution_id, input.context_ids.join(" ")),
                ),
            )
            .await
            .unwrap();
        }
        learn_old.push(started.elapsed().as_secs_f64() * 1000.0);
        let started = Instant::now();
        for input in &feedback {
            let row = validate_feedback(input).expect("feedback");
            crate::store::insert_feedback(pool, &row).await.unwrap();
        }
        learn_neural.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    let stand_in = "the old code had no path for this task; old side is hash embed plus cosine";
    vec![
        Front {
            name: "embed",
            basis: Basis::SameWork,
            note: "both sides build a per-text representation in memory",
            old_ms: embed_old,
            neural_ms: embed_neural,
        },
        Front {
            name: "index",
            basis: Basis::DifferentWork,
            note: "old writes SQLite memory rows; neural builds lexical docs in memory",
            old_ms: index_old,
            neural_ms: index_neural,
        },
        Front {
            name: "query",
            basis: Basis::DifferentWork,
            note: "old recalls from SQLite; neural ranks prebuilt lexical docs in memory",
            old_ms: query_old,
            neural_ms: query_neural,
        },
        Front {
            name: "blocks",
            basis: Basis::NoOldEquivalent,
            note: stand_in,
            old_ms: blocks_old,
            neural_ms: blocks_neural,
        },
        Front {
            name: "predict",
            basis: Basis::NoOldEquivalent,
            note: stand_in,
            old_ms: predict_old,
            neural_ms: predict_neural,
        },
        Front {
            name: "graph",
            basis: Basis::NoOldEquivalent,
            note: stand_in,
            old_ms: graph_old,
            neural_ms: graph_neural,
        },
        Front {
            name: "learn",
            basis: Basis::SameWork,
            note: "both sides write one SQLite row per feedback item",
            old_ms: learn_old,
            neural_ms: learn_neural,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_carries_basis_and_sample_counts() {
        let fronts = vec![
            Front {
                name: "embed",
                basis: Basis::SameWork,
                note: "n",
                old_ms: vec![3.0, 1.0, 2.0],
                neural_ms: vec![0.5, 0.7, 0.6],
            },
            Front {
                name: "graph",
                basis: Basis::NoOldEquivalent,
                note: "stand-in",
                old_ms: vec![1.0],
                neural_ms: vec![2.0],
            },
        ];
        let value = report(&fronts);
        assert_eq!(value["embed"]["old"], 2.0);
        assert_eq!(value["embed"]["neural"], 0.6);
        assert_eq!(value["embed"]["oldSamples"], 3);
        assert_eq!(value["embed"]["basis"], "same_work");
        assert_eq!(value["graph"]["basis"], "no_old_equivalent");
        assert_eq!(value["graph"]["note"], "stand-in");
    }

    #[tokio::test]
    #[ignore]
    async fn neural_modules_against_the_old_path() {
        let fronts = measure_fronts().await;
        let report = report(&fronts);
        let text = serde_json::to_string_pretty(&report).unwrap();
        println!("{text}");
        std::fs::write(crate::measurement_output("SPEED-FRONTS.json"), text).unwrap();
        for front in &fronts {
            assert_eq!(front.old_ms.len(), SAMPLES, "{} old samples", front.name);
            assert_eq!(
                front.neural_ms.len(),
                SAMPLES,
                "{} neural samples",
                front.name
            );
            if front.basis == Basis::SameWork {
                assert!(
                    front.neural_median() < front.old_median(),
                    "{} neural {} old {}",
                    front.name,
                    front.neural_median(),
                    front.old_median()
                );
            }
        }
    }
}
