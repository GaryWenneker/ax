//! Cursor `state.vscdb` import — Composer model + context-meter input tokens.

use std::path::{Path, PathBuf};

use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::ConnectOptions;

use crate::store::open_pool;

const COMPOSER_KEY_PREFIX: &str = "composerData:";

/// Build a stable model label from Cursor `model_id` + boolean `model_params`.
pub fn normalize_cursor_model(model_id: &str, model_params: &[(String, String)]) -> String {
    let base = model_id.trim();
    if base.is_empty() {
        return String::new();
    }
    let mut out = base.to_string();
    let mut suffixes: Vec<String> = model_params
        .iter()
        .filter_map(|(id, value)| {
            let id = id.trim();
            if id.is_empty() {
                return None;
            }
            let truthy = matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "true" | "1" | "yes"
            );
            truthy.then(|| id.to_ascii_lowercase())
        })
        .collect();
    suffixes.sort();
    suffixes.dedup();
    for suffix in suffixes {
        let needle = format!("-{suffix}");
        if out.ends_with(&needle) || out.contains(&format!("{needle}-")) {
            continue;
        }
        out.push('-');
        out.push_str(&suffix);
    }
    out
}

/// Parsed Composer session row from `state.vscdb`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerStateRow {
    pub session_id: String,
    pub model: Option<String>,
    pub input_tokens: Option<i64>,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
}

/// Default Cursor global state database (read-only import source).
pub fn cursor_state_vscdb_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("AX_CURSOR_STATE_VSCDB") {
        let path = path.trim();
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }

    #[cfg(windows)]
    {
        return dirs::data_dir().map(|d| {
            d.join("Cursor")
                .join("User")
                .join("globalStorage")
                .join("state.vscdb")
        });
    }

    #[cfg(target_os = "macos")]
    {
        return dirs::data_dir().map(|d| {
            d.join("Cursor")
                .join("User")
                .join("globalStorage")
                .join("state.vscdb")
        });
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return dirs::config_dir().map(|d| {
            d.join("Cursor")
                .join("User")
                .join("globalStorage")
                .join("state.vscdb")
        });
    }

    #[cfg(not(any(windows, unix)))]
    {
        None
    }
}

/// Path to the active Cursor session marker written by `ax session-hook`.
pub fn active_cursor_session_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".ax").join("active-cursor-session"))
}

/// Persist the latest Cursor `session_id` so MCP verbose lines can tag `session=`.
pub fn write_active_cursor_session(session_id: &str) -> Result<(), String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err("session_id required".into());
    }
    let path = active_cursor_session_path().ok_or("home directory not found")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, format!("{session_id}\n")).map_err(|e| e.to_string())
}

