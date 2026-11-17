//! Whole-block selection with final model-projection accounting.
use ax_usage::{count_tokens, ContextBlock, ContextClass};
use serde_json::{json, Value};

pub fn projection_tokens(value: &Value) -> usize {
    projection_tokens_with_mode(value, crate::server::render_full())
}

fn projection_tokens_with_mode(value: &Value, full: bool) -> usize {
    let text = value.get("inject").and_then(Value::as_str).unwrap_or("");
    let metadata = if full {
        value.clone()
    } else {
        crate::server::lean_structured("ax_preflight", value).unwrap_or(Value::Null)
    };
    count_tokens(text) + count_tokens(&metadata.to_string())
}

fn render(
    base: &Value,
    blocks: &[ContextBlock],
    ids: &[String],
    limit: Option<u32>,
    minimum: usize,
    overflow: bool,
) -> Result<Value, String> {
    render_with_iterations(base, blocks, ids, limit, minimum, overflow, 16)
}

fn render_with_iterations(
    base: &Value,
    blocks: &[ContextBlock],
    ids: &[String],
    limit: Option<u32>,
    minimum: usize,
    overflow: bool,
    iterations: usize,
) -> Result<Value, String> {
    let mut out = base.clone();
    if let Some(sources) = out.get_mut("policySources").and_then(Value::as_array_mut) {
        sources.retain(|source| {
            let key = format!(
                "{}:{}",
                source["kind"].as_str().unwrap_or_default(),
                source["id"].as_str().unwrap_or_default()
            );
            !blocks
                .iter()
                .any(|b| b.id == key && b.class != ContextClass::HardRequired)
                || ids.contains(&key)
        });
    }
    let omitted: Vec<_> = blocks
        .iter()
        .filter(|b| !ids.contains(&b.id))
        .map(|b| &b.id)
        .collect();
    let mut text = ids
        .iter()
        .filter_map(|id| blocks.iter().find(|b| &b.id == id))
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    if !omitted.is_empty() {
        text.push_str("\n\nOptional blocks omitted. Retrieve with ax_status, ax_rules, ax_skill, ax_recall, ax_history or ax_expand; graph queries accept fresh: true.");
    }
    if overflow {
        text.push_str(
            "\n\nContext budget exceeded: required policy and working context are preserved.",
        );
    }
    out["inject"] = json!(text);
    let neural = base.pointer("/contextBudget/neural").cloned();
    out["contextBudget"] = json!({"included":ids, "omitted":omitted,
        "limit":limit, "requestedBudget":limit, "requiredMinimum":minimum,
        "overBudget":overflow, "tokens":0, "finalTokens":0,
        "measurement":if ax_usage::tokenizer_available() {"o200k_base"} else {"estimate"},
        "projection":"content.text + structuredContent JSON"});
    if let Some(neural) = neural {
        out["contextBudget"]["neural"] = neural;
    }
    // Self-describing token totals converge once their digit widths are stable.
    for _ in 0..iterations {
        let tokens = projection_tokens(&out);
        if out["contextBudget"]["tokens"].as_u64() == Some(tokens as u64) {
            return Ok(out);
        }
        out["contextBudget"]["tokens"] = json!(tokens);
        out["contextBudget"]["finalTokens"] = json!(tokens);
    }
    Err("context token accounting did not converge".into())
}

/// Neural ranking facts that belong in `contextBudget.neural`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NeuralBudget {
    pub duplicates: Vec<String>,
    pub duplicate_tokens: usize,
}

/// Single JSON shape for `contextBudget.neural` (preflight, status, tests).
pub fn neural_budget_json(budget: &NeuralBudget) -> Value {
    json!({
        "kept": "ranked",
        "omitted": "over-budget",
        "duplicates": budget.duplicates,
        "duplicateTokens": budget.duplicate_tokens
    })
}

impl NeuralBudget {
    /// Kept optional blocks were ranked; `omitted` ids were dropped by the budget;
    /// `duplicates` were dropped before it because their text repeated an earlier block.
    fn to_json(&self) -> Value {
        neural_budget_json(self)
    }
}

/// Like `finish_with_memory_reserve`, with `contextBudget.neural` inside the counted reply.
pub fn finish_with_neural(
    mut base: Value,
    blocks: &[ContextBlock],
    limit: Option<u32>,
    reserve: u32,
    neural: &NeuralBudget,
) -> Result<Value, String> {
    base["contextBudget"] = json!({ "neural": neural.to_json() });
    finish_with_memory_reserve(base, blocks, limit, reserve)
}

pub fn finish(base: Value, blocks: &[ContextBlock], limit: Option<u32>) -> Result<Value, String> {
    finish_with_memory_reserve(base, blocks, limit, ax_usage::MEMORY_RESERVE_TOKENS)
}

pub fn finish_with_memory_reserve(
    base: Value,
    blocks: &[ContextBlock],
    limit: Option<u32>,
    reserve: u32,
) -> Result<Value, String> {
    finish_with_iterations(base, blocks, limit, reserve, 16)
}

