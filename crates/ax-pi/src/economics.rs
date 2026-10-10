//! Token estimates, repeat detection, tool alternatives, and cost/budget math.
//!
//! Model dollars go through [`ax_usage::calculate`]. A tool's own runtime is not
//! priced as tokens. Context cost is the estimate of tokens that would be
//! delivered to a later model call, priced at the input rate.

use std::collections::HashMap;

use ax_usage::{calculate, CostConfidence, CostUsage, ModelPricing};
use serde_json::Value;

use crate::types::{
    AxModelUsage, BudgetSnapshot, Confidence, OptimizationMode, RepeatRecord, ToolAlternative, ToolCallAdvice,
    ToolCallObservation, GRAPH_EXPLORE_TOKEN_ESTIMATE,
};

pub struct UsageTokenEstimator;

pub trait TokenEstimator {
    fn estimate_text(&self, text: &str) -> u32;
}

impl TokenEstimator for UsageTokenEstimator {
    fn estimate_text(&self, text: &str) -> u32 {
        ax_usage::count_tokens(text) as u32
    }
}

pub fn estimate_text(text: &str) -> u32 {
    UsageTokenEstimator.estimate_text(text)
}

pub fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys
                .into_iter()
                .map(|key| format!("{key}:{}", canonical_json(&map[key])))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", parts.join(","))
        }
        other => other.to_string(),
    }
}

pub fn identity_hash(tool_name: &str, arguments: &Value, repository_state: &str) -> String {
    let material = format!(
        "{}|{}|{}",
        tool_name.trim().to_ascii_lowercase(),
        canonical_json(arguments),
        repository_state
    );
    blake3::hash(material.as_bytes()).to_hex().to_string()
}

#[derive(Debug, Clone)]
struct SeenCall {
    first_call_id: String,
    latest_call_id: String,
    count: u32,
    tokens_repeated: u64,
    repeated_cost: Option<f64>,
}

#[derive(Debug, Default)]
pub struct RepeatDetector {
    seen: HashMap<String, SeenCall>,
}

impl RepeatDetector {
    pub fn observe(
        &mut self,
        identity: &str,
        call_id: &str,
        output_tokens: u32,
        context_cost: Option<f64>,
    ) -> (bool, RepeatRecord) {
        if let Some(prior) = self.seen.get_mut(identity) {
            prior.count += 1;
            prior.latest_call_id = call_id.to_string();
            prior.tokens_repeated += u64::from(output_tokens);
            if let Some(cost) = context_cost {
                prior.repeated_cost = Some(prior.repeated_cost.unwrap_or(0.0) + cost);
            }
            let record = RepeatRecord {
                repeat_count: prior.count,
                first_call_id: prior.first_call_id.clone(),
                latest_call_id: prior.latest_call_id.clone(),
                tokens_repeated: prior.tokens_repeated,
                estimated_repeated_cost: prior.repeated_cost,
            };
            return (true, record);
        }
        self.seen.insert(
            identity.to_string(),
            SeenCall {
                first_call_id: call_id.to_string(),
                latest_call_id: call_id.to_string(),
                count: 1,
                tokens_repeated: 0,
                repeated_cost: None,
            },
        );
        (
            false,
            RepeatRecord {
                repeat_count: 1,
                first_call_id: call_id.to_string(),
                latest_call_id: call_id.to_string(),
                tokens_repeated: 0,
                estimated_repeated_cost: None,
            },
        )
    }

    pub fn contains(&self, identity: &str) -> bool {
        self.seen.contains_key(identity)
    }
}

pub fn reduction_percent(current: u32, alternative: u32) -> f64 {
    if current == 0 {
        return 0.0;
    }
    let saved = current.saturating_sub(alternative) as f64;
    (saved / current as f64) * 100.0
}

fn search_tool(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "rg" | "grep" | "ripgrep" | "search"
    )
}

fn read_tool(name: &str) -> bool {
    matches!(name.trim().to_ascii_lowercase().as_str(), "read" | "cat")
}

fn graph_tool(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "ax_explore" | "ax_node"
    )
}

