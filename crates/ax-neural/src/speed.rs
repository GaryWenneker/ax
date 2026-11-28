//! Neural modules against the old hash and recall path. No language model.

use crate::fixture::median;
use crate::graph::{suggest_report, InputEdge};
use crate::learning::{validate_feedback, FeedbackInput};
use crate::lexical::{rank, LexicalDoc};
use crate::predict::{predict, Observation};
use crate::rank::{keep_order, plan, RankClass, RankItem};
use ax_memory::embed::{cosine, embed_text};
use std::hint::black_box;
use std::time::Instant;

const DOCS: usize = 80;
const QUERIES: usize = 40;
const EMBEDS: usize = 200;
const PASSES: usize = 40;

fn text_for(index: usize) -> String {
    format!("module alpha{index} keeps ledger row {index} stable under replay")
}

fn time_ms(mut work: impl FnMut()) -> f64 {
    let started = Instant::now();
    work();
    started.elapsed().as_secs_f64() * 1000.0
}

/// Paired fronts: embed, index, query, block rank, predict, graph, learn.
pub struct Fronts {
    pub embed_old_ms: f64,
    pub embed_neural_ms: f64,
    pub index_old_ms: f64,
    pub index_neural_ms: f64,
    pub query_old_ms: f64,
    pub query_neural_ms: f64,
    pub blocks_old_ms: f64,
    pub blocks_neural_ms: f64,
    pub predict_old_ms: f64,
    pub predict_neural_ms: f64,
    pub graph_old_ms: f64,
    pub graph_neural_ms: f64,
    pub learn_old_ms: f64,
    pub learn_neural_ms: f64,
}

pub async fn measure_fronts() -> Fronts {
    let mut embed_old = Vec::new();
    let mut embed_neural = Vec::new();
    for _ in 0..5 {
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
    let mut index_neural = Vec::new();
    for _ in 0..3 {
        index_neural.push(time_ms(|| {
            let docs: Vec<LexicalDoc> = (0..DOCS)
                .map(|index| LexicalDoc::new(format!("m{index}"), "", &text_for(index), &[]))
                .collect();
            black_box(docs.last().map(|doc| doc.key.clone()));
        }));
    }
    let started = Instant::now();
    for index in 0..DOCS {
        ax_memory::bench::remember_stable(
            pool,
            &format!("m{index}"),
            ax_memory::RememberInput {
                title: String::new(),
                body: text_for(index),
                kind: Some("note".into()),
                tags: Vec::new(),
                files: Vec::new(),
                source: None,
            },
        )
        .await
        .unwrap();
    }
    let mut index_old = vec![started.elapsed().as_secs_f64() * 1000.0];

    let docs: Vec<LexicalDoc> = (0..DOCS)
        .map(|index| LexicalDoc::new(format!("m{index}"), "", &text_for(index), &[]))
        .collect();
    let mut query_old = Vec::new();
    let mut query_neural = Vec::new();
    for _ in 0..5 {
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
    let feedback: Vec<FeedbackInput> = (0..QUERIES)
        .map(|index| FeedbackInput {
            project_id: "speed".into(),
            execution_id: format!("exec{index}"),
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

    let mut blocks_old = Vec::new();
    let mut blocks_neural = Vec::new();
    let mut predict_old = Vec::new();
    let mut predict_neural = Vec::new();
    let mut graph_old = Vec::new();
    let mut graph_neural = Vec::new();
    let mut learn_neural = Vec::new();
    for _ in 0..5 {
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
        learn_neural.push(time_ms(|| {
            for input in &feedback {
                black_box(validate_feedback(input).expect("feedback"));
            }
        }));
    }
    let started = Instant::now();
    for (index, input) in feedback.iter().enumerate() {
        ax_memory::bench::remember_stable(
            pool,
            &format!("f{index}"),
            ax_memory::RememberInput {
                title: input.task_category.clone(),
                body: format!("{} {}", input.execution_id, input.context_ids.join(" ")),
                kind: Some("note".into()),
                tags: Vec::new(),
                files: Vec::new(),
                source: None,
            },
        )
        .await
        .unwrap();
    }
    let mut learn_old = vec![started.elapsed().as_secs_f64() * 1000.0];

    Fronts {
        embed_old_ms: median(&mut embed_old),
        embed_neural_ms: median(&mut embed_neural),
        index_old_ms: median(&mut index_old),
        index_neural_ms: median(&mut index_neural),
        query_old_ms: median(&mut query_old),
        query_neural_ms: median(&mut query_neural),
        blocks_old_ms: median(&mut blocks_old),
        blocks_neural_ms: median(&mut blocks_neural),
        predict_old_ms: median(&mut predict_old),
        predict_neural_ms: median(&mut predict_neural),
        graph_old_ms: median(&mut graph_old),
        graph_neural_ms: median(&mut graph_neural),
        learn_old_ms: median(&mut learn_old),
        learn_neural_ms: median(&mut learn_neural),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn neural_modules_against_the_old_path() {
        let fronts = measure_fronts().await;
        let report = serde_json::json!({
            "embed": { "old": fronts.embed_old_ms, "neural": fronts.embed_neural_ms },
            "index": { "old": fronts.index_old_ms, "neural": fronts.index_neural_ms },
            "query": { "old": fronts.query_old_ms, "neural": fronts.query_neural_ms },
            "blocks": { "old": fronts.blocks_old_ms, "neural": fronts.blocks_neural_ms },
            "predict": { "old": fronts.predict_old_ms, "neural": fronts.predict_neural_ms },
            "graph": { "old": fronts.graph_old_ms, "neural": fronts.graph_neural_ms },
            "learn": { "old": fronts.learn_old_ms, "neural": fronts.learn_neural_ms }
        });
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/specs/ax-neural/SPEED-FRONTS.json");
        std::fs::write(path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        assert!(
            fronts.embed_neural_ms < fronts.embed_old_ms,
            "embed neural {} old {}",
            fronts.embed_neural_ms,
            fronts.embed_old_ms
        );
        assert!(
            fronts.index_neural_ms < fronts.index_old_ms,
            "index neural {} old {}",
            fronts.index_neural_ms,
            fronts.index_old_ms
        );
        assert!(
            fronts.query_neural_ms < fronts.query_old_ms,
            "query neural {} old {}",
            fronts.query_neural_ms,
            fronts.query_old_ms
        );
        assert!(
            fronts.blocks_neural_ms < fronts.blocks_old_ms,
            "blocks neural {} old {}",
            fronts.blocks_neural_ms,
            fronts.blocks_old_ms
        );
        assert!(
            fronts.predict_neural_ms < fronts.predict_old_ms,
            "predict neural {} old {}",
            fronts.predict_neural_ms,
            fronts.predict_old_ms
        );
        assert!(
            fronts.graph_neural_ms < fronts.graph_old_ms,
            "graph neural {} old {}",
            fronts.graph_neural_ms,
            fronts.graph_old_ms
        );
        assert!(
            fronts.learn_neural_ms < fronts.learn_old_ms,
            "learn neural {} old {}",
            fronts.learn_neural_ms,
            fronts.learn_old_ms
        );
    }
}
