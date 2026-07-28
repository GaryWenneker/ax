//! Persist daily price snapshots into `~/.ax/usage.db` and refresh the resolver cache.

use chrono::Local;
use serde::Serialize;
use sqlx::SqlitePool;

use crate::pricing::{invalidate_price_cache, refresh_price_cache_from_db};
use crate::pricing_fetch::{
    fetch_all_sources, FetchedBenchmark, FetchedCodingAgent, FetchedPrice, SOURCE_OPENROUTER,
};
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
    pub aa_price_count: usize,
    pub aa_benchmark_count: usize,
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

async fn upsert_prices(pool: &SqlitePool, date: &str, source: &str, rows: &[FetchedPrice]) -> Result<(), String> {
    for row in rows {
        sqlx::query(
            "INSERT INTO model_price_daily
                (date, source, model_id, display_name, provider, input_per_mtok, output_per_mtok,
                 cache_read_per_mtok, blended_3_to_1, context_length, raw_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(date, source, model_id) DO UPDATE SET
                display_name = excluded.display_name,
                provider = excluded.provider,
                input_per_mtok = excluded.input_per_mtok,
                output_per_mtok = excluded.output_per_mtok,
                cache_read_per_mtok = excluded.cache_read_per_mtok,
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
        .bind(row.blended_3_to_1)
        .bind(row.context_length)
        .bind(&row.raw_json)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[allow(dead_code)]
async fn upsert_benchmarks(
    pool: &SqlitePool,
    date: &str,
    source: &str,
    rows: &[FetchedBenchmark],
) -> Result<(), String> {
    for row in rows {
        sqlx::query(
            "INSERT INTO model_benchmark_daily
                (date, source, model_id, display_name, intelligence, coding, agentic,
                 median_output_tps, median_ttft_seconds)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(date, source, model_id) DO UPDATE SET
                display_name = excluded.display_name,
                intelligence = excluded.intelligence,
                coding = excluded.coding,
                agentic = excluded.agentic,
                median_output_tps = excluded.median_output_tps,
                median_ttft_seconds = excluded.median_ttft_seconds",
        )
        .bind(date)
        .bind(source)
        .bind(&row.model_id)
        .bind(&row.display_name)
        .bind(row.intelligence)
        .bind(row.coding)
        .bind(row.agentic)
        .bind(row.median_output_tps)
        .bind(row.median_ttft_seconds)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[allow(dead_code)]
async fn upsert_agents(pool: &SqlitePool, date: &str, rows: &[FetchedCodingAgent]) -> Result<(), String> {
    for row in rows {
        sqlx::query(
            "INSERT INTO coding_agent_daily
                (date, agent, model, index_score, cost_per_task, time_per_task, tokens_per_task, raw_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(date, agent, model) DO UPDATE SET
                index_score = excluded.index_score,
                cost_per_task = excluded.cost_per_task,
                time_per_task = excluded.time_per_task,
                tokens_per_task = excluded.tokens_per_task,
                raw_json = excluded.raw_json",
        )
        .bind(date)
        .bind(&row.agent)
        .bind(&row.model)
        .bind(row.index_score)
        .bind(row.cost_per_task)
        .bind(row.time_per_task)
        .bind(row.tokens_per_task)
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

    let bundle = fetch_all_sources().await?;
    upsert_prices(&pool, &date, SOURCE_OPENROUTER, &bundle.openrouter).await?;

    let or_status = if bundle.openrouter.is_empty() {
        "error"
    } else if bundle.warnings.iter().any(|w| w.contains("OpenRouter")) {
        "partial"
    } else {
        "ok"
    };
    let or_err = if or_status == "ok" {
        None
    } else {
        bundle
            .warnings
            .iter()
            .find(|w| w.contains("OpenRouter"))
            .map(|s| s.as_str())
    };
    record_meta(
        &pool,
        SOURCE_OPENROUTER,
        or_status,
        or_err,
        bundle.openrouter.len() as i64,
        if or_status != "error" {
            Some(&date)
        } else {
            None
        },
    )
    .await?;

    invalidate_price_cache();
    let _ = refresh_price_cache_from_db().await;

    let overall = if bundle.openrouter.is_empty() {
        "error"
    } else if bundle.warnings.is_empty() {
        "ok"
    } else {
        "partial"
    };

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

pub async fn pricing_status() -> Result<PricingStatus, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    let today = today_local();
    let sources: Vec<(
        String,
        Option<i64>,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
    )> = sqlx::query_as(
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
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT date, source, model_id, display_name, provider, input_per_mtok, output_per_mtok,
                cache_read_per_mtok, blended_3_to_1, context_length
         FROM model_price_daily WHERE date = ? AND source = ?
         ORDER BY input_per_mtok ASC, model_id ASC",
    )
    .bind(&date)
    .bind(source)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut out = Vec::with_capacity(rows.len());
    for (
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
    ) in rows
    {
        let bench: Option<(Option<f64>, Option<f64>, Option<f64>)> = sqlx::query_as(
            "SELECT intelligence, coding, agentic FROM model_benchmark_daily
             WHERE date = ? AND model_id = ?
             ORDER BY CASE source WHEN 'artificial_analysis' THEN 0 ELSE 1 END
             LIMIT 1",
        )
        .bind(&date)
        .bind(&model_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| e.to_string())?;
        let (intelligence, coding, agentic) = bench.unwrap_or((None, None, None));
        out.push(PricingCatalogRow {
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
        });
    }
    Ok(out)
}

pub async fn price_history(
    model: &str,
    source: Option<&str>,
    days: i64,
) -> Result<Vec<PricingHistoryPoint>, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    let days = days.clamp(1, 365);
    let like = format!("%{model}%");
    let source = source.unwrap_or(SOURCE_OPENROUTER);
    type Row = (String, String, f64, f64, Option<f64>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT date, source, input_per_mtok, output_per_mtok, blended_3_to_1
         FROM model_price_daily
         WHERE source = ? AND (model_id LIKE ? OR display_name LIKE ?)
         ORDER BY date DESC
         LIMIT ?",
    )
    .bind(source)
    .bind(&like)
    .bind(&like)
    .bind(days)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;
    let mut out: Vec<PricingHistoryPoint> = rows
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
        .collect();
    out.reverse();
    Ok(out)
}

pub async fn list_coding_agents(days: i64) -> Result<Vec<CodingAgentRow>, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    let _ = days.clamp(1, 365);
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
         ORDER BY date DESC, agent ASC
         LIMIT 500",
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(
            |(
                date,
                agent,
                model,
                index_score,
                cost_per_task,
                time_per_task,
                tokens_per_task,
            )| CodingAgentRow {
                date,
                agent,
                model,
                index_score,
                cost_per_task,
                time_per_task,
                tokens_per_task,
            },
        )
        .collect())
}

/// Look up input/output rates for a model as of a calendar date (or latest ≤ date).
pub async fn lookup_price_as_of(
    model: &str,
    date: &str,
) -> Result<Option<(crate::pricing::ModelPricing, String)>, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    let lowered = model.to_ascii_lowercase();
    type Row = (f64, f64, String, String);
    // Prefer AA over OpenRouter when both match.
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT input_per_mtok, output_per_mtok, source, model_id
         FROM model_price_daily
         WHERE date <= ?
         ORDER BY date DESC,
                  CASE source WHEN 'artificial_analysis' THEN 0 ELSE 1 END",
    )
    .bind(date)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut best: Option<(usize, i32, Row)> = None;
    for row in rows {
        let key = row.3.to_ascii_lowercase();
        let short = key.rsplit('/').next().unwrap_or(&key).to_string();
        let or_rank = if row.2 == "openrouter" { 0 } else { 1 };
        let matched = if lowered == key
            || lowered == short
            || lowered.contains(&short) && short.len() >= 4
            || short.contains(&lowered) && lowered.len() >= 4
        {
            Some(short.len())
        } else {
            None
        };
        let Some(score) = matched else { continue };
        match &best {
            Some((s, r, _)) if *s > score || (*s == score && *r <= or_rank) => {}
            _ => best = Some((score, or_rank, row)),
        }
    }
    Ok(best.map(|(_, _, (input, output, source, _))| {
        (
            crate::pricing::ModelPricing {
                input_per_mtok: input,
                output_per_mtok: output,
            },
            source,
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pricing_fetch::{
        parse_aa_models, parse_openrouter_models, SOURCE_ARTIFICIAL_ANALYSIS,
    };
    use serde_json::json;
    use std::sync::Mutex;

    static DB_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn temp_db(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("ax-pricing-test-{name}-{}.db", std::process::id()));
        p
    }

    #[tokio::test]
    async fn upsert_and_history() {
        let _guard = DB_TEST_LOCK.lock().unwrap();
        let db = temp_db("hist");
        let _ = std::fs::remove_file(&db);
        std::env::set_var("AX_USAGE_DB", &db);

        let pool = open_pool().await.expect("pool");
        let body = json!({
            "data": [{
                "id": "anthropic/claude-sonnet-4",
                "name": "Claude Sonnet 4",
                "pricing": { "prompt": "0.000003", "completion": "0.000015" }
            }]
        });
        let rows = parse_openrouter_models(&body);
        upsert_prices(&pool, "2026-07-25", SOURCE_OPENROUTER, &rows)
            .await
            .unwrap();
        upsert_prices(&pool, "2026-07-26", SOURCE_OPENROUTER, &rows)
            .await
            .unwrap();

        let hist = price_history("claude-sonnet", Some(SOURCE_OPENROUTER), 30)
            .await
            .unwrap();
        assert!(hist.len() >= 2);
        assert!((hist[0].input_per_mtok - 3.0).abs() < 1e-9);

        let (aa_prices, aa_benches) = parse_aa_models(&json!({
            "data": [{
                "slug": "claude-sonnet-4",
                "name": "Claude Sonnet 4",
                "pricing": { "price_1m_input_tokens": 2.5, "price_1m_output_tokens": 12.0 },
                "evaluations": { "artificial_analysis_coding_index": 55.0 }
            }]
        }));
        upsert_prices(&pool, "2026-07-26", SOURCE_ARTIFICIAL_ANALYSIS, &aa_prices)
            .await
            .unwrap();
        upsert_benchmarks(&pool, "2026-07-26", SOURCE_ARTIFICIAL_ANALYSIS, &aa_benches)
            .await
            .unwrap();

        let found = lookup_price_as_of("claude-sonnet-4", "2026-07-26")
            .await
            .unwrap()
            .expect("price");
        // OpenRouter wins when both sources have a row for the same day.
        assert_eq!(found.1, SOURCE_OPENROUTER);
        assert!((found.0.input_per_mtok - 3.0).abs() < 1e-9);

        std::env::remove_var("AX_USAGE_DB");
        let _ = std::fs::remove_file(db);
    }
}