fn query_text(arguments: &Value) -> String {
    match arguments {
        Value::String(text) => text.clone(),
        Value::Object(map) => ["pattern", "query", "q", "path", "file", "target"]
            .iter()
            .filter_map(|key| map.get(*key).and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

fn target_path(arguments: &Value) -> Option<String> {
    match arguments {
        Value::String(text) => Some(text.clone()),
        Value::Object(map) => ["path", "file", "target", "file_path"]
            .iter()
            .find_map(|key| map.get(*key).and_then(Value::as_str).map(str::to_string)),
        _ => None,
    }
}

pub fn advise_observation(
    observation: &ToolCallObservation,
    already_seen: bool,
    mode: OptimizationMode,
    oversized_tokens: u32,
    expose: bool,
) -> ToolCallAdvice {
    let current = observation.output_token_estimate.unwrap_or_else(|| {
        observation
            .output_text_for_estimate
            .as_deref()
            .map(estimate_text)
            .unwrap_or(0)
    });
    let mut found: Option<ToolAlternative> = None;

    if search_tool(&observation.tool_name) {
        let query = query_text(&observation.arguments);
        if let Some(entity) = observation
            .known_graph_entities
            .iter()
            .find(|entity| !entity.is_empty() && query.contains(entity.as_str()))
        {
            let alternative_tokens = if current > GRAPH_EXPLORE_TOKEN_ESTIMATE {
                GRAPH_EXPLORE_TOKEN_ESTIMATE
            } else {
                current
            };
            found = Some(ToolAlternative {
                tool_name: "ax_explore".into(),
                estimated_current_tokens: current,
                estimated_alternative_tokens: alternative_tokens,
                estimated_reduction_percent: reduction_percent(current, alternative_tokens),
                confidence: Confidence::High,
                reason: format!("{entity} exists as a known graph entity."),
                rule: "known-symbol-search".into(),
            });
        }
    }

    if found.is_none() && read_tool(&observation.tool_name) {
        if let Some(path) = target_path(&observation.arguments) {
            let indexed = observation
                .indexed_paths
                .iter()
                .any(|known| known == &path || path.ends_with(known));
            if indexed {
                let alternative_tokens = if current > GRAPH_EXPLORE_TOKEN_ESTIMATE {
                    GRAPH_EXPLORE_TOKEN_ESTIMATE
                } else {
                    current
                };
                found = Some(ToolAlternative {
                    tool_name: "ax_node".into(),
                    estimated_current_tokens: current,
                    estimated_alternative_tokens: alternative_tokens,
                    estimated_reduction_percent: reduction_percent(current, alternative_tokens),
                    confidence: Confidence::High,
                    reason: format!("{path} is indexed source. A graph-backed read is smaller."),
                    rule: "indexed-read".into(),
                });
            }
        }
    }

    if found.is_none() && already_seen {
        found = Some(ToolAlternative {
            tool_name: "cached-result".into(),
            estimated_current_tokens: current,
            estimated_alternative_tokens: 0,
            estimated_reduction_percent: reduction_percent(current, 0),
            confidence: Confidence::High,
            reason: "The same tool, arguments, and repository state already ran.".into(),
            rule: "exact-repeat".into(),
        });
    }

    if found.is_none() && current > oversized_tokens {
        let confidence = if graph_tool(&observation.tool_name) {
            Confidence::High
        } else {
            Confidence::Medium
        };
        let rule = if graph_tool(&observation.tool_name) {
            "oversized-graph".to_string()
        } else {
            "oversized-output".to_string()
        };
        found = Some(ToolAlternative {
            tool_name: "filtered-output".into(),
            estimated_current_tokens: current,
            estimated_alternative_tokens: oversized_tokens,
            estimated_reduction_percent: reduction_percent(current, oversized_tokens),
            confidence,
            reason: format!("Tool output exceeds the configured threshold of {oversized_tokens} estimated tokens."),
            rule,
        });
    }

    let apply = found.as_ref().and_then(|alternative| {
        if mode == OptimizationMode::Optimize && auto_allowed(alternative) {
            Some(alternative.clone())
        } else {
            None
        }
    });
    let alternatives = if expose {
        found.into_iter().collect()
    } else {
        Vec::new()
    };
    ToolCallAdvice {
        mode,
        alternatives: if mode == OptimizationMode::Observe {
            Vec::new()
        } else {
            alternatives
        },
        apply,
        cached_output: None,
        degraded: false,
    }
}

fn auto_allowed(alternative: &ToolAlternative) -> bool {
    alternative.confidence == Confidence::High
        && matches!(
            alternative.rule.as_str(),
            "known-symbol-search" | "exact-repeat" | "oversized-graph"
        )
}

pub fn model_cost(
    provider: Option<&str>,
    model: Option<&str>,
    usage: &AxModelUsage,
    session_id: &str,
    turn_id: Option<&str>,
    pricing: Option<ModelPricing>,
) -> Option<f64> {
    let cost = calculate(
        &CostUsage {
            provider: provider.map(str::to_string),
            model: model.map(str::to_string),
            input_tokens: usage.input_tokens.map(|n| n as i64),
            output_tokens: usage.output_tokens.map(|n| n as i64),
            cache_read_tokens: usage.cached_input_tokens.map(|n| n as i64),
            cache_write_tokens: Some(0),
            timestamp_ms: None,
            session_id: Some(session_id.to_string()),
            turn_id: turn_id.map(str::to_string),
            confidence: CostConfidence::Estimated,
        },
        pricing,
    );
    cost.total_usd
}

pub fn context_token_cost(tokens: u32, pricing: Option<ModelPricing>) -> Option<f64> {
    let pricing = pricing?;
    Some((f64::from(tokens) / 1_000_000.0) * pricing.input_per_mtok)
}

pub fn budget_snapshot(
    spent: f64,
    days_elapsed: u32,
    days_remaining: u32,
    monthly: f64,
) -> BudgetSnapshot {
    let average = if days_elapsed == 0 {
        0.0
    } else {
        spent / f64::from(days_elapsed)
    };
    BudgetSnapshot {
        spent,
        remaining: (monthly - spent).max(0.0),
        projected: average * f64::from(days_remaining),
        currency: String::new(),
        days_elapsed,
        days_remaining,
    }
}

pub fn display_amount(usd: f64, currency: &str, usd_per_eur: Option<f64>) -> (f64, String) {
    if currency.eq_ignore_ascii_case("eur") {
        if let Some(rate) = usd_per_eur.filter(|rate| *rate > 0.0) {
            return (usd / rate, "EUR".into());
        }
    }
    if currency.eq_ignore_ascii_case("eur") {
        return (usd, "USD".into());
    }
    (usd, currency.to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn observation(tool: &str, args: Value, tokens: u32) -> ToolCallObservation {
        ToolCallObservation {
            session_id: "s".into(),
            turn_id: "t".into(),
            call_id: "c".into(),
            tool_name: tool.into(),
            arguments: args,
            repository_state: "head".into(),
            known_graph_entities: vec!["UserService".into()],
            indexed_paths: vec!["src/UserService.cs".into()],
            output_text_for_estimate: None,
            output_token_estimate: Some(tokens),
            output_bytes: None,
            duration_ms: 1,
            success: true,
        }
    }

    #[test]
    fn rg_userservice_suggests_ax_explore() {
        let advice = advise_observation(
            &observation(
                "rg",
                json!({"pattern": "UserService", "path": "src/"}),
                7_423,
            ),
            false,
            OptimizationMode::Advise,
            2_000,
            true,
        );
        let alternative = &advice.alternatives[0];
        assert_eq!(alternative.tool_name, "ax_explore");
        assert_eq!(alternative.confidence, Confidence::High);
        assert!((alternative.estimated_reduction_percent - 91.4).abs() < 0.05);
        assert!(alternative.reason.contains("UserService"));
        assert!(advice.apply.is_none());
    }

    #[test]
    fn optimize_mode_approves_known_symbol_search_only() {
        let advice = advise_observation(
            &observation("rg", json!({"pattern": "UserService"}), 7_423),
            false,
            OptimizationMode::Optimize,
            2_000,
            true,
        );
        assert_eq!(
            advice.apply.as_ref().map(|item| item.tool_name.as_str()),
            Some("ax_explore")
        );
    }

    #[test]
    fn observe_mode_hides_advice() {
        let advice = advise_observation(
            &observation("rg", json!({"pattern": "UserService"}), 7_423),
            false,
            OptimizationMode::Observe,
            2_000,
            true,
        );
        assert!(advice.alternatives.is_empty());
        assert!(advice.apply.is_none());
    }

    #[test]
    fn second_identical_call_is_repeated() {
        let mut detector = RepeatDetector::default();
        let hash = identity_hash("read", &json!("src/UserService.cs"), "head");
        let (first, _) = detector.observe(&hash, "a", 10, None);
        let (second, record) = detector.observe(&hash, "b", 10, None);
        assert!(!first);
        assert!(second);
        assert_eq!(record.repeat_count, 2);
        assert_eq!(record.first_call_id, "a");
        assert_eq!(record.latest_call_id, "b");
        assert_eq!(record.tokens_repeated, 10);
    }

    #[test]
    fn different_repository_state_is_not_a_repeat() {
        let left = identity_hash("read", &json!("src/UserService.cs"), "aaa");
        let right = identity_hash("read", &json!("src/UserService.cs"), "bbb");
        assert_ne!(left, right);
    }

    #[test]
    fn huge_output_advises_filtering() {
        let advice = advise_observation(
            &observation("bash", json!({"cmd": "cat big"}), 5_000),
            false,
            OptimizationMode::Advise,
            2_000,
            true,
        );
        assert_eq!(advice.alternatives[0].tool_name, "filtered-output");
        assert!(advice.apply.is_none());
    }

    #[test]
    fn oversized_graph_tool_is_approved_in_optimize_mode() {
        let advice = advise_observation(
            &observation("ax_explore", json!({"query": "other"}), 5_000),
            false,
            OptimizationMode::Optimize,
            2_000,
            true,
        );
        assert_eq!(
            advice.apply.as_ref().map(|item| item.rule.as_str()),
            Some("oversized-graph")
        );
    }

    #[test]
    fn indexed_read_suggests_ax_node_without_auto_apply() {
        let advice = advise_observation(
            &observation("read", json!({"path": "src/UserService.cs"}), 3_000),
            false,
            OptimizationMode::Optimize,
            2_000,
            true,
        );
        assert_eq!(advice.alternatives[0].tool_name, "ax_node");
        assert!(advice.apply.is_none());
    }

    #[test]
    fn empty_text_estimates_zero_tokens() {
        assert_eq!(estimate_text(""), 0);
    }

    #[test]
    fn model_cost_uses_catalog_math_and_unknown_rate_stays_unknown() {
        let pricing = ModelPricing::rates(3.0, 15.0);
        let known = model_cost(
            Some("anthropic"),
            Some("claude"),
            &AxModelUsage {
                input_tokens: Some(1_000_000),
                output_tokens: Some(0),
                cached_input_tokens: Some(0),
                total_tokens: None,
            },
            "s",
            None,
            Some(pricing),
        );
        assert_eq!(known, Some(3.0));
        let unknown = model_cost(
            Some("anthropic"),
            Some("claude"),
            &AxModelUsage {
                input_tokens: Some(10),
                output_tokens: Some(1),
                cached_input_tokens: Some(0),
                total_tokens: None,
            },
            "s",
            None,
            None,
        );
        assert_eq!(unknown, None);
    }

    #[test]
    fn tool_execution_is_not_priced_as_model_tokens() {
        let pricing = ModelPricing::rates(3.0, 15.0);
        let context = context_token_cost(1_000_000, Some(pricing));
        assert_eq!(context, Some(3.0));
    }

    #[test]
    fn eur_conversion_uses_usd_per_eur() {
        let (amount, currency) = display_amount(1.08, "eur", Some(1.08));
        assert!((amount - 1.0).abs() < 0.001);
        assert_eq!(currency, "EUR");
        let (usd, label) = display_amount(1.08, "eur", None);
        assert_eq!(label, "USD");
        assert!((usd - 1.08).abs() < 0.001);
    }

    #[test]
    fn budget_projects_daily_average_times_remaining_days() {
        let snapshot = budget_snapshot(30.0, 10, 20, 60.0);
        assert_eq!(snapshot.spent, 30.0);
        assert_eq!(snapshot.remaining, 30.0);
        assert!((snapshot.projected - 60.0).abs() < 0.001);
    }
}
