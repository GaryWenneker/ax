//! Neural ranking on the preflight block list.
//! The layer is required, so a project without ax.json still ranks.

use std::path::Path;
use std::time::{Duration, Instant};

use ax_neural::rank::{keep_order, plan, RankClass, RankItem, RankNote};
use ax_usage::{ContextBlock, ContextClass};
use sqlx::SqlitePool;

use crate::context_budget::NeuralBudget;

pub const PREDICTED_BLOCK: &str = "neural:predicted";
const MAX_PREDICTIONS: usize = 8;
const TOTALS_TIMEOUT: Duration = Duration::from_millis(250);

pub struct RankOutcome {
    pub notes: Vec<RankNote>,
    pub budget: NeuralBudget,
    pub micros: i64,
}

pub fn maybe_reorder(
    root: &Path,
    query: &str,
    blocks: &mut Vec<ContextBlock>,
) -> Option<RankOutcome> {
    if !ax_neural::load(root).ranking_active() {
        return None;
    }
    let started = Instant::now();
    let items: Vec<RankItem> = blocks
        .iter()
        .map(|block| RankItem {
            id: block.id.clone(),
            class: match block.class {
                ContextClass::HardRequired => RankClass::HardRequired,
                ContextClass::HighValue => RankClass::HighValue,
                ContextClass::Optional => RankClass::Optional,
            },
            text: block.text.clone(),
        })
        .collect();
    let notes = plan(&items, query);
    let order = keep_order(&items, &notes);
    let mut next = Vec::new();
    for id in &order {
        if let Some(block) = blocks.iter().find(|block| &block.id == id) {
            next.push(block.clone());
        }
    }
    for block in blocks.iter() {
        if block.class == ContextClass::HardRequired && !next.iter().any(|kept| kept.id == block.id)
        {
            next.insert(0, block.clone());
        }
    }
    let budget = duplicate_budget(&items, &notes);
    *blocks = next;
    Some(RankOutcome {
        notes,
        budget,
        micros: i64::try_from(started.elapsed().as_micros()).unwrap_or(i64::MAX),
    })
}

fn duplicate_budget(items: &[RankItem], notes: &[RankNote]) -> NeuralBudget {
    let mut budget = NeuralBudget::default();
    for note in notes.iter().filter(|note| note.reason == "duplicate") {
        if let Some(item) = items.iter().find(|item| item.id == note.id) {
            budget.duplicate_tokens += ax_usage::count_tokens(&item.text);
        }
        budget.duplicates.push(note.id.clone());
    }
    budget
}

/// Predicted skills, rules, memories, and files for the prompt, from this project's feedback.
/// Optional, so the budget may drop it. Nothing when prediction is off or nothing is confident.
pub async fn predicted_block(root: &Path, pool: &SqlitePool, prompt: &str) -> Option<ContextBlock> {
    let cfg = ax_neural::load(root);
    if !cfg.prediction_active() {
        return None;
    }
    let project = ax_neural::dispatch::adopted_project_id(pool, root).await;
    let rows = match ax_neural::store::observations(pool, &project).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!("neural predictions unavailable: {error}");
            return None;
        }
    };
    let predictions = ax_neural::predict::predict(&rows, prompt, cfg.minimum_confidence);
    if predictions.is_empty() {
        return None;
    }
    let lines: Vec<String> = predictions
        .iter()
        .take(MAX_PREDICTIONS)
        .map(|item| {
            format!(
                "- {} {} (confidence {:.2}, {})",
                item.kind, item.target_id, item.confidence, item.label
            )
        })
        .collect();
    Some(ContextBlock {
        id: PREDICTED_BLOCK.into(),
        class: ContextClass::Optional,
        text: format!(
            "## Predicted\n\nLearned from past tasks in this project. Not policy; verify before relying on it.\n\n{}",
            lines.join("\n")
        ),
    })
}

