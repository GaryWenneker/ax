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
    out["contextBudget"] = json!({"included":ids, "omitted":omitted,
        "limit":limit, "requestedBudget":limit, "requiredMinimum":minimum,
        "overBudget":overflow, "tokens":0, "finalTokens":0,
        "measurement":if ax_usage::tokenizer_available() {"o200k_base"} else {"estimate"},
        "projection":"content.text + structuredContent JSON"});
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

pub fn finish(base: Value, blocks: &[ContextBlock], limit: Option<u32>) -> Result<Value, String> {
    finish_with_iterations(base, blocks, limit, 16)
}

fn finish_with_iterations(
    base: Value,
    blocks: &[ContextBlock],
    limit: Option<u32>,
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
    let mut out = render(&base, blocks, &ids, limit, minimum, overflow)?;
    for class in [ContextClass::HighValue, ContextClass::Optional] {
        for block in blocks.iter().filter(|b| b.class == class) {
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
        assert!(finish_with_iterations(base, &[], Some(100), 0)
            .unwrap_err()
            .contains("required context token accounting did not converge"));
    }

    #[test]
    fn full_projection_counts_both_text_and_raw_metadata() {
        let base = json!({"inject":"required text","rules":[{"body":"raw body"}]});
        assert_eq!(
            projection_tokens_with_mode(&base, true),
            count_tokens("required text") + count_tokens(&base.to_string())
        );
    }
}
