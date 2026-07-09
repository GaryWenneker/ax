//! SQLite persistence for token usage events.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous};
use sqlx::ConnectOptions;

use ax_utils::errors::{AxError, DatabaseError};

use crate::period::{resolve_period, UsagePeriod};

pub const USAGE_DB_FILENAME: &str = "usage.db";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS token_usage (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  model TEXT NOT NULL,
  prompt_tokens INTEGER NOT NULL DEFAULT 0,
  completion_tokens INTEGER NOT NULL DEFAULT 0,
  total_tokens INTEGER NOT NULL DEFAULT 0,
  source TEXT NOT NULL,
  project TEXT,
  created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_token_usage_created_at ON token_usage(created_at);
CREATE INDEX IF NOT EXISTS idx_token_usage_model ON token_usage(model);
";

pub fn usage_db_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".ax").join(USAGE_DB_FILENAME))
        .unwrap_or_else(|| PathBuf::from(".ax").join(USAGE_DB_FILENAME))
}

pub async fn open_pool() -> Result<SqlitePool, AxError> {
    let path = usage_db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    }

    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .disable_statement_logging();

    let pool = SqlitePoolOptions::new()
        .max_connections(3)
        .connect_with(options)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;

    for stmt in SCHEMA.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        sqlx::query(stmt)
            .execute(&pool)
            .await
            .map_err(|e| AxError::Database(DatabaseError::new(format!("usage schema: {e}"))))?;
    }

    Ok(pool)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub source: String,
    pub project: Option<String>,
}

/// Persist usage (awaited — safe for short-lived CLI processes).
pub async fn record_usage(record: UsageRecord) {
    if let Ok(pool) = open_pool().await {
        let _ = insert_usage(&pool, &record).await;
    }
}

/// Returns `true` when token counts were parsed and persisted.
pub async fn record_from_response(
    model: &str,
    source: &str,
    project: Option<&str>,
    response: &serde_json::Value,
) -> bool {
    let Some((prompt, completion, total)) = parse_usage(response) else {
        return false;
    };
    record_usage(UsageRecord {
        model: model.to_string(),
        prompt_tokens: prompt,
        completion_tokens: completion,
        total_tokens: total,
        source: source.to_string(),
        project: project.map(str::to_string),
    })
    .await;
    true
}

fn json_i64(v: &serde_json::Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_u64().and_then(|u| i64::try_from(u).ok()))
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn usage_block<'a>(data: &'a serde_json::Value) -> Option<&'a serde_json::Value> {
    if let Some(usage) = data.get("usage") {
        return Some(usage);
    }
    if data.get("prompt_eval_count").is_some() || data.get("eval_count").is_some() {
        return Some(data);
    }
    None
}

