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
    pub cache_write_per_mtok: Option<f64>,
    pub blended_3_to_1: Option<f64>,
    pub context_length: Option<i64>,
    pub raw_json: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FetchBundle {
    pub openrouter: Vec<FetchedPrice>,
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

/// The Artificial Analysis API key, if set. ax does not fetch from that
/// source yet; the status page reports whether a key is configured.
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
    #[serde(default)]
    input_cache_write: Option<String>,
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
        let provider = m.id.split('/').next().unwrap_or("").to_string();
        let cache_read = pricing
            .input_cache_read
            .as_deref()
            .and_then(per_token_to_per_mtok);
        let cache_write = pricing
            .input_cache_write
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
                cache_read_per_mtok: cache_read,
                cache_write_per_mtok: cache_write,
            },
            cache_read_per_mtok: cache_read,
            cache_write_per_mtok: cache_write,
            blended_3_to_1: None,
            context_length: m.context_length,
            raw_json: raw,
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
}
