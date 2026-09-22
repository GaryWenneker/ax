//! Ephemeral store for oversized MCP replies. Not the memory vault.

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::store::open_pool;
use crate::tokenizer::count_tokens;

const DEFAULT_THRESHOLD: i64 = 3_000;
const TTL_SECS: i64 = 7 * 24 * 60 * 60;
pub const EXPAND_DEFAULT_CHARS: usize = 8_000;
pub const EXPAND_MAX_CHARS: usize = 12_000;

const NEVER_STUB: &[&str] = &[
    "ax_preflight",
    "ax_guard",
    "ax_policy_capture",
    "ax_expand",
    "ax_stash",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheOutcome {
    Passthrough,
    Stubbed {
        text: String,
        id: String,
        original_tokens: i64,
        sent_tokens: i64,
        removed_tokens: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandPage {
    pub text: String,
    pub next_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashReceipt {
    pub id: String,
    pub original_tokens: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub id: String,
    pub tool: String,
    pub summary: String,
    pub original_tokens: i64,
}

pub fn cache_enabled() -> bool {
    if std::env::var("AX_CONTEXT_CACHE")
        .ok()
        .is_some_and(|v| v.eq_ignore_ascii_case("off") || v == "0")
    {
        return false;
    }
    cache_threshold() > 0
}

pub fn cache_threshold() -> i64 {
    match std::env::var("AX_CONTEXT_CACHE_TOKENS") {
        Ok(raw) => raw.trim().parse::<i64>().unwrap_or(DEFAULT_THRESHOLD),
        Err(_) => DEFAULT_THRESHOLD,
    }
}

pub fn exempt_from_cache(tool: &str) -> bool {
    NEVER_STUB.contains(&tool)
}

pub fn cache_id(body: &str) -> String {
    let dig = Sha256::digest(body.as_bytes());
    let mut hex_id = String::with_capacity(16);
    for byte in dig.iter().take(8) {
        hex_id.push_str(&format!("{byte:02x}"));
    }
    format!("cc_{hex_id}")
}

pub fn one_line_summary(body: &str) -> String {
    let line = body.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let collapsed: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out = String::new();
    for ch in collapsed.chars() {
        if out.chars().count() >= 160 {
            break;
        }
        out.push(ch);
    }
    out
}

fn render_stub(tool: &str, id: &str, original: i64, summary: &str) -> (String, i64, i64) {
    let mut sent = 1_i64;
    let mut text = String::new();
    for _ in 0..4 {
        text = format!(
            "[ax context cache] tool={tool} id={id}\noriginal_tokens={original} sent_tokens={sent} removed_tokens={removed}\nsummary: {summary}\nCall ax_expand with id \"{id}\". Optional offset and limit (characters) page the original.\n",
            removed = original.saturating_sub(sent),
        );
        let measured = count_tokens(&text) as i64;
        if measured == sent {
            return (text, sent, original.saturating_sub(sent));
        }
        sent = measured;
    }
    let removed = original.saturating_sub(sent);
    (text, sent, removed)
}

pub async fn cache_oversized_reply(tool: &str, body: &str) -> CacheOutcome {
    if !cache_enabled() || exempt_from_cache(tool) {
        return CacheOutcome::Passthrough;
    }
    let original = count_tokens(body) as i64;
    if original < cache_threshold() {
        return CacheOutcome::Passthrough;
    }
    let id = cache_id(body);
    let summary = one_line_summary(body);
    let (stub, sent, removed) = render_stub(tool, &id, original, &summary);
    if sent >= original {
        return CacheOutcome::Passthrough;
    }
    match open_pool().await {
        Ok(pool) => {
            if store_body(&pool, &id, tool, body, original).await.is_err() {
                return CacheOutcome::Passthrough;
            }
        }
        Err(_) => return CacheOutcome::Passthrough,
    }
    CacheOutcome::Stubbed {
        text: stub,
        id,
        original_tokens: original,
        sent_tokens: sent,
        removed_tokens: removed,
    }
}

pub async fn stash_text(label: Option<&str>, text: &str) -> Result<StashReceipt, String> {
    if !cache_enabled() {
        return Err("context cache is off".to_string());
    }
    if text.trim().is_empty() {
        return Err("text required".to_string());
    }
    let tool = label
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("stash");
    let tool: String = tool.chars().filter(|c| !c.is_control()).take(64).collect();
    let id = cache_id(text);
    let original_tokens = count_tokens(text) as i64;
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    store_body(&pool, &id, &tool, text, original_tokens).await?;
    Ok(StashReceipt { id, original_tokens })
}

pub async fn recent_catalog(limit: usize) -> Result<Vec<CatalogEntry>, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    list_recent(&pool, limit).await
}

pub fn format_catalog(entries: &[CatalogEntry], max_tokens: i64) -> String {
    let mut lines = vec!["<ax_context_catalog>".to_string()];
    for entry in entries {
        lines.push(format!(
            "- {id} tool={tool} tokens={tokens} summary={summary}",
            id = entry.id,
            tool = entry.tool,
            tokens = entry.original_tokens,
            summary = entry.summary,
        ));
        let draft = lines.join("\n") + "\n</ax_context_catalog>";
        if count_tokens(&draft) as i64 > max_tokens {
            lines.pop();
            break;
        }
    }
    lines.push("</ax_context_catalog>".to_string());
    lines.join("\n")
}

/// One line for the current session: row count, tokens kept out of the prompt,
/// and tokens that stayed inline. No bodies.
pub fn format_session_ledger(rows: i64, stored_tokens: i64, inline_tokens: i64) -> String {
    format!(
        "<ax_session_ledger>rows={rows} stored_tokens={stored_tokens} inline_tokens={inline_tokens}</ax_session_ledger>"
    )
}

pub async fn session_ledger(session_id: Option<&str>) -> Result<Option<String>, String> {
    let Some(session_id) = session_id.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if !cache_enabled() {
        return Ok(None);
    }
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    let row: Option<(i64, i64, i64)> = sqlx::query_as(
        "SELECT COUNT(*),
                COALESCE(SUM(CASE WHEN cache_id IS NOT NULL THEN original_tokens ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN cache_id IS NULL THEN original_tokens ELSE 0 END), 0)
         FROM mcp_session_index
         WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| e.to_string())?;
    let Some((rows, stored, inline)) = row else {
        return Ok(None);
    };
    if rows == 0 {
        return Ok(None);
    }
    Ok(Some(format_session_ledger(rows, stored, inline)))
}

pub async fn expand_cached(id: &str, offset: usize, limit: Option<usize>) -> Result<ExpandPage, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    let body = load_body(&pool, id).await?;
    Ok(page_body(&body, offset, limit))
}

pub fn page_body(body: &str, offset: usize, limit: Option<usize>) -> ExpandPage {
    let chars: Vec<char> = body.chars().collect();
    if offset > chars.len() {
        return ExpandPage {
            text: format!("offset {offset} is past the end ({})", chars.len()),
            next_offset: None,
        };
    }
    let limit = limit.unwrap_or(EXPAND_DEFAULT_CHARS).clamp(1, EXPAND_MAX_CHARS);
    let end = (offset + limit).min(chars.len());
    let text: String = chars[offset..end].iter().collect();
    let next_offset = if end < chars.len() { Some(end) } else { None };
    let mut out = text;
    if let Some(next) = next_offset {
        out.push_str(&format!(
            "\n\n[ax context cache] more remains; ax_expand id offset={next}"
        ));
    }
    ExpandPage {
        text: out,
        next_offset,
    }
}

async fn store_body(
    pool: &SqlitePool,
    id: &str,
    tool: &str,
    body: &str,
    original_tokens: i64,
) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp();
    let expires = now + TTL_SECS;
    sqlx::query("DELETE FROM mcp_context_cache WHERE expires_at < ?")
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT INTO mcp_context_cache (id, tool, body, original_tokens, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           tool = excluded.tool,
           body = excluded.body,
           original_tokens = excluded.original_tokens,
           created_at = excluded.created_at,
           expires_at = excluded.expires_at",
    )
    .bind(id)
    .bind(tool)
    .bind(body)
    .bind(original_tokens)
    .bind(now)
    .bind(expires)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

async fn list_recent(pool: &SqlitePool, limit: usize) -> Result<Vec<CatalogEntry>, String> {
    let now = chrono::Utc::now().timestamp();
    let limit = limit.clamp(1, 50) as i64;
    let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT id, tool, body, original_tokens FROM mcp_context_cache
         WHERE expires_at >= ? ORDER BY created_at DESC LIMIT ?",
    )
    .bind(now)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(id, tool, body, original_tokens)| CatalogEntry {
            summary: one_line_summary(&body),
            id,
            tool,
            original_tokens,
        })
        .collect())
}

