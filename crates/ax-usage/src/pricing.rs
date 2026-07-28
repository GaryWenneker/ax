//! Model pricing for dollar-cost estimates.
//!
//! Resolution order:
//! 1. Exact / longest-substring match in `~/.ax/pricing.toml` (user override)
//! 2. Latest synced snapshot in `~/.ax/usage.db` (prefer Artificial Analysis over OpenRouter)
//! 3. Built-in defaults
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
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

use crate::store::open_pool;

pub const PRICING_FILENAME: &str = "pricing.toml";

/// USD per million tokens.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
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
    pub source: String,
    pub config_path: String,
}

#[derive(Debug, Clone)]
struct DbPriceEntry {
    pricing: ModelPricing,
    source: String,
    model_id: String,
}

#[derive(Debug, Default)]
struct PriceCache {
    loaded: bool,
    entries: Vec<DbPriceEntry>,
}

fn price_cache() -> &'static RwLock<PriceCache> {
    static CACHE: OnceLock<RwLock<PriceCache>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(PriceCache::default()))
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

fn match_in_map(model: &str, models: &HashMap<String, ModelPricing>) -> Option<ModelPricing> {
    let lowered = model.to_ascii_lowercase();
    let mut best: Option<(&String, &ModelPricing)> = None;
    for (key, pricing) in models {
        if lowered.contains(&key.to_ascii_lowercase()) {
            match best {
                Some((bk, _)) if bk.len() >= key.len() => {}
                _ => best = Some((key, pricing)),
            }
        }
    }
    best.map(|(_, p)| *p)
}

fn user_override_for(model: &str) -> Option<ModelPricing> {
    let config = pricing_config();
    if config.source != "user" {
        return None;
    }
    // Only treat as override when the key came from the toml file.
    // We re-read keys that exist in defaults too if the user set them — simplest:
    // if source is user, any match in the merged map that the user could have set wins
    // for known keys. Prefer loading toml-only keys.
    let path = pricing_config_path();
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(file) = toml::from_str::<PricingFile>(&text) {
            return match_in_map(model, &file.models);
        }
    }
    None
}

fn match_db_entry(model: &str, entries: &[DbPriceEntry]) -> Option<(ModelPricing, String)> {
    let lowered = model.to_ascii_lowercase();
    let mut best: Option<(usize, i32, &DbPriceEntry)> = None;
    for entry in entries {
        let key = entry.model_id.to_ascii_lowercase();
        let short = key.rsplit('/').next().unwrap_or(&key);
        let or_rank = if entry.source == "openrouter" {
            0
        } else {
            1
        };
        let matched = if lowered == key
            || lowered == short
            || (lowered.contains(short) && short.len() >= 4)
            || (short.contains(&lowered) && lowered.len() >= 4)
        {
            Some(short.len())
        } else {
            None
        };
        let Some(score) = matched else { continue };
        match best {
            Some((s, r, _)) if s > score || (s == score && r <= or_rank) => {}
            _ => best = Some((score, or_rank, entry)),
        }
    }
    best.map(|(_, _, e)| (e.pricing, e.source.clone()))
}

/// Invalidate cached DB prices (called after sync).
pub fn invalidate_price_cache() {
    if let Ok(mut cache) = price_cache().write() {
        cache.loaded = false;
        cache.entries.clear();
    }
}

/// Load latest-per-model prices from usage.db into the in-memory cache.
pub async fn refresh_price_cache_from_db() -> Result<usize, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    type Row = (String, String, f64, f64);
    // Prefer OpenRouter over other sources for the same calendar max date set.
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT p.model_id, p.source, p.input_per_mtok, p.output_per_mtok
         FROM model_price_daily p
         INNER JOIN (
           SELECT model_id, MAX(date) AS max_date FROM model_price_daily
           WHERE source = 'openrouter'
           GROUP BY model_id
         ) latest ON p.model_id = latest.model_id AND p.date = latest.max_date
         WHERE p.source = 'openrouter'",
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut by_model: HashMap<String, DbPriceEntry> = HashMap::new();
    for (model_id, source, input, output) in rows {
        by_model.entry(model_id.clone()).or_insert(DbPriceEntry {
            pricing: ModelPricing {
                input_per_mtok: input,
                output_per_mtok: output,
            },
            source,
            model_id,
        });
    }
    let entries: Vec<DbPriceEntry> = by_model.into_values().collect();
    let n = entries.len();
    if let Ok(mut cache) = price_cache().write() {
        cache.entries = entries;
        cache.loaded = true;
    }
    Ok(n)
}

fn ensure_cache_loaded_blocking() {
    // Cache is populated by sync / refresh_price_cache_from_db / query paths.
    // Never block_on from inside a Tokio worker — that can deadlock.
    let _ = price_cache().read().map(|c| c.loaded);
}

/// Resolve pricing for a full model id (e.g. `claude-sonnet-4-5-20250929`)
/// by longest substring key match. Falls back to the reference model.
pub fn price_for_model(model: &str) -> ModelPricing {
    price_for_model_with_source(model).0
}

pub fn price_for_model_with_source(model: &str) -> (ModelPricing, String) {
    if let Some(p) = user_override_for(model) {
        return (p, "user".into());
    }
    ensure_cache_loaded_blocking();
    if let Ok(cache) = price_cache().read() {
        if let Some((p, src)) = match_db_entry(model, &cache.entries) {
            return (p, src);
        }
    }
    if let Some(p) = match_in_map(model, &pricing_config().models) {
        return (p, pricing_config().source.to_string());
    }
    let r = reference_pricing_with_source();
    (r.0, r.1)
}

/// Pricing for the configured reference model (used when the model is unknown).
pub fn reference_pricing() -> ModelPricing {
    reference_pricing_with_source().0
}

pub fn reference_pricing_with_source() -> (ModelPricing, String) {
    let config = pricing_config();
    price_for_model_with_source(&config.reference_model)
}

pub fn pricing_info() -> PricingInfo {
    let config = pricing_config();
    let (reference, source) = reference_pricing_with_source();
    PricingInfo {
        reference_model: config.reference_model.clone(),
        input_per_mtok: reference.input_per_mtok,
        output_per_mtok: reference.output_per_mtok,
        source,
        config_path: pricing_config_path().display().to_string(),
    }
}

/// USD cost of `tokens` input tokens at the given per-million rate.
pub fn input_cost_usd(tokens: i64, pricing: ModelPricing) -> f64 {
    (tokens.max(0) as f64 / 1_000_000.0) * pricing.input_per_mtok
}

/// Async as-of-date pricing for historical savings bars.
pub async fn price_as_of(model: Option<&str>, date: &str) -> (ModelPricing, String) {
    if let Some(m) = model {
        if let Some(p) = user_override_for(m) {
            return (p, "user".into());
        }
        if let Ok(Some((p, src))) = crate::pricing_sync::lookup_price_as_of(m, date).await {
            return (p, src);
        }
        return price_for_model_with_source(m);
    }
    // Reference model as-of date.
    let ref_model = pricing_config().reference_model.clone();
    if let Ok(Some((p, src))) = crate::pricing_sync::lookup_price_as_of(&ref_model, date).await {
        return (p, src);
    }
    reference_pricing_with_source()
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
        let pricing = ModelPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
        };
        assert_eq!(input_cost_usd(1_000_000, pricing), 3.0);
        assert_eq!(input_cost_usd(0, pricing), 0.0);
        assert_eq!(input_cost_usd(-5, pricing), 0.0);
    }
}