/// Adds this preflight to the project's rank totals. Awaited, because a CLI process ends
/// as soon as preflight returns; bounded, so a busy database cannot hold the reply.
pub async fn record_totals(pool: &SqlitePool, root: &Path, outcome: &RankOutcome) {
    let blocks = i64::try_from(outcome.budget.duplicates.len()).unwrap_or(i64::MAX);
    let tokens = i64::try_from(outcome.budget.duplicate_tokens).unwrap_or(i64::MAX);
    let write = async {
        let project = ax_neural::dispatch::adopted_project_id(pool, root).await;
        ax_neural::store::record_rank(pool, &project, blocks, tokens, outcome.micros).await
    };
    match tokio::time::timeout(TOTALS_TIMEOUT, write).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!("neural rank totals not recorded: {error}"),
        Err(_) => tracing::warn!(
            "neural rank totals not recorded: database busy for {} ms",
            TOTALS_TIMEOUT.as_millis()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_without_neural_config_still_ranks_and_keeps_hard_required() {
        let dir = tempfile::tempdir().unwrap();
        let mut blocks = vec![
            ContextBlock {
                id: "opt".into(),
                class: ContextClass::Optional,
                text: "optional memory note".into(),
            },
            ContextBlock {
                id: "required".into(),
                class: ContextClass::HardRequired,
                text: "keep this".into(),
            },
        ];
        let notes = maybe_reorder(dir.path(), "memory", &mut blocks)
            .unwrap()
            .notes;
        assert_eq!(blocks[0].id, "required");
        assert!(notes.iter().any(|note| note.id == "required" && note.keep));
    }

    #[test]
    fn duplicate_block_is_reported_with_its_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let text = "graph edges stay authoritative";
        let mut blocks = vec![
            ContextBlock {
                id: "first".into(),
                class: ContextClass::Optional,
                text: text.into(),
            },
            ContextBlock {
                id: "copy".into(),
                class: ContextClass::Optional,
                text: text.into(),
            },
        ];
        let outcome = maybe_reorder(dir.path(), "graph", &mut blocks).unwrap();
        assert_eq!(outcome.budget.duplicates, vec!["copy".to_string()]);
        assert_eq!(
            outcome.budget.duplicate_tokens,
            ax_usage::count_tokens(text)
        );
        let kept: Vec<&str> = blocks.iter().map(|block| block.id.as_str()).collect();
        assert_eq!(kept, vec!["first"]);
    }

    #[test]
    fn rank_time_is_measured() {
        let dir = tempfile::tempdir().unwrap();
        let mut blocks: Vec<ContextBlock> = (0..200)
            .map(|i| ContextBlock {
                id: format!("memory:{i}"),
                class: ContextClass::Optional,
                text: format!("note {i} ").repeat(200),
            })
            .collect();
        let outcome = maybe_reorder(dir.path(), "note 7", &mut blocks).unwrap();
        assert!(outcome.micros > 0);
    }

    #[test]
    fn enabled_ranking_keeps_hard_required_first() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ax.json"), r#"{"neural":{"enabled":true}}"#).unwrap();
        let mut blocks = vec![
            ContextBlock {
                id: "opt-late".into(),
                class: ContextClass::Optional,
                text: "unrelated text".into(),
            },
            ContextBlock {
                id: "required".into(),
                class: ContextClass::HardRequired,
                text: "policy".into(),
            },
            ContextBlock {
                id: "opt-memory".into(),
                class: ContextClass::Optional,
                text: "memory hash vectors".into(),
            },
        ];
        let notes = maybe_reorder(dir.path(), "memory hash", &mut blocks)
            .unwrap()
            .notes;
        assert_eq!(blocks[0].id, "required");
        assert!(notes.iter().any(|note| note.id == "required" && note.keep));
        assert_eq!(blocks[1].id, "opt-memory");
    }

    /// A project whose feedback and rank totals sit under the key older
    /// versions stored: the root with a trailing separator. Returns the
    /// project handle (keeps the pool open), the canonical root and the
    /// legacy spelling.
    async fn legacy_keyed_project(dir: &Path) -> (ax_core::Ax, std::path::PathBuf, String) {
        let root = dir.canonicalize().unwrap();
        std::fs::write(
            root.join("ax.json"),
            r#"{"neural":{"prediction":{"enabled":true},"learning":{"feedback_collection":true}}}"#,
        )
        .unwrap();
        let ax = ax_core::Ax::init(&root).await.unwrap();
        let pool = ax.db_pool();
        for execution in ["e1", "e2"] {
            ax_neural::dispatch::handle(
                &root,
                "feedback",
                &serde_json::json!({
                    "executionId": execution,
                    "taskCategory": "parser",
                    "outcome": "success",
                    "skillIds": ["parse-errors"],
                    "ruleIds": ["english-only"],
                    "contextIds": ["memory:parser-lines"]
                }),
                Some(pool),
            )
            .await
            .unwrap();
        }
        let canonical = ax_neural::dispatch::project_id(&root);
        ax_neural::store::record_rank(pool, &canonical, 1, 50, 200)
            .await
            .unwrap();
        let legacy = format!("{}{}", root.to_string_lossy(), std::path::MAIN_SEPARATOR);
        for table in ["neural_feedback", "neural_audit", "neural_rank_total"] {
            sqlx::query(&format!("UPDATE {table} SET project_id = ?"))
                .bind(&legacy)
                .execute(pool)
                .await
                .unwrap();
        }
        (ax, root, legacy)
    }

    #[tokio::test]
    async fn predictions_include_feedback_stored_under_a_legacy_key() {
        let dir = tempfile::tempdir().unwrap();
        let (ax, _root, legacy) = legacy_keyed_project(dir.path()).await;
        let block = predicted_block(Path::new(&legacy), ax.db_pool(), "fix the parser")
            .await
            .expect("legacy feedback predicts without any dispatch call first");
        assert!(block.text.contains("parse-errors"), "{}", block.text);
    }

    #[tokio::test]
    async fn rank_totals_merge_with_totals_stored_under_a_legacy_key() {
        let dir = tempfile::tempdir().unwrap();
        let (ax, root, legacy) = legacy_keyed_project(dir.path()).await;
        let pool = ax.db_pool();
        let outcome = RankOutcome {
            notes: Vec::new(),
            budget: NeuralBudget::default(),
            micros: 100,
        };
        record_totals(pool, Path::new(&legacy), &outcome).await;
        let canonical = ax_neural::dispatch::project_id(&root);
        let totals = ax_neural::store::rank_totals(pool, &canonical)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(totals.preflights, 2);
        assert_eq!(totals.duplicate_tokens, 50);
        let left: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM neural_rank_total WHERE project_id = ?")
                .bind(&legacy)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(left, 0);
    }
}