pub fn parse_usage(data: &serde_json::Value) -> Option<(i64, i64, i64)> {
    let usage = usage_block(data)?;
    let mut prompt = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("input_tokens"))
        .or_else(|| usage.get("prompt_eval_count"))
        .and_then(json_i64)
        .unwrap_or(0);
    for key in ["cache_read_input_tokens", "cache_creation_input_tokens"] {
        if let Some(n) = usage.get(key).and_then(json_i64) {
            prompt += n;
        }
    }
    let completion = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .or_else(|| usage.get("eval_count"))
        .and_then(json_i64)
        .unwrap_or(0);
    let total = usage
        .get("total_tokens")
        .and_then(json_i64)
        .unwrap_or(prompt + completion);
    if prompt == 0 && completion == 0 && total == 0 {
        return None;
    }
    Some((prompt, completion, total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn openai_usage() {
        let data = json!({
            "usage": { "prompt_tokens": 100, "completion_tokens": 50, "total_tokens": 150 }
        });
        assert_eq!(parse_usage(&data), Some((100, 50, 150)));
    }

    #[test]
    fn anthropic_usage() {
        let data = json!({
            "usage": { "input_tokens": 80, "output_tokens": 40 }
        });
        assert_eq!(parse_usage(&data), Some((80, 40, 120)));
    }

    #[test]
    fn anthropic_additive_cache() {
        let data = json!({
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5,
                "cache_read_input_tokens": 200,
                "cache_creation_input_tokens": 50
            }
        });
        assert_eq!(parse_usage(&data), Some((260, 5, 265)));
    }

    #[test]
    fn ollama_native_root_fields() {
        let data = json!({ "prompt_eval_count": 12, "eval_count": 8 });
        assert_eq!(parse_usage(&data), Some((12, 8, 20)));
    }

    #[test]
    fn string_encoded_counts() {
        let data = json!({
            "usage": { "prompt_tokens": "30", "completion_tokens": "20", "total_tokens": "50" }
        });
        assert_eq!(parse_usage(&data), Some((30, 20, 50)));
    }

    #[test]
    fn total_only() {
        let data = json!({ "usage": { "total_tokens": 99 } });
        assert_eq!(parse_usage(&data), Some((0, 0, 99)));
    }

    #[test]
    fn missing_usage_returns_none() {
        assert_eq!(parse_usage(&json!({ "choices": [] })), None);
    }

    #[test]
    fn all_zeros_returns_none() {
        let data = json!({ "usage": { "prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0 } });
        assert_eq!(parse_usage(&data), None);
    }
}

async fn insert_usage(pool: &SqlitePool, record: &UsageRecord) -> Result<(), AxError> {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        "INSERT INTO token_usage (model, prompt_tokens, completion_tokens, total_tokens, source, project, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&record.model)
    .bind(record.prompt_tokens)
    .bind(record.completion_tokens)
    .bind(record.total_tokens)
    .bind(&record.source)
    .bind(&record.project)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct UsageQuery {
    pub period: UsagePeriod,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelUsageSummary {
    pub model: String,
    pub calls: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyUsage {
    pub date: String,
    pub total_tokens: i64,
    pub calls: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsageSummary {
    pub period: UsagePeriod,
    pub from: String,
    pub to: String,
    pub from_ms: i64,
    pub to_ms: i64,
    pub calls: i64,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub by_model: Vec<ModelUsageSummary>,
    pub daily: Vec<DailyUsage>,
    pub db_path: String,
}

pub async fn query_summary(q: &UsageQuery) -> Result<UsageSummary, String> {
    let range = resolve_period(q.period, q.from.as_deref(), q.to.as_deref())?;
    let pool = open_pool().await.map_err(|e| e.to_string())?;

    let totals: (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(prompt_tokens),0), COALESCE(SUM(completion_tokens),0), COALESCE(SUM(total_tokens),0)
         FROM token_usage WHERE created_at >= ? AND created_at <= ?",
    )
    .bind(range.from_ms)
    .bind(range.to_ms)
    .fetch_one(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let by_model: Vec<ModelUsageSummary> = sqlx::query_as(
        "SELECT model, COUNT(*) as calls,
                COALESCE(SUM(prompt_tokens),0), COALESCE(SUM(completion_tokens),0), COALESCE(SUM(total_tokens),0)
         FROM token_usage WHERE created_at >= ? AND created_at <= ?
         GROUP BY model ORDER BY SUM(total_tokens) DESC",
    )
    .bind(range.from_ms)
    .bind(range.to_ms)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(|(model, calls, prompt, completion, total)| ModelUsageSummary {
        model,
        calls,
        prompt_tokens: prompt,
        completion_tokens: completion,
        total_tokens: total,
    })
    .collect();

    let daily: Vec<DailyUsage> = sqlx::query_as(
        "SELECT date(created_at / 1000, 'unixepoch', 'localtime') as d,
                COALESCE(SUM(total_tokens),0), COUNT(*)
         FROM token_usage WHERE created_at >= ? AND created_at <= ?
         GROUP BY d ORDER BY d",
    )
    .bind(range.from_ms)
    .bind(range.to_ms)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(|(date, total_tokens, calls)| DailyUsage {
        date,
        total_tokens,
        calls,
    })
    .collect();

    Ok(UsageSummary {
        period: range.period,
        from: range.from_date,
        to: range.to_date,
        from_ms: range.from_ms,
        to_ms: range.to_ms,
        calls: totals.0,
        prompt_tokens: totals.1,
        completion_tokens: totals.2,
        total_tokens: totals.3,
        by_model,
        daily,
        db_path: usage_db_path().display().to_string(),
    })
}