/// Record one session event. Stores the body only when it is over the cache
/// threshold and no cache id was already assigned.
pub async fn note_session_event(
    session_id: Option<&str>,
    tool: &str,
    body: &str,
    existing_cache_id: Option<&str>,
) -> Result<(), String> {
    if !cache_enabled() || body.trim().is_empty() {
        return Ok(());
    }
    let original_tokens = count_tokens(body) as i64;
    let summary = one_line_summary(body);
    let mut cache_id = existing_cache_id.map(|s| s.to_string());
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    if cache_id.is_none() && original_tokens >= cache_threshold() {
        let id = cache_id_of(body);
        store_body(&pool, &id, tool, body, original_tokens).await?;
        cache_id = Some(id);
    }
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        "INSERT INTO mcp_session_index
         (session_id, tool, summary, original_tokens, cache_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(session_id.filter(|s| !s.is_empty()))
    .bind(tool)
    .bind(&summary)
    .bind(original_tokens)
    .bind(&cache_id)
    .bind(now)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn spawn_note_session_event(
    session_id: Option<String>,
    tool: String,
    body: String,
    existing_cache_id: Option<String>,
) {
    tokio::spawn(async move {
        let _ = note_session_event(
            session_id.as_deref(),
            &tool,
            &body,
            existing_cache_id.as_deref(),
        )
        .await;
    });
}

fn cache_id_of(body: &str) -> String {
    cache_id(body)
}

pub async fn recent_session_catalog(
    session_id: Option<&str>,
    limit: usize,
) -> Result<Vec<CatalogEntry>, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    let limit = limit.clamp(1, 50) as i64;
    let rows: Vec<(Option<String>, String, String, i64, Option<String>)> = sqlx::query_as(
        "SELECT cache_id, tool, summary, original_tokens, session_id
         FROM mcp_session_index
         ORDER BY CASE WHEN session_id IS NOT NULL AND session_id = ?1 THEN 0 ELSE 1 END,
                  created_at DESC
         LIMIT ?2",
    )
    .bind(session_id.unwrap_or(""))
    .bind(limit)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(cache_id, tool, summary, original_tokens, _session)| CatalogEntry {
            id: cache_id.unwrap_or_else(|| "inline".to_string()),
            tool,
            summary,
            original_tokens,
        })
        .collect())
}

