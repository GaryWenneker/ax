//! Persist daily price snapshots into `~/.ax/usage.db` and refresh the resolver cache.

use chrono::Local;
use serde::Serialize;
use sqlx::SqlitePool;

use crate::pricing::{invalidate_price_cache, refresh_price_cache_from_db};
use crate::pricing_fetch::{fetch_all_sources, FetchedPrice, SOURCE_OPENROUTER};
use crate::store::open_pool;

#[derive(Debug, Clone, Serialize)]
pub struct SourceSyncStatus {
    pub source: String,
    pub last_attempt_at: Option<i64>,
    pub last_success_at: Option<i64>,
    pub last_success_date: Option<String>,
    pub status: Option<String>,
    pub error: Option<String>,
    pub models_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PricingSyncReport {
    pub date: String,
    pub forced: bool,
    pub skipped: bool,
    pub openrouter_count: usize,
    /// Always 0: only OpenRouter is fetched. Kept so the web API shape does
    /// not change for clients that read these fields.
    pub aa_price_count: usize,
    /// Always 0; see `aa_price_count`.
    pub aa_benchmark_count: usize,
    /// Always 0; see `aa_price_count`.
    pub coding_agent_count: usize,
    pub warnings: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PricingCatalogRow {
    pub date: String,
    pub source: String,
    pub model_id: String,
    pub display_name: Option<String>,
    pub provider: Option<String>,
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_read_per_mtok: Option<f64>,
    pub blended_3_to_1: Option<f64>,
    pub context_length: Option<i64>,
    pub intelligence: Option<f64>,
    pub coding: Option<f64>,
    pub agentic: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PricingHistoryPoint {
    pub date: String,
    pub source: String,
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub blended_3_to_1: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodingAgentRow {
    pub date: String,
    pub agent: String,
    pub model: Option<String>,
    pub index_score: Option<f64>,
    pub cost_per_task: Option<f64>,
    pub time_per_task: Option<f64>,
    pub tokens_per_task: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PricingStatus {
    pub today: String,
    pub synced_today: bool,
    pub sources: Vec<SourceSyncStatus>,
    pub price_rows: i64,
    pub benchmark_rows: i64,
    pub agent_rows: i64,
    pub aa_key_configured: bool,
}

fn today_local() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

async fn record_meta(
    pool: &SqlitePool,
    source: &str,
    status: &str,
    error: Option<&str>,
    models_count: i64,
    success_date: Option<&str>,
) -> Result<(), String> {
    let now = now_ms();
    let success_at = if status == "ok" || status == "partial" {
        Some(now)
    } else {
        None
    };
    sqlx::query(
        "INSERT INTO pricing_sync_meta
            (source, last_attempt_at, last_success_at, last_success_date, status, error, models_count)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(source) DO UPDATE SET
            last_attempt_at = excluded.last_attempt_at,
            last_success_at = COALESCE(excluded.last_success_at, pricing_sync_meta.last_success_at),
            last_success_date = COALESCE(excluded.last_success_date, pricing_sync_meta.last_success_date),
            status = excluded.status,
            error = excluded.error,
            models_count = excluded.models_count",
    )
    .bind(source)
    .bind(now)
    .bind(success_at)
    .bind(success_date)
    .bind(status)
    .bind(error)
    .bind(models_count)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

async fn upsert_prices(
    pool: &SqlitePool,
    date: &str,
    source: &str,
    rows: &[FetchedPrice],
) -> Result<(), String> {
    for row in rows {
        sqlx::query(
            "INSERT INTO model_price_daily
                (date, source, model_id, display_name, provider, input_per_mtok, output_per_mtok,
                 cache_read_per_mtok, cache_write_per_mtok, blended_3_to_1, context_length, raw_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(date, source, model_id) DO UPDATE SET
                display_name = excluded.display_name,
                provider = excluded.provider,
                input_per_mtok = excluded.input_per_mtok,
                output_per_mtok = excluded.output_per_mtok,
                cache_read_per_mtok = excluded.cache_read_per_mtok,
                cache_write_per_mtok = excluded.cache_write_per_mtok,
                blended_3_to_1 = excluded.blended_3_to_1,
                context_length = excluded.context_length,
                raw_json = excluded.raw_json",
        )
        .bind(date)
        .bind(source)
        .bind(&row.model_id)
        .bind(&row.display_name)
        .bind(&row.provider)
        .bind(row.pricing.input_per_mtok)
        .bind(row.pricing.output_per_mtok)
        .bind(row.cache_read_per_mtok)
        .bind(row.cache_write_per_mtok)
        .bind(row.blended_3_to_1)
        .bind(row.context_length)
        .bind(&row.raw_json)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

async fn any_success_today(pool: &SqlitePool, date: &str) -> Result<bool, String> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM pricing_sync_meta
         WHERE last_success_date = ? AND status IN ('ok', 'partial')
         LIMIT 1",
    )
    .bind(date)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(row.is_some())
}

/// Force or no-op daily sync depending on `force` and whether today already succeeded.
pub async fn sync_pricing(force: bool) -> Result<PricingSyncReport, String> {
    sync_pricing_with(force, fetch_all_sources).await
}

async fn sync_pricing_with<F, Fut>(force: bool, fetch: F) -> Result<PricingSyncReport, String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<crate::pricing_fetch::FetchBundle, String>>,
{
    let date = today_local();
    let pool = open_pool().await.map_err(|e| e.to_string())?;

    if !force && any_success_today(&pool, &date).await? {
        return Ok(PricingSyncReport {
            date,
            forced: false,
            skipped: true,
            openrouter_count: 0,
            aa_price_count: 0,
            aa_benchmark_count: 0,
            coding_agent_count: 0,
            warnings: vec!["already synced today".into()],
            status: "skipped".into(),
        });
    }

    // A failed fetch is recorded so the status shows it, while the last
    // successful date and time are kept.
    let bundle = match fetch().await {
        Ok(bundle) => bundle,
        Err(err) => {
            record_meta(&pool, SOURCE_OPENROUTER, "error", Some(&err), 0, None).await?;
            return Err(err);
        }
    };
    upsert_prices(&pool, &date, SOURCE_OPENROUTER, &bundle.openrouter).await?;

    // An Ok bundle always has OpenRouter models; warnings make it partial.
    let overall = if bundle.warnings.is_empty() {
        "ok"
    } else {
        "partial"
    };
    let warning = bundle.warnings.first().map(String::as_str);
    record_meta(
        &pool,
        SOURCE_OPENROUTER,
        overall,
        warning,
        bundle.openrouter.len() as i64,
        Some(&date),
    )
    .await?;

    invalidate_price_cache();
    let _ = refresh_price_cache_from_db().await;

    Ok(PricingSyncReport {
        date,
        forced: force,
        skipped: false,
        openrouter_count: bundle.openrouter.len(),
        aa_price_count: 0,
        aa_benchmark_count: 0,
        coding_agent_count: 0,
        warnings: bundle.warnings,
        status: overall.into(),
    })
}

/// Background-friendly: sync only if today has no successful snapshot yet.
pub async fn ensure_daily_pricing_sync() {
    match sync_pricing(false).await {
        Ok(report) => {
            if report.skipped {
                tracing::debug!("pricing sync skipped (already synced today)");
            } else {
                tracing::info!(
                    "pricing sync {}: openrouter={}",
                    report.status,
                    report.openrouter_count
                );
            }
        }
        Err(e) => tracing::warn!("pricing sync failed: {e}"),
    }
}

pub fn spawn_ensure_daily_pricing_sync() {
    tokio::spawn(async {
        ensure_daily_pricing_sync().await;
    });
}

/// `(source, last_attempt_at, last_success_at, last_success_date, status, error, models_count)`
type SyncMetaRow = (
    String,
    Option<i64>,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
);

pub async fn pricing_status() -> Result<PricingStatus, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    let today = today_local();
    let sources: Vec<SyncMetaRow> = sqlx::query_as(
        "SELECT source, last_attempt_at, last_success_at, last_success_date, status, error, models_count
         FROM pricing_sync_meta ORDER BY source",
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let price_rows: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM model_price_daily")
        .fetch_one(&pool)
        .await
        .map_err(|e| e.to_string())?;
    let benchmark_rows: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM model_benchmark_daily")
        .fetch_one(&pool)
        .await
        .map_err(|e| e.to_string())?;
    let agent_rows: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM coding_agent_daily")
        .fetch_one(&pool)
        .await
        .map_err(|e| e.to_string())?;

    let synced_today = any_success_today(&pool, &today).await?;
    Ok(PricingStatus {
        today,
        synced_today,
        sources: sources
            .into_iter()
            .map(
                |(
                    source,
                    last_attempt_at,
                    last_success_at,
                    last_success_date,
                    status,
                    error,
                    models_count,
                )| SourceSyncStatus {
                    source,
                    last_attempt_at,
                    last_success_at,
                    last_success_date,
                    status,
                    error,
                    models_count,
                },
            )
            .collect(),
        price_rows: price_rows.0,
        benchmark_rows: benchmark_rows.0,
        agent_rows: agent_rows.0,
        aa_key_configured: crate::pricing_fetch::aa_api_key().is_some(),
    })
}

pub async fn list_latest_prices(source: Option<&str>) -> Result<Vec<PricingCatalogRow>, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    let source = source.unwrap_or(SOURCE_OPENROUTER);
    let latest: Option<(String,)> =
        sqlx::query_as("SELECT MAX(date) FROM model_price_daily WHERE source = ?")
            .bind(source)
            .fetch_optional(&pool)
            .await
            .map_err(|e| e.to_string())?;
    let Some((date,)) = latest.filter(|(d,)| !d.is_empty()) else {
        return Ok(Vec::new());
    };

    type Row = (
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        f64,
        f64,
        Option<f64>,
        Option<f64>,
        Option<i64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
    );
    // Benchmark scores come from the same day's model_benchmark_daily row, in
    // one query; Artificial Analysis scores win when several sources have one.
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT p.date, p.source, p.model_id, p.display_name, p.provider, p.input_per_mtok,
                p.output_per_mtok, p.cache_read_per_mtok, p.blended_3_to_1, p.context_length,
                b.intelligence, b.coding, b.agentic
         FROM model_price_daily p
         LEFT JOIN model_benchmark_daily b
           ON b.rowid = (
             SELECT b2.rowid FROM model_benchmark_daily b2
             WHERE b2.date = p.date AND b2.model_id = p.model_id
             ORDER BY CASE b2.source WHEN 'artificial_analysis' THEN 0 ELSE 1 END, b2.source
             LIMIT 1)
         WHERE p.date = ? AND p.source = ?
         ORDER BY p.input_per_mtok ASC, p.model_id ASC",
    )
    .bind(&date)
    .bind(source)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(
            |(
                date,
                source,
                model_id,
                display_name,
                provider,
                input_per_mtok,
                output_per_mtok,
                cache_read_per_mtok,
                blended_3_to_1,
                context_length,
                intelligence,
                coding,
                agentic,
            )| PricingCatalogRow {
                date,
                source,
                model_id,
                display_name,
                provider,
                input_per_mtok,
                output_per_mtok,
                cache_read_per_mtok,
                blended_3_to_1,
                context_length,
                intelligence,
                coding,
                agentic,
            },
        )
        .collect())
}

pub async fn price_history(
    model: &str,
    source: Option<&str>,
    days: i64,
) -> Result<Vec<PricingHistoryPoint>, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    let days = days.clamp(1, 365);
    let source = source.unwrap_or(SOURCE_OPENROUTER);
    let Some(model_id) = resolve_catalog_model(&pool, source, model).await? else {
        return Ok(Vec::new());
    };
    let since = (Local::now().date_naive() - chrono::Duration::days(days - 1))
        .format("%Y-%m-%d")
        .to_string();
    type Row = (String, String, f64, f64, Option<f64>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT date, source, input_per_mtok, output_per_mtok, blended_3_to_1
         FROM model_price_daily
         WHERE source = ? AND model_id = ? AND date >= ?
         ORDER BY date ASC",
    )
    .bind(source)
    .bind(&model_id)
    .bind(&since)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(
            |(date, source, input_per_mtok, output_per_mtok, blended_3_to_1)| PricingHistoryPoint {
                date,
                source,
                input_per_mtok,
                output_per_mtok,
                blended_3_to_1,
            },
        )
        .collect())
}

/// Resolve a requested model name to one catalog `model_id` of `source`, using
/// the same matcher as cost pricing; the display name counts as a second key.
/// Ties go to the alphabetically first id so the answer is stable.
async fn resolve_catalog_model(
    pool: &SqlitePool,
    source: &str,
    model: &str,
) -> Result<Option<String>, String> {
    let lowered = model.trim().to_ascii_lowercase();
    if lowered.is_empty() {
        return Ok(None);
    }
    let ids: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT model_id, MAX(display_name) FROM model_price_daily
         WHERE source = ? GROUP BY model_id ORDER BY model_id",
    )
    .bind(source)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let mut best: Option<((u8, usize), String)> = None;
    for (id, display) in ids {
        let by_id = crate::pricing::catalog_match_score(&lowered, &id);
        let by_name = display
            .as_deref()
            .and_then(|d| crate::pricing::catalog_match_score(&lowered, d));
        let Some(score) = by_id.max(by_name) else {
            continue;
        };
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, id));
        }
    }
    Ok(best.map(|(_, id)| id))
}