fn finish_with_iterations(
    base: Value,
    blocks: &[ContextBlock],
    limit: Option<u32>,
    reserve: u32,
    iterations: usize,
) -> Result<Value, String> {
    let required: Vec<String> = blocks
        .iter()
        .filter(|b| b.class == ContextClass::HardRequired)
        .map(|b| b.id.clone())
        .collect();
    let mut minimum = 0;
    let mut converged = false;
    for _ in 0..iterations {
        let candidate = render(
            &base,
            blocks,
            &required,
            limit,
            minimum,
            limit.is_some_and(|b| minimum > b as usize),
        )?;
        let measured = projection_tokens(&candidate);
        if measured == minimum {
            converged = true;
            break;
        }
        minimum = measured;
    }
    if !converged {
        return Err("required context token accounting did not converge".into());
    }
    let overflow = limit.is_some_and(|b| minimum > b as usize);
    let mut ids = required;
    if let Some(memory) = blocks.iter().find(|block| block.id == "memories") {
        let tokens = count_tokens(&memory.text);
        if tokens <= reserve as usize && !ids.iter().any(|id| id == "memories") {
            ids.push(memory.id.clone());
        }
    }
    let mut out = render(&base, blocks, &ids, limit, minimum, overflow)?;
    for class in [ContextClass::HighValue, ContextClass::Optional] {
        for block in blocks.iter().filter(|b| b.class == class) {
            if ids.iter().any(|id| id == &block.id) {
                continue;
            }
            let mut candidate_ids = ids.clone();
            candidate_ids.push(block.id.clone());
            let candidate = render(&base, blocks, &candidate_ids, limit, minimum, overflow)?;
            if limit.is_none_or(|b| projection_tokens(&candidate) <= b as usize) {
                ids = candidate_ids;
                out = candidate;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accounting_exhaustion_fails_instead_of_claiming_a_budget_fit() {
        let base = json!({"inject":"required"});
        assert!(
            render_with_iterations(&base, &[], &[], Some(100), 0, false, 0)
                .unwrap_err()
                .contains("accounting did not converge")
        );
        assert!(
            finish_with_iterations(base, &[], Some(100), ax_usage::MEMORY_RESERVE_TOKENS, 0)
                .unwrap_err()
                .contains("required context token accounting did not converge")
        );
    }

    #[test]
    fn full_projection_counts_both_text_and_raw_metadata() {
        let base = json!({"inject":"required text","rules":[{"body":"raw body"}]});
        assert_eq!(
            projection_tokens_with_mode(&base, true),
            count_tokens("required text") + count_tokens(&base.to_string())
        );
    }

    #[test]
    fn a_short_memory_block_stays_when_required_context_is_over_budget() {
        use ax_usage::{ContextBlock, ContextClass};
        let blocks = vec![
            ContextBlock {
                id: "required".into(),
                class: ContextClass::HardRequired,
                text: "rule ".repeat(80),
            },
            ContextBlock {
                id: "memories".into(),
                class: ContextClass::HighValue,
                text: "hash vectors".into(),
            },
        ];
        let out = finish(json!({"project":{"path":"a"}}), &blocks, Some(1)).unwrap();
        let included = out["contextBudget"]["included"].as_array().unwrap();
        assert!(included.iter().any(|value| value == "memories"));
        assert!(out["inject"].as_str().unwrap().contains("hash vectors"));
    }

    #[test]
    fn neural_reasons_are_counted_in_the_budget() {
        let blocks = vec![
            ContextBlock {
                id: "required".into(),
                class: ContextClass::HardRequired,
                text: "rule".into(),
            },
            ContextBlock {
                id: "optional".into(),
                class: ContextClass::Optional,
                text: "word ".repeat(400),
            },
        ];
        let neural = NeuralBudget {
            duplicates: vec!["memories:copy".into()],
            duplicate_tokens: 12,
        };
        let base = json!({"project":{"path":"a"}});
        let out = finish_with_neural(base.clone(), &blocks, Some(200), 0, &neural).unwrap();
        let budget = &out["contextBudget"];
        assert_eq!(budget["neural"], neural_budget_json(&neural));
        assert_eq!(budget["omitted"], json!(["optional"]));
        assert_eq!(
            budget["tokens"].as_u64().unwrap() as usize,
            projection_tokens(&out)
        );
        let plain = finish_with_memory_reserve(base, &blocks, Some(200), 0).unwrap();
        assert!(plain["contextBudget"].get("neural").is_none());
        assert!(
            budget["tokens"].as_u64().unwrap() > plain["contextBudget"]["tokens"].as_u64().unwrap()
        );
    }

    #[test]
    fn a_zero_reserve_does_not_force_a_memory_block_over_budget() {
        use ax_usage::{ContextBlock, ContextClass};
        let blocks = vec![
            ContextBlock {
                id: "required".into(),
                class: ContextClass::HardRequired,
                text: "rule ".repeat(80),
            },
            ContextBlock {
                id: "memories".into(),
                class: ContextClass::HighValue,
                text: "hash vectors".into(),
            },
        ];
        let out = finish_with_memory_reserve(json!({"project":{"path":"a"}}), &blocks, Some(1), 0)
            .unwrap();
        let included = out["contextBudget"]["included"].as_array().unwrap();
        assert!(included.iter().all(|value| value != "memories"));
    }
}