/// Read the active Cursor session id from `~/.ax/active-cursor-session`, if present.
pub fn read_active_cursor_session() -> Option<String> {
    let path = active_cursor_session_path()?;
    let text = std::fs::read_to_string(path).ok()?;
    let id = text.lines().next()?.trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

fn json_i64(v: &Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_u64().and_then(|u| i64::try_from(u).ok()))
        .or_else(|| v.as_f64().map(|f| f as i64))
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn parse_iso_ms(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

fn timestamp_ms(v: &Value) -> Option<i64> {
    json_i64(v).filter(|n| *n > 0).or_else(|| {
        v.as_str()
            .and_then(|s| parse_iso_ms(s))
    })
}

/// Extract model label from `composerData.modelConfig`.
pub fn parse_composer_model_config(value: &Value) -> Option<String> {
    let cfg = value.get("modelConfig")?;
    let base = cfg
        .get("modelName")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    let mut params: Vec<(String, String)> = Vec::new();
    if cfg.get("maxMode").and_then(|v| v.as_bool()) == Some(true) {
        params.push(("max".to_string(), "true".to_string()));
    }
    if let Some(models) = cfg.get("selectedModels").and_then(|v| v.as_array()) {
        for entry in models {
            if let Some(arr) = entry.get("parameters").and_then(|v| v.as_array()) {
                for param in arr {
                    let id = param.get("id").and_then(|v| v.as_str())?.trim();
                    if id.is_empty() {
                        continue;
                    }
                    let val = param
                        .get("value")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    params.push((id.to_string(), val));
                }
            }
        }
    }
    let model = normalize_cursor_model(base, &params);
    if model.is_empty() {
        None
    } else {
        Some(model)
    }
}

/// Context-meter input tokens from `promptTokenBreakdown` / `contextTokensUsed`.
pub fn parse_composer_input_tokens(value: &Value) -> Option<i64> {
    value
        .get("promptTokenBreakdown")
        .and_then(|b| b.get("totalUsedTokens"))
        .and_then(json_i64)
        .filter(|n| *n > 0)
        .or_else(|| {
            value
                .get("contextTokensUsed")
                .and_then(json_i64)
                .filter(|n| *n > 0)
        })
}

fn conversation_timestamps(value: &Value) -> (Option<i64>, Option<i64>) {
    let mut started: Option<i64> = None;
    let mut ended: Option<i64> = None;

    for key in ["createdAt", "lastUpdatedAt", "updatedAt"] {
        if let Some(ts) = value.get(key).and_then(timestamp_ms) {
            started = Some(started.map_or(ts, |s| s.min(ts)));
            ended = Some(ended.map_or(ts, |e| e.max(ts)));
        }
    }

    if let Some(headers) = value
        .get("fullConversationHeadersOnly")
        .and_then(|v| v.as_array())
    {
        for header in headers {
            if let Some(ts) = header.get("createdAt").and_then(timestamp_ms) {
                started = Some(started.map_or(ts, |s| s.min(ts)));
                ended = Some(ended.map_or(ts, |e| e.max(ts)));
            }
        }
    }

    (started, ended)
}

/// Parse one `composerData` JSON blob (session id without prefix).
pub fn parse_composer_data(session_id: &str, value: &Value) -> Option<ComposerStateRow> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return None;
    }

    let model = parse_composer_model_config(value);
    let input_tokens = parse_composer_input_tokens(value);
    let (started_at, ended_at) = conversation_timestamps(value);

    if model.is_none() && input_tokens.is_none() && started_at.is_none() {
        return None;
    }

    Some(ComposerStateRow {
        session_id: session_id.to_string(),
        model,
        input_tokens,
        started_at,
        ended_at,
    })
}

async fn open_cursor_vscdb_readonly(path: &Path) -> Result<SqlitePool, String> {
    if !path.is_file() {
        return Err(format!("Cursor state database not found: {}", path.display()));
    }

    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .disable_statement_logging();

    match SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
    {
        Ok(pool) => Ok(pool),
        Err(e) => Err(format!(
            "could not open Cursor state.vscdb (is Cursor running?): {e}"
        )),
    }
}

/// Load all `composerData:*` rows from Cursor `state.vscdb`.
pub async fn load_composer_state_rows(
    path: &Path,
) -> Result<Vec<ComposerStateRow>, String> {
    let pool = open_cursor_vscdb_readonly(path).await?;
    let keys: Vec<(String, String)> = sqlx::query_as(
        "SELECT key, value FROM cursorDiskKV WHERE key LIKE 'composerData:%'",
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| format!("cursor state query: {e}"))?;

    let mut rows = Vec::with_capacity(keys.len());
    for (key, raw) in keys {
        let Some(session_id) = key.strip_prefix(COMPOSER_KEY_PREFIX) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        if let Some(row) = parse_composer_data(session_id, &value) {
            rows.push(row);
        }
    }
    Ok(rows)
}