/// Pull tool-result strings out of a Cursor or Claude JSONL transcript.
pub fn tool_chunks_from_jsonl(jsonl: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in jsonl.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        collect_tool_chunks(&value, "tool_result", &mut out);
    }
    out
}

fn collect_tool_chunks(value: &serde_json::Value, tool: &str, out: &mut Vec<(String, String)>) {
    match value {
        serde_json::Value::Object(map) => {
            let next_tool = map
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| map.get("tool_name").and_then(|v| v.as_str()))
                .unwrap_or(tool);
            let is_result = map
                .get("type")
                .and_then(|v| v.as_str())
                .is_some_and(|t| t == "tool_result" || t == "tool_use");
            if is_result {
                if let Some(text) = map.get("content").and_then(value_text) {
                    if text.len() > 200 {
                        out.push((next_tool.to_string(), text));
                    }
                }
                for (key, child) in map {
                    if key != "content" {
                        collect_tool_chunks(child, next_tool, out);
                    }
                }
                return;
            }
            for (key, child) in map {
                if key == "content" || key == "message" || key == "messages" {
                    collect_tool_chunks(child, next_tool, out);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_tool_chunks(item, tool, out);
            }
        }
        _ => {}
    }
}

fn value_text(value: &serde_json::Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    let arr = value.as_array()?;
    let mut buf = String::new();
    for item in arr {
        if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
            buf.push_str(text);
            buf.push('\n');
        } else if let Some(text) = item.as_str() {
            buf.push_str(text);
            buf.push('\n');
        }
    }
    if buf.is_empty() {
        None
    } else {
        Some(buf)
    }
}

/// Stash tool-result chunks that cross the token threshold. Returns how many were stored.
pub async fn ingest_jsonl_oversized(session_id: Option<&str>, jsonl: &str) -> Result<usize, String> {
    if !cache_enabled() {
        return Ok(0);
    }
    let mut stored = 0;
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    for (tool, text) in tool_chunks_from_jsonl(jsonl) {
        let tokens = count_tokens(&text) as i64;
        if tokens < cache_threshold() {
            continue;
        }
        let id = cache_id(&text);
        if cache_id_recorded(&pool, &id).await? {
            continue;
        }
        note_session_event(session_id, &tool, &text, None).await?;
        stored += 1;
    }
    Ok(stored)
}

async fn cache_id_recorded(pool: &SqlitePool, id: &str) -> Result<bool, String> {
    let hit: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM mcp_context_cache WHERE id = ?
         UNION
         SELECT 1 FROM mcp_session_index WHERE cache_id = ?
         LIMIT 1",
    )
    .bind(id)
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(hit.is_some())
}

