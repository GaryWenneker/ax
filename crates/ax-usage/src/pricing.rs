//! Model pricing for dollar-cost estimates.
//!
//! Built-in defaults for common models, overridable via `~/.ax/pricing.toml`:
//!
//! ```toml
//! reference_model = "claude-sonnet"
//!
//! [models.claude-sonnet]
//! input_per_mtok = 3.0
//! output_per_mtok = 15.0
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

pub const PRICING_FILENAME: &str = "pricing.toml";

/// USD per million tokens.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ModelPricing {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct PricingFile {
    reference_model: Option<String>,
    #[serde(default)]
    models: HashMap<String, ModelPricing>,
}

#[derive(Debug, Clone)]
pub struct PricingConfig {
    /// Model used to price savings when the actual model is unknown.
    pub reference_model: String,
    /// Substring-matched model table (key matched against full model ids).
    pub models: HashMap<String, ModelPricing>,
    /// "default" or "user" (loaded from pricing.toml).
    pub source: &'static str,
}

/// Pricing metadata surfaced in API responses.
#[derive(Debug, Clone, Serialize)]
pub struct PricingInfo {
    pub reference_model: String,
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub source: &'static str,
    pub config_path: String,
}

fn default_models() -> HashMap<String, ModelPricing> {
    let mut m = HashMap::new();
    // USD per 1M tokens. Users can pin exact numbers in ~/.ax/pricing.toml.
    m.insert("claude-opus", ModelPricing { input_per_mtok: 5.0, output_per_mtok: 25.0 });
    m.insert("claude-sonnet", ModelPricing { input_per_mtok: 3.0, output_per_mtok: 15.0 });
    m.insert("claude-haiku", ModelPricing { input_per_mtok: 1.0, output_per_mtok: 5.0 });
    m.insert("gpt-5", ModelPricing { input_per_mtok: 1.25, output_per_mtok: 10.0 });
    m.insert("gpt-4o", ModelPricing { input_per_mtok: 2.5, output_per_mtok: 10.0 });
    m.insert("gpt-4.1", ModelPricing { input_per_mtok: 2.0, output_per_mtok: 8.0 });
    m.insert("gemini-2.5-pro", ModelPricing { input_per_mtok: 1.25, output_per_mtok: 10.0 });
    m.insert("gemini", ModelPricing { input_per_mtok: 0.30, output_per_mtok: 2.5 });
    // Cursor Composer labels (e.g. composer-2.5-fast) match via substring "composer".
    m.insert("composer", ModelPricing { input_per_mtok: 1.25, output_per_mtok: 10.0 });
    m.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

pub fn pricing_config_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".ax").join(PRICING_FILENAME))
        .unwrap_or_else(|| PathBuf::from(".ax").join(PRICING_FILENAME))
}

fn load_config() -> PricingConfig {
    let mut config = PricingConfig {
        reference_model: "claude-sonnet".to_string(),
        models: default_models(),
        source: "default",
    };
    let path = pricing_config_path();
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(file) = toml::from_str::<PricingFile>(&text) {
            if let Some(reference) = file.reference_model {
                config.reference_model = reference;
            }
            for (k, v) in file.models {
                config.models.insert(k, v);
            }
            config.source = "user";
        }
    }
    config
}

static CONFIG: OnceLock<PricingConfig> = OnceLock::new();

pub fn pricing_config() -> &'static PricingConfig {
    CONFIG.get_or_init(load_config)
}

/// Resolve pricing for a full model id (e.g. `claude-sonnet-4-5-20250929`)
/// by longest substring key match. Falls back to the reference model.
pub fn price_for_model(model: &str) -> ModelPricing {
    let config = pricing_config();
    let lowered = model.to_ascii_lowercase();
    let mut best: Option<(&String, &ModelPricing)> = None;
    for (key, pricing) in &config.models {
        if lowered.contains(&key.to_ascii_lowercase()) {
            match best {
                Some((bk, _)) if bk.len() >= key.len() => {}
                _ => best = Some((key, pricing)),
            }
        }
    }
    match best {
        Some((_, pricing)) => *pricing,
        None => reference_pricing(),
    }
}

/// Pricing for the configured reference model (used when the model is unknown).
pub fn reference_pricing() -> ModelPricing {
    let config = pricing_config();
    price_lookup_exactish(&config.reference_model).unwrap_or(ModelPricing {
        input_per_mtok: 3.0,
        output_per_mtok: 15.0,
    })
}

fn price_lookup_exactish(name: &str) -> Option<ModelPricing> {
    let config = pricing_config();
    let lowered = name.to_ascii_lowercase();
    if let Some(p) = config.models.get(&lowered) {
        return Some(*p);
    }
    config
        .models
        .iter()
        .find(|(k, _)| lowered.contains(&k.to_ascii_lowercase()))
        .map(|(_, p)| *p)
}

pub fn pricing_info() -> PricingInfo {
    let config = pricing_config();
    let reference = reference_pricing();
    PricingInfo {
        reference_model: config.reference_model.clone(),
        input_per_mtok: reference.input_per_mtok,
        output_per_mtok: reference.output_per_mtok,
        source: config.source,
        config_path: pricing_config_path().display().to_string(),
    }
}

/// USD cost of `tokens` input tokens at the given per-million rate.
pub fn input_cost_usd(tokens: i64, pricing: ModelPricing) -> f64 {
    (tokens.max(0) as f64 / 1_000_000.0) * pricing.input_per_mtok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_substring_match_prefers_longest_key() {
        let p = price_for_model("claude-sonnet-4-5-20250929");
        assert!(p.input_per_mtok > 0.0);
        // gemini-2.5-pro should match the specific key, not the generic "gemini"
        let pro = price_for_model("models/gemini-2.5-pro-latest");
        assert!(pro.input_per_mtok >= 1.0);
    }

    #[test]
    fn unknown_model_uses_reference() {
        let p = price_for_model("some-unknown-model-xyz");
        let r = reference_pricing();
        assert_eq!(p.input_per_mtok, r.input_per_mtok);
    }

    #[test]
    fn cost_math() {
        let pricing = ModelPricing { input_per_mtok: 3.0, output_per_mtok: 15.0 };
        assert_eq!(input_cost_usd(1_000_000, pricing), 3.0);
        assert_eq!(input_cost_usd(0, pricing), 0.0);
        assert_eq!(input_cost_usd(-5, pricing), 0.0);
    }
}