/// Merge Composer state into `agent_session_log` without clobbering tool-call counts.
pub async fn upsert_composer_state_row(row: &ComposerStateRow) -> Result<bool, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    let input = row.input_tokens.filter(|n| *n > 0);
    let result = sqlx::query(
        "INSERT INTO agent_session_log
         (agent, session_id, read_calls, grep_calls, ax_calls, session_input_tokens,
          model, source_mtime, started_at, ended_at)
         VALUES ('cursor', ?, 0, 0, 0, ?, ?, 0, ?, ?)
         ON CONFLICT(agent, session_id) DO UPDATE SET
           model = CASE
             WHEN excluded.model IS NOT NULL AND excluded.model != '' THEN
               CASE
                 WHEN agent_session_log.model IS NULL OR agent_session_log.model = '' THEN excluded.model
                 WHEN length(excluded.model) > length(agent_session_log.model) THEN excluded.model
                 ELSE agent_session_log.model
               END
             ELSE agent_session_log.model
           END,
           session_input_tokens = CASE
             WHEN excluded.session_input_tokens IS NOT NULL AND (
               agent_session_log.session_input_tokens IS NULL
               OR excluded.session_input_tokens > agent_session_log.session_input_tokens
             ) THEN excluded.session_input_tokens
             ELSE agent_session_log.session_input_tokens
           END,
           started_at = CASE
             WHEN agent_session_log.started_at IS NULL THEN excluded.started_at
             WHEN excluded.started_at IS NULL THEN agent_session_log.started_at
             ELSE MIN(agent_session_log.started_at, excluded.started_at)
           END,
           ended_at = CASE
             WHEN agent_session_log.ended_at IS NULL THEN excluded.ended_at
             WHEN excluded.ended_at IS NULL THEN agent_session_log.ended_at
             ELSE MAX(agent_session_log.ended_at, excluded.ended_at)
           END",
    )
    .bind(&row.session_id)
    .bind(input)
    .bind(&row.model)
    .bind(row.started_at)
    .bind(row.ended_at)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(result.rows_affected() > 0)
}

/// Import Composer metadata from Cursor `state.vscdb`.
pub async fn import_cursor_composer_state() -> Result<(usize, usize), String> {
    let Some(path) = cursor_state_vscdb_path() else {
        return Ok((0, 0));
    };
    if !path.is_file() {
        return Ok((0, 0));
    }

    let rows = match load_composer_state_rows(&path).await {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!("ax savings import: cursor state.vscdb: {e}");
            return Ok((0, 0));
        }
    };

    let mut enriched = 0usize;
    let mut skipped = 0usize;
    for row in &rows {
        match upsert_composer_state_row(row).await {
            Ok(true) => enriched += 1,
            Ok(false) => skipped += 1,
            Err(e) => eprintln!(
                "ax savings import: cursor state session {}: {e}",
                row.session_id
            ),
        }
    }
    Ok((enriched, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalize_cursor_model_skips_duplicate_suffix() {
        let params = vec![("fast".to_string(), "true".to_string())];
        assert_eq!(
            normalize_cursor_model("composer-2.5-fast", &params),
            "composer-2.5-fast"
        );
    }

    #[test]
    fn parse_composer_model_with_fast_param() {
        let value = json!({
            "modelConfig": {
                "modelName": "composer-2.5",
                "maxMode": false,
                "selectedModels": [{
                    "modelId": "composer-2.5",
                    "parameters": [{ "id": "fast", "value": "true" }]
                }]
            }
        });
        assert_eq!(
            parse_composer_model_config(&value).as_deref(),
            Some("composer-2.5-fast")
        );
    }

    #[test]
    fn parse_composer_input_tokens_from_breakdown() {
        let value = json!({
            "contextTokensUsed": 120000,
            "promptTokenBreakdown": { "totalUsedTokens": 126877 }
        });
        assert_eq!(parse_composer_input_tokens(&value), Some(126877));
    }

    #[test]
    fn parse_composer_data_full_row() {
        let value = json!({
            "modelConfig": { "modelName": "composer-2.5", "selectedModels": [] },
            "promptTokenBreakdown": { "totalUsedTokens": 5000 },
            "fullConversationHeadersOnly": [
                { "createdAt": "2026-07-21T11:37:35.276Z" },
                { "createdAt": "2026-07-21T13:55:47.000Z" }
            ]
        });
        let row = parse_composer_data("218bb987-86eb-45f0-a8e7-eedae17f995c", &value).unwrap();
        assert_eq!(row.input_tokens, Some(5000));
        assert_eq!(row.model.as_deref(), Some("composer-2.5"));
        assert!(row.started_at.is_some());
        assert!(row.ended_at.is_some());
    }
}
