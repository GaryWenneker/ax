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
///
/// Cache rates stay `None` when the catalog row did not include them.
/// Missing rates are not filled from another model.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ModelPricing {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    #[serde(default)]
    pub cache_read_per_mtok: Option<f64>,
    #[serde(default)]
    pub cache_write_per_mtok: Option<f64>,
}

impl ModelPricing {
    pub const fn rates(input_per_mtok: f64, output_per_mtok: f64) -> Self {
        Self {
            input_per_mtok,
            output_per_mtok,
            cache_read_per_mtok: None,
            cache_write_per_mtok: None,
        }
    }
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
    /// Only the models pricing.toml sets; they win over the synced catalog.
    pub user_models: HashMap<String, ModelPricing>,
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
    m.insert("claude-opus", ModelPricing::rates(5.0, 25.0));
    m.insert("claude-sonnet", ModelPricing::rates(3.0, 15.0));
    m.insert("claude-haiku", ModelPricing::rates(1.0, 5.0));
    m.insert("gpt-5", ModelPricing::rates(1.25, 10.0));
    m.insert("gpt-4o", ModelPricing::rates(2.5, 10.0));
    m.insert("gpt-4.1", ModelPricing::rates(2.0, 8.0));
    m.insert("gemini-2.5-pro", ModelPricing::rates(1.25, 10.0));
    m.insert("gemini", ModelPricing::rates(0.30, 2.5));
    // Cursor Composer labels (e.g. composer-2.5-fast) match via substring "composer".
    m.insert("composer", ModelPricing::rates(1.25, 10.0));
    m.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

pub fn pricing_config_path() -> PathBuf {
    ax_utils::paths::home_dir()
        .map(|h| h.join(".ax").join(PRICING_FILENAME))
        .unwrap_or_else(|| PathBuf::from(".ax").join(PRICING_FILENAME))
}

fn load_config() -> PricingConfig {
    load_config_from(&pricing_config_path())
}

fn load_config_from(path: &std::path::Path) -> PricingConfig {
    let mut config = PricingConfig {
        reference_model: "claude-sonnet".to_string(),
        models: default_models(),
        source: "default",
        user_models: HashMap::new(),
    };
    if let Ok(text) = std::fs::read_to_string(path) {
        if let Ok(file) = toml::from_str::<PricingFile>(&text) {
            if let Some(reference) = file.reference_model {
                config.reference_model = reference;
            }
            for (k, v) in &file.models {
                config.models.insert(k.clone(), *v);
            }
            config.user_models = file.models;
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
    user_override_in(pricing_config(), model)
}

/// The pricing.toml entry for `model`, from the file as loaded at start.
fn user_override_in(config: &PricingConfig, model: &str) -> Option<ModelPricing> {
    match_in_map(model, &config.user_models)
}

/// How well catalog id `catalog_id` matches the lowercased requested model id.
///
/// Higher is better. Match kinds rank first: exact id (the full id or the
/// short id after the last `/`; equal rank, so the caller's source preference
/// decides between providers), requested id contains the short id (the
/// longest such id wins), then short id contains the requested id (the
/// shortest such id wins). `None` when the ids do not match.
pub(crate) fn catalog_match_score(lowered: &str, catalog_id: &str) -> Option<(u8, usize)> {
    let key = catalog_id.to_ascii_lowercase();
    let short = key.rsplit('/').next().unwrap_or(&key);
    if lowered == key || lowered == short {
        Some((3, 0))
    } else if short.len() >= 4 && lowered.contains(short) {
        Some((2, short.len()))
    } else if lowered.len() >= 4 && short.contains(lowered) {
        Some((1, usize::MAX - short.len()))
    } else {
        None
    }
}

fn match_db_entry(model: &str, entries: &[DbPriceEntry]) -> Option<(ModelPricing, String)> {
    let lowered = model.to_ascii_lowercase();
    let mut best: Option<((u8, usize), i32, &DbPriceEntry)> = None;
    for entry in entries {
        let or_rank = if entry.source == "openrouter" { 0 } else { 1 };
        let Some(score) = catalog_match_score(&lowered, &entry.model_id) else {
            continue;
        };
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
    type Row = (String, String, f64, f64, Option<f64>, Option<f64>);
    // Prefer OpenRouter over other sources for the same calendar max date set.
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT p.model_id, p.source, p.input_per_mtok, p.output_per_mtok,
                p.cache_read_per_mtok, p.cache_write_per_mtok
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
    for (model_id, source, input, output, cache_read, cache_write) in rows {
        by_model.entry(model_id.clone()).or_insert(DbPriceEntry {
            pricing: ModelPricing {
                input_per_mtok: input,
                output_per_mtok: output,
                cache_read_per_mtok: cache_read,
                cache_write_per_mtok: cache_write,
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

/// Catalog or user-override price for a cost quote.
///
/// Returns `None` when the model is absent. Does not fall back to the
/// reference model or the built-in savings table.
pub fn price_for_cost(model: &str) -> Option<(ModelPricing, String)> {
    if model.trim().is_empty() {
        return None;
    }
    if let Some(p) = user_override_for(model) {
        return Some((p, "user".into()));
    }
    ensure_cache_loaded_blocking();
    if let Ok(cache) = price_cache().read() {
        if let Some(hit) = match_db_entry(model, &cache.entries) {
            return Some(hit);
        }
    }
    None
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
    // Reference model as-of date, in the same order as a named model.
    let ref_model = pricing_config().reference_model.clone();
    if let Some(p) = user_override_for(&ref_model) {
        return (p, "user".into());
    }
    if let Ok(Some((p, src))) = crate::pricing_sync::lookup_price_as_of(&ref_model, date).await {
        return (p, src);
    }
    reference_pricing_with_source()
}

/// Reference-model pricing as of each date in `dates`, the same answer as
/// [`price_as_of`]`(None, date)` for every date, from one catalog read.
pub async fn reference_prices_as_of(
    pool: &sqlx::SqlitePool,
    dates: &[String],
) -> Vec<(ModelPricing, String)> {
    let ref_model = pricing_config().reference_model.clone();
    if let Some(p) = user_override_for(&ref_model) {
        return vec![(p, "user".into()); dates.len()];
    }
    let Some(last) = dates.iter().max() else {
        return Vec::new();
    };
    let rows = crate::pricing_sync::matching_price_rows(pool, &ref_model, last)
        .await
        .unwrap_or_default();
    dates
        .iter()
        .map(|date| {
            crate::pricing_sync::pick_price_as_of(&ref_model, date, &rows)
                .unwrap_or_else(reference_pricing_with_source)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_override_comes_from_the_loaded_file_without_rereading_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(PRICING_FILENAME);
        std::fs::write(
            &path,
            "[models.my-private-model]\ninput_per_mtok = 7.0\noutput_per_mtok = 9.0\n",
        )
        .unwrap();
        let config = load_config_from(&path);
        assert_eq!(config.source, "user");
        // The file goes away (or changes) after start: the loaded config,
        // which also holds reference_model, stays the single source.
        std::fs::remove_file(&path).unwrap();
        let p = user_override_in(&config, "vendor/my-private-model-v2").expect("override");
        assert_eq!(p.input_per_mtok, 7.0);
        // A default-only key is not a user override.
        assert!(user_override_in(&config, "claude-sonnet-4-5").is_none());
    }

    /// Built-in defaults only: never the developer's own ~/.ax/pricing.toml.
    fn defaults_only() -> PricingConfig {
        let dir = tempfile::tempdir().unwrap();
        load_config_from(&dir.path().join(PRICING_FILENAME))
    }

    #[test]
    fn model_substring_match_prefers_longest_key() {
        let config = defaults_only();
        assert_eq!(config.source, "default");
        let p = match_in_map("claude-sonnet-4-5-20250929", &config.models).expect("sonnet");
        assert_eq!(p.input_per_mtok, 3.0);
        // gemini-2.5-pro should match the specific key, not the generic "gemini"
        let pro = match_in_map("models/gemini-2.5-pro-latest", &config.models).expect("pro");
        assert_eq!(pro.input_per_mtok, 1.25);
    }

    fn db_entry(model_id: &str, input: f64) -> DbPriceEntry {
        DbPriceEntry {
            pricing: ModelPricing::rates(input, input * 4.0),
            source: "openrouter".into(),
            model_id: model_id.into(),
        }
    }

    #[test]
    fn exact_catalog_id_beats_longer_superstring_id() {
        let entries = vec![
            db_entry("openai/gpt-4o-mini-search-preview", 0.15),
            db_entry("openai/gpt-4o", 2.5),
            db_entry("openai/gpt-4o-mini", 0.15),
        ];
        let (p, _) = match_db_entry("gpt-4o", &entries).expect("match");
        assert_eq!(p.input_per_mtok, 2.5);

        let entries = vec![
            db_entry("anthropic/claude-sonnet-4.5", 9.0),
            db_entry("anthropic/claude-sonnet-4", 3.0),
        ];
        let (p, _) = match_db_entry("claude-sonnet-4", &entries).expect("match");
        assert_eq!(p.input_per_mtok, 3.0);
    }

    #[test]
    fn match_kind_order_contained_longest_then_containing_shortest() {
        // Requested id contains a catalog id: the longest contained id wins.
        let entries = vec![
            db_entry("anthropic/claude-sonnet", 1.0),
            db_entry("anthropic/claude-sonnet-4-5", 3.0),
        ];
        let (p, _) = match_db_entry("claude-sonnet-4-5-20250929", &entries).expect("match");
        assert_eq!(p.input_per_mtok, 3.0);
        // Containment of the requested id beats a catalog id that contains it.
        let entries = vec![
            db_entry("openai/gpt-4o-mini-2024", 0.15),
            db_entry("openai/gpt-4o", 2.5),
        ];
        let (p, _) = match_db_entry("gpt-4o-2024-08-06", &entries).expect("match");
        assert_eq!(p.input_per_mtok, 2.5);
        // Only superstrings: the shortest catalog id wins.
        let entries = vec![
            db_entry("openai/o4-mini-high-search", 9.0),
            db_entry("openai/o4-mini-high", 1.1),
        ];
        let (p, _) = match_db_entry("o4-mini", &entries).expect("match");
        assert_eq!(p.input_per_mtok, 1.1);
    }

    #[test]
    fn unknown_model_uses_reference() {
        // Uses the process-wide config, whatever it holds: an unknown model
        // must price the same as the configured reference model.
        let p = price_for_model("some-unknown-model-xyz");
        let r = reference_pricing();
        assert_eq!(p.input_per_mtok, r.input_per_mtok);
    }

    #[test]
    fn cost_math() {
        let pricing = ModelPricing::rates(3.0, 15.0);
        assert_eq!(input_cost_usd(1_000_000, pricing), 3.0);
        assert_eq!(input_cost_usd(0, pricing), 0.0);
        assert_eq!(input_cost_usd(-5, pricing), 0.0);
    }
}
