//! Fetch model prices from OpenRouter.

use serde::Deserialize;
use serde_json::Value;

use crate::pricing::ModelPricing;

pub const SOURCE_OPENROUTER: &str = "openrouter";
pub const SOURCE_ARTIFICIAL_ANALYSIS: &str = "artificial_analysis";
pub const SOURCE_CODING_AGENTS: &str = "coding_agents";

const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

#[derive(Debug, Clone)]
pub struct FetchedPrice {
    pub model_id: String,
    pub display_name: String,
    pub provider: String,
    pub pricing: ModelPricing,
    pub cache_read_per_mtok: Option<f64>,
    pub blended_3_to_1: Option<f64>,
    pub context_length: Option<i64>,
    pub raw_json: Option<String>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FetchedBenchmark {
    pub model_id: String,
    pub display_name: String,
    pub intelligence: Option<f64>,
    pub coding: Option<f64>,
    pub agentic: Option<f64>,
    pub median_output_tps: Option<f64>,
    pub median_ttft_seconds: Option<f64>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FetchedCodingAgent {
    pub agent: String,
    pub model: String,
    pub index_score: Option<f64>,
    pub cost_per_task: Option<f64>,
    pub time_per_task: Option<f64>,
    pub tokens_per_task: Option<f64>,
    pub raw_json: Option<String>,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct FetchBundle {
    pub openrouter: Vec<FetchedPrice>,
    pub aa_prices: Vec<FetchedPrice>,
    pub aa_benchmarks: Vec<FetchedBenchmark>,
    pub coding_agents: Vec<FetchedCodingAgent>,
    pub warnings: Vec<String>,
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent(format!("ax/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())
}

/// Convert OpenRouter USD-per-token string to USD per million tokens.
pub fn per_token_to_per_mtok(raw: &str) -> Option<f64> {
    let v: f64 = raw.trim().parse().ok()?;
    if !v.is_finite() || v < 0.0 {
        return None;
    }
    Some(v * 1_000_000.0)
}

/// Parse AA values that are already USD per million tokens.
#[allow(dead_code)]
pub fn aa_per_mtok(v: Option<f64>) -> Option<f64> {
    v.filter(|x| x.is_finite() && *x >= 0.0)
}

pub fn aa_api_key() -> Option<String> {
    std::env::var("AA_API_KEY")
        .or_else(|_| std::env::var("ARTIFICIAL_ANALYSIS_API_KEY"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[derive(Debug, Deserialize)]
struct OpenRouterList {
    data: Vec<OpenRouterModel>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModel {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    context_length: Option<i64>,
    #[serde(default)]
    pricing: Option<OpenRouterPricing>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterPricing {
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    completion: Option<String>,
    #[serde(default)]
    input_cache_read: Option<String>,
}

pub fn parse_openrouter_models(body: &Value) -> Vec<FetchedPrice> {
    let parsed: OpenRouterList = match serde_json::from_value(body.clone()) {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::with_capacity(parsed.data.len());
    for m in parsed.data {
        let Some(pricing) = m.pricing else { continue };
        let Some(input) = pricing.prompt.as_deref().and_then(per_token_to_per_mtok) else {
            continue;
        };
        let Some(output) = pricing
            .completion
            .as_deref()
            .and_then(per_token_to_per_mtok)
        else {
            continue;
        };
        let provider = m
            .id
            .split('/')
            .next()
            .unwrap_or("")
            .to_string();
        let cache_read = pricing
            .input_cache_read
            .as_deref()
            .and_then(per_token_to_per_mtok);
        let display_name = m.name.unwrap_or_else(|| m.id.clone());
        let raw = serde_json::to_string(&serde_json::json!({
            "id": m.id,
            "pricing": {
                "prompt": pricing.prompt,
                "completion": pricing.completion,
            }
        }))
        .ok();
        out.push(FetchedPrice {
            model_id: m.id,
            display_name,
            provider,
            pricing: ModelPricing {
                input_per_mtok: input,
                output_per_mtok: output,
            },
            cache_read_per_mtok: cache_read,
            blended_3_to_1: None,
            context_length: m.context_length,
            raw_json: raw,
        });
    }
    out
}

#[allow(dead_code)]
fn f64_field(obj: &Value, keys: &[&str]) -> Option<f64> {
    for k in keys {
        if let Some(v) = obj.get(*k) {
            if let Some(n) = v.as_f64() {
                return Some(n);
            }
            if let Some(s) = v.as_str() {
                if let Ok(n) = s.parse::<f64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

#[allow(dead_code)]
fn str_field(obj: &Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(v) = obj.get(*k) {
            if let Some(s) = v.as_str() {
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
        }
    }
    None
}

#[allow(dead_code)]
pub fn parse_aa_models(body: &Value) -> (Vec<FetchedPrice>, Vec<FetchedBenchmark>) {
    let rows = body
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();
    let mut prices = Vec::new();
    let mut benchmarks = Vec::new();
    for row in rows {
        let model_id = str_field(&row, &["slug", "id", "name"]).unwrap_or_default();
        if model_id.is_empty() {
            continue;
        }
        let display_name = str_field(&row, &["name", "slug"]).unwrap_or_else(|| model_id.clone());
        let provider = row
            .get("model_creator")
            .and_then(|c| str_field(c, &["name", "slug"]))
            .unwrap_or_default();
        let pricing = row.get("pricing").cloned().unwrap_or(Value::Null);
        let input = aa_per_mtok(f64_field(
            &pricing,
            &["price_1m_input_tokens", "input_per_mtok", "input"],
        ));
        let output = aa_per_mtok(f64_field(
            &pricing,
            &["price_1m_output_tokens", "output_per_mtok", "output"],
        ));
        let blended = aa_per_mtok(f64_field(
            &pricing,
            &["price_1m_blended_3_to_1", "blended_3_to_1"],
        ));
        if let (Some(input), Some(output)) = (input, output) {
            prices.push(FetchedPrice {
                model_id: model_id.clone(),
                display_name: display_name.clone(),
                provider: provider.clone(),
                pricing: ModelPricing {
                    input_per_mtok: input,
                    output_per_mtok: output,
                },
                cache_read_per_mtok: None,
                blended_3_to_1: blended,
                context_length: f64_field(&row, &["context_window", "context_length"])
                    .map(|n| n as i64),
                raw_json: serde_json::to_string(&row).ok(),
            });
        }
        let evals = row.get("evaluations").cloned().unwrap_or(Value::Null);
        let intelligence = f64_field(
            &evals,
            &[
                "artificial_analysis_intelligence_index",
                "intelligence_index",
            ],
        )
        .or_else(|| f64_field(&row, &["artificial_analysis_intelligence_index"]));
        let coding = f64_field(
            &evals,
            &["artificial_analysis_coding_index", "coding_index"],
        );
        let agentic = f64_field(
            &evals,
            &["artificial_analysis_agentic_index", "agentic_index"],
        );
        let median_output_tps = f64_field(
            &row,
            &[
                "median_output_tokens_per_second",
                "median_output_tps",
            ],
        )
        .or_else(|| {
            row.get("performance")
                .and_then(|p| f64_field(p, &["median_output_tokens_per_second"]))
        });
        let median_ttft = f64_field(
            &row,
            &[
                "median_time_to_first_token_seconds",
                "median_ttft_seconds",
            ],
        )
        .or_else(|| {
            row.get("performance")
                .and_then(|p| f64_field(p, &["median_time_to_first_token_seconds"]))
        });
        if intelligence.is_some() || coding.is_some() || agentic.is_some() {
            benchmarks.push(FetchedBenchmark {
                model_id,
                display_name,
                intelligence,
                coding,
                agentic,
                median_output_tps,
                median_ttft_seconds: median_ttft,
            });
        }
    }
    (prices, benchmarks)
}

#[allow(dead_code)]
pub fn parse_coding_agents(body: &Value) -> Vec<FetchedCodingAgent> {
    let rows = body
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| body.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for row in rows {
        let agent = str_field(
            &row,
            &["agent", "agent_name", "name", "harness", "slug"],
        )
        .unwrap_or_default();
        if agent.is_empty() {
            continue;
        }
        let model = str_field(&row, &["model", "model_name", "model_slug", "slug"])
            .unwrap_or_else(|| "(unknown)".into());
        out.push(FetchedCodingAgent {
            agent,
            model,
            index_score: f64_field(
                &row,
                &[
                    "index_score",
                    "coding_agent_index",
                    "score",
                    "artificial_analysis_coding_agent_index",
                ],
            ),
            cost_per_task: f64_field(&row, &["cost_per_task", "avg_cost_per_task", "cost"]),
            time_per_task: f64_field(&row, &["time_per_task", "avg_time_per_task", "time"]),
            tokens_per_task: f64_field(
                &row,
                &["tokens_per_task", "avg_tokens_per_task", "tokens"],
            ),
            raw_json: serde_json::to_string(&row).ok(),
        });
    }
    out
}

pub async fn fetch_all_sources() -> Result<FetchBundle, String> {
    let client = http_client()?;
    let mut bundle = FetchBundle::default();

    match client.get(OPENROUTER_MODELS_URL).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                match resp.json::<Value>().await {
                    Ok(body) => {
                        bundle.openrouter = parse_openrouter_models(&body);
                        if bundle.openrouter.is_empty() {
                            bundle
                                .warnings
                                .push("OpenRouter returned no priced models".into());
                        }
                    }
                    Err(e) => bundle.warnings.push(format!("OpenRouter JSON: {e}")),
                }
            } else {
                bundle
                    .warnings
                    .push(format!("OpenRouter HTTP {}", resp.status()));
            }
        }
        Err(e) => bundle.warnings.push(format!("OpenRouter fetch: {e}")),
    }

    if bundle.openrouter.is_empty() {
        return Err(format!(
            "pricing sync fetched no models ({})",
            bundle.warnings.join("; ")
        ));
    }

    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn openrouter_per_token_conversion() {
        assert!((per_token_to_per_mtok("0.000003").unwrap() - 3.0).abs() < 1e-9);
        assert!((per_token_to_per_mtok("0.000015").unwrap() - 15.0).abs() < 1e-9);
        assert!(per_token_to_per_mtok("-1").is_none());
    }

    #[test]
    fn parse_openrouter_sample() {
        let body = json!({
            "data": [{
                "id": "anthropic/claude-sonnet-4",
                "name": "Claude Sonnet 4",
                "context_length": 200000,
                "pricing": { "prompt": "0.000003", "completion": "0.000015" }
            }]
        });
        let rows = parse_openrouter_models(&body);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].model_id, "anthropic/claude-sonnet-4");
        assert!((rows[0].pricing.input_per_mtok - 3.0).abs() < 1e-9);
        assert!((rows[0].pricing.output_per_mtok - 15.0).abs() < 1e-9);
        assert_eq!(rows[0].provider, "anthropic");
    }

    #[test]
    fn parse_aa_sample() {
        let body = json!({
            "data": [{
                "slug": "claude-sonnet-4",
                "name": "Claude Sonnet 4",
                "model_creator": { "name": "Anthropic", "slug": "anthropic" },
                "pricing": {
                    "price_1m_input_tokens": 3.0,
                    "price_1m_output_tokens": 15.0,
                    "price_1m_blended_3_to_1": 6.0
                },
                "evaluations": {
                    "artificial_analysis_intelligence_index": 50.0,
                    "artificial_analysis_coding_index": 60.0,
                    "artificial_analysis_agentic_index": 40.0
                },
                "median_output_tokens_per_second": 80.0,
                "median_time_to_first_token_seconds": 1.2
            }]
        });
        let (prices, benches) = parse_aa_models(&body);
        assert_eq!(prices.len(), 1);
        assert_eq!(benches.len(), 1);
        assert!((prices[0].pricing.input_per_mtok - 3.0).abs() < 1e-9);
        assert_eq!(benches[0].coding, Some(60.0));
    }
}