async fn load_body(pool: &SqlitePool, id: &str) -> Result<String, String> {
    let now = chrono::Utc::now().timestamp();
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT body FROM mcp_context_cache WHERE id = ? AND expires_at >= ?",
    )
    .bind(id)
    .bind(now)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    row.map(|r| r.0)
        .ok_or_else(|| format!("No cached MCP reply for id {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn big_body() -> String {
        "line one of the cached reply\n".repeat(4_000)
    }

    #[test]
    fn tool_chunks_reads_tool_result_content() {
        let body = "y".repeat(250);
        let line = format!(
            r#"{{"message":{{"content":[{{"type":"tool_result","name":"Read","content":"{body}"}}]}}}}"#
        );
        let chunks = tool_chunks_from_jsonl(&line);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, "Read");
        assert!(chunks[0].1.starts_with('y'));
    }

    #[test]
    fn exempt_tools_are_the_turn_contract() {
        assert!(exempt_from_cache("ax_preflight"));
        assert!(exempt_from_cache("ax_guard"));
        assert!(exempt_from_cache("ax_policy_capture"));
        assert!(exempt_from_cache("ax_expand"));
        assert!(!exempt_from_cache("ax_explore"));
    }

    #[test]
    fn summary_is_first_line_capped_at_160() {
        let body = format!("  {}\nsecond", "x".repeat(200));
        let summary = one_line_summary(&body);
        assert_eq!(summary.chars().count(), 160);
        assert!(summary.chars().all(|c| c == 'x'));
    }

    #[test]
    fn page_respects_offset_and_clamps_limit() {
        let body: String = (0..100).map(|i| char::from(b'a' + (i % 26))).collect();
        let page = page_body(&body, 10, Some(5));
        assert!(page.text.starts_with("klmno"));
        assert_eq!(page.next_offset, Some(15));
        let wide = page_body(&body, 0, Some(100_000));
        assert!(wide.text.chars().count() <= EXPAND_MAX_CHARS + 80);
        assert!(page_body(&body, 500, None).text.contains("past the end"));
    }

    #[test]
    fn stub_is_smaller_and_names_expand() {
        let body = big_body();
        let original = count_tokens(&body) as i64;
        let (text, sent, removed) = render_stub("ax_explore", "cc_abc", original, "line one");
        assert!(text.contains("ax_expand"));
        assert!(text.contains("cc_abc"));
        assert!(sent < original);
        assert_eq!(removed, original - sent);
        assert_eq!(count_tokens(&text) as i64, sent);
    }

    #[tokio::test]
    async fn store_roundtrip_and_expiry() {
        let path: PathBuf = std::env::temp_dir().join(format!(
            "ax-context-cache-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rwc", path.display()))
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE mcp_context_cache (
              id TEXT PRIMARY KEY,
              tool TEXT NOT NULL,
              body TEXT NOT NULL,
              original_tokens INTEGER NOT NULL,
              created_at INTEGER NOT NULL,
              expires_at INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        store_body(&pool, "cc_test", "ax_explore", "hello cache", 3)
            .await
            .unwrap();
        let loaded = load_body(&pool, "cc_test").await.unwrap();
        assert_eq!(loaded, "hello cache");
        sqlx::query("UPDATE mcp_context_cache SET expires_at = 1 WHERE id = 'cc_test'")
            .execute(&pool)
            .await
            .unwrap();
        let err = load_body(&pool, "cc_test").await.unwrap_err();
        assert!(err.contains("No cached MCP reply"));
        let listed = list_recent(&pool, 20).await.unwrap();
        assert!(listed.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn catalog_omits_bodies_and_respects_token_cap() {
        let entries = vec![
            CatalogEntry {
                id: "cc_a".into(),
                tool: "stash".into(),
                summary: "short".into(),
                original_tokens: 10,
            },
            CatalogEntry {
                id: "cc_b".into(),
                tool: "ax_explore".into(),
                summary: "x".repeat(400),
                original_tokens: 9_000,
            },
        ];
        let text = format_catalog(&entries, 80);
        assert!(text.contains("cc_a"));
        assert!(!text.contains(&"x".repeat(400)));
        assert!(text.contains("<ax_context_catalog>"));
    }

    #[test]
    fn ledger_line_has_counts_and_no_body() {
        let line = format_session_ledger(3, 9000, 40);
        assert!(line.contains("rows=3"));
        assert!(line.contains("stored_tokens=9000"));
        assert!(line.contains("inline_tokens=40"));
        assert!(line.starts_with("<ax_session_ledger>"));
    }
}