pub async fn list_coding_agents(days: i64) -> Result<Vec<CodingAgentRow>, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    // Rows from the last `days` calendar days, today included.
    let since = (Local::now().date_naive() - chrono::Duration::days(days.clamp(1, 365) - 1))
        .format("%Y-%m-%d")
        .to_string();
    type Row = (
        String,
        String,
        Option<String>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT date, agent, model, index_score, cost_per_task, time_per_task, tokens_per_task
         FROM coding_agent_daily
         WHERE date >= ?
         ORDER BY date DESC, agent ASC
         LIMIT 500",
    )
    .bind(since)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(
            |(date, agent, model, index_score, cost_per_task, time_per_task, tokens_per_task)| {
                CodingAgentRow {
                    date,
                    agent,
                    model,
                    index_score,
                    cost_per_task,
                    time_per_task,
                    tokens_per_task,
                }
            },
        )
        .collect())
}

/// One catalog price row: `(date, input, output, cache_read, cache_write, source, model_id)`.
pub(crate) type CatalogPriceRow = (String, f64, f64, Option<f64>, Option<f64>, String, String);

/// Catalog rows on or before `until` whose model id matches `model`, newest
/// first and OpenRouter first within a day. Model ids are matched
/// once; only the matching models' rows are read.
pub(crate) async fn matching_price_rows(
    pool: &SqlitePool,
    model: &str,
    until: &str,
) -> Result<Vec<CatalogPriceRow>, String> {
    #[cfg(test)]
    CATALOG_SCANS.with(|n| n.set(n.get() + 1));
    let lowered = model.to_ascii_lowercase();
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT model_id FROM model_price_daily WHERE date <= ?")
            .bind(until)
            .fetch_all(pool)
            .await
            .map_err(|e| e.to_string())?;
    let matching: Vec<String> = ids
        .into_iter()
        .filter(|id| crate::pricing::catalog_match_score(&lowered, id).is_some())
        .collect();
    if matching.is_empty() {
        return Ok(Vec::new());
    }
    let ids_json = serde_json::to_string(&matching).map_err(|e| e.to_string())?;
    sqlx::query_as(
        "SELECT date, input_per_mtok, output_per_mtok, cache_read_per_mtok, cache_write_per_mtok,
                source, model_id
         FROM model_price_daily
         WHERE date <= ? AND model_id IN (SELECT value FROM json_each(?))
         ORDER BY date DESC,
                  CASE source WHEN 'openrouter' THEN 0 ELSE 1 END",
    )
    .bind(until)
    .bind(ids_json)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

/// Pick the price for `model` as of `date` from rows ordered as
/// [`matching_price_rows`] returns them: the best id match wins, then
/// OpenRouter, then the newest row.
pub(crate) fn pick_price_as_of(
    model: &str,
    date: &str,
    rows: &[CatalogPriceRow],
) -> Option<(crate::pricing::ModelPricing, String)> {
    let lowered = model.to_ascii_lowercase();
    let mut best: Option<((u8, usize), i32, &CatalogPriceRow)> = None;
    for row in rows.iter().filter(|row| row.0.as_str() <= date) {
        let or_rank = if row.5 == "openrouter" { 0 } else { 1 };
        let Some(score) = crate::pricing::catalog_match_score(&lowered, &row.6) else {
            continue;
        };
        match &best {
            Some((s, r, _)) if *s > score || (*s == score && *r <= or_rank) => {}
            _ => best = Some((score, or_rank, row)),
        }
    }
    best.map(
        |(_, _, (_, input, output, cache_read, cache_write, source, _))| {
            (
                crate::pricing::ModelPricing {
                    input_per_mtok: *input,
                    output_per_mtok: *output,
                    cache_read_per_mtok: *cache_read,
                    cache_write_per_mtok: *cache_write,
                },
                source.clone(),
            )
        },
    )
}

#[cfg(test)]
thread_local! {
    /// Catalog reads made on this thread, so tests can count them.
    pub(crate) static CATALOG_SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Look up input/output rates for a model as of a calendar date (or latest ≤ date).
pub async fn lookup_price_as_of(
    model: &str,
    date: &str,
) -> Result<Option<(crate::pricing::ModelPricing, String)>, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    let rows = matching_price_rows(&pool, model, date).await?;
    Ok(pick_price_as_of(model, date, &rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pricing_fetch::{parse_openrouter_models, SOURCE_ARTIFICIAL_ANALYSIS};
    use serde_json::json;

    use crate::store::UsageDbEnv;

    #[tokio::test]
    async fn lookup_price_as_of_prefers_exact_id_over_superstring() {
        let _env = UsageDbEnv::new().await;

        let pool = open_pool().await.expect("pool");
        let rows = parse_openrouter_models(&json!({
            "data": [
                {
                    "id": "openai/gpt-4o-mini-search-preview",
                    "name": "GPT-4o mini search",
                    "pricing": { "prompt": "0.00000015", "completion": "0.0000006" }
                },
                {
                    "id": "openai/gpt-4o",
                    "name": "GPT-4o",
                    "pricing": { "prompt": "0.0000025", "completion": "0.00001" }
                }
            ]
        }));
        upsert_prices(&pool, "2026-07-26", SOURCE_OPENROUTER, &rows)
            .await
            .unwrap();
        let found = lookup_price_as_of("gpt-4o", "2026-07-26")
            .await
            .unwrap()
            .expect("price");
        assert!(
            (found.0.input_per_mtok - 2.5).abs() < 1e-9,
            "gpt-4o priced at {}",
            found.0.input_per_mtok
        );
    }

    #[tokio::test]
    async fn failed_sync_is_recorded_as_error() {
        let _env = UsageDbEnv::new().await;
        let pool = open_pool().await.expect("pool");
        // A previous good run.
        record_meta(&pool, SOURCE_OPENROUTER, "ok", None, 10, Some("2026-01-01"))
            .await
            .unwrap();

        let result = sync_pricing_with(true, || async {
            Err::<crate::pricing_fetch::FetchBundle, String>("OpenRouter fetch: timed out".into())
        })
        .await;
        let status = pricing_status().await.unwrap();

        assert!(result.is_err());
        let or = status
            .sources
            .iter()
            .find(|s| s.source == SOURCE_OPENROUTER)
            .expect("openrouter meta row");
        assert_eq!(or.status.as_deref(), Some("error"));
        assert!(or.error.as_deref().is_some_and(|e| e.contains("timed out")));
        assert_eq!(or.last_success_date.as_deref(), Some("2026-01-01"));
    }

    #[tokio::test]
    async fn successful_sync_is_recorded_as_ok() {
        let _env = UsageDbEnv::new().await;
        let _ = open_pool().await.expect("pool");
        let result = sync_pricing_with(true, || async {
            Ok(crate::pricing_fetch::FetchBundle {
                openrouter: parse_openrouter_models(&json!({
                    "data": [{
                        "id": "openai/gpt-4o",
                        "name": "GPT-4o",
                        "pricing": { "prompt": "0.0000025", "completion": "0.00001" }
                    }]
                })),
                ..Default::default()
            })
        })
        .await;
        let status = pricing_status().await.unwrap();

        let report = result.expect("sync ok");
        assert_eq!(report.status, "ok");
        let or = status
            .sources
            .iter()
            .find(|s| s.source == SOURCE_OPENROUTER)
            .expect("openrouter meta row");
        assert_eq!(or.status.as_deref(), Some("ok"));
        assert_eq!(or.error, None);
        assert_eq!(or.models_count, Some(1));
        assert!(status.synced_today);
    }

    fn days_ago(n: i64) -> String {
        (Local::now().date_naive() - chrono::Duration::days(n))
            .format("%Y-%m-%d")
            .to_string()
    }

    #[tokio::test]
    async fn price_history_is_one_model_over_calendar_days() {
        let _env = UsageDbEnv::new().await;
        let pool = open_pool().await.expect("pool");
        let rows = parse_openrouter_models(&json!({
            "data": [
                {
                    "id": "anthropic/claude-sonnet-4",
                    "name": "Claude Sonnet 4",
                    "pricing": { "prompt": "0.000003", "completion": "0.000015" }
                },
                {
                    "id": "anthropic/claude-sonnet-4.5",
                    "name": "Claude Sonnet 4.5",
                    "pricing": { "prompt": "0.000005", "completion": "0.000025" }
                }
            ]
        }));
        for n in 0..5 {
            upsert_prices(&pool, &days_ago(n), SOURCE_OPENROUTER, &rows)
                .await
                .unwrap();
        }
        let exact = price_history("claude-sonnet-4", None, 3).await.unwrap();
        let wildcard = price_history("%", None, 30).await.unwrap();

        // Three calendar days of one model, oldest first.
        assert_eq!(exact.len(), 3, "{exact:?}");
        assert!(exact.iter().all(|p| (p.input_per_mtok - 3.0).abs() < 1e-9));
        assert_eq!(exact[0].date, days_ago(2));
        assert_eq!(exact[2].date, days_ago(0));
        // `%` is not a wildcard.
        assert!(wildcard.is_empty(), "{wildcard:?}");
    }

    #[tokio::test]
    async fn coding_agents_honour_the_days_window() {
        let _env = UsageDbEnv::new().await;
        let pool = open_pool().await.expect("pool");
        for (date, agent) in [(days_ago(0), "recent"), (days_ago(40), "old")] {
            sqlx::query("INSERT INTO coding_agent_daily (date, agent, model) VALUES (?, ?, 'm')")
                .bind(date)
                .bind(agent)
                .execute(&pool)
                .await
                .unwrap();
        }
        let week: Vec<String> = list_coding_agents(7)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.agent)
            .collect();
        assert_eq!(week, ["recent"]);
        assert_eq!(list_coding_agents(60).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn catalog_rows_carry_their_benchmark_scores() {
        let _env = UsageDbEnv::new().await;
        let pool = open_pool().await.expect("pool");
        let today = days_ago(0);
        let rows = parse_openrouter_models(&json!({
            "data": [
                { "id": "a/scored", "pricing": { "prompt": "0.000001", "completion": "0.000002" } },
                { "id": "a/plain", "pricing": { "prompt": "0.000003", "completion": "0.000004" } }
            ]
        }));
        upsert_prices(&pool, &today, SOURCE_OPENROUTER, &rows)
            .await
            .unwrap();
        for (source, coding) in [("other", 10.0), (SOURCE_ARTIFICIAL_ANALYSIS, 55.0)] {
            sqlx::query(
                "INSERT INTO model_benchmark_daily (date, source, model_id, coding) VALUES (?, ?, 'a/scored', ?)",
            )
            .bind(&today)
            .bind(source)
            .bind(coding)
            .execute(&pool)
            .await
            .unwrap();
        }
        let catalog = list_latest_prices(None).await.unwrap();
        let by_id = |id: &str| catalog.iter().find(|r| r.model_id == id).unwrap().coding;
        assert_eq!(catalog.len(), 2);
        assert_eq!(by_id("a/scored"), Some(55.0));
        assert_eq!(by_id("a/plain"), None);
    }

    #[tokio::test]
    async fn upsert_and_history() {
        let _env = UsageDbEnv::new().await;

        let pool = open_pool().await.expect("pool");
        let (yesterday, today) = (days_ago(1), days_ago(0));
        let body = json!({
            "data": [{
                "id": "anthropic/claude-sonnet-4",
                "name": "Claude Sonnet 4",
                "pricing": { "prompt": "0.000003", "completion": "0.000015" }
            }]
        });
        let rows = parse_openrouter_models(&body);
        upsert_prices(&pool, &yesterday, SOURCE_OPENROUTER, &rows)
            .await
            .unwrap();
        upsert_prices(&pool, &today, SOURCE_OPENROUTER, &rows)
            .await
            .unwrap();

        let hist = price_history("claude-sonnet", Some(SOURCE_OPENROUTER), 30)
            .await
            .unwrap();
        assert!(hist.len() >= 2);
        assert!((hist[0].input_per_mtok - 3.0).abs() < 1e-9);

        // Another source with a different price for the same model and day.
        let mut other = rows.clone();
        other[0].model_id = "claude-sonnet-4".into();
        other[0].pricing.input_per_mtok = 2.5;
        upsert_prices(&pool, &today, SOURCE_ARTIFICIAL_ANALYSIS, &other)
            .await
            .unwrap();

        let found = lookup_price_as_of("claude-sonnet-4", &today)
            .await
            .unwrap()
            .expect("price");
        // OpenRouter wins when both sources have a row for the same day.
        assert_eq!(found.1, SOURCE_OPENROUTER);
        assert!((found.0.input_per_mtok - 3.0).abs() < 1e-9);
    }
}
