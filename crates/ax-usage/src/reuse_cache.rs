//! Per-conversation reuse of read-only graph replies.
//!
//! A repeated call with the same arguments in the same conversation gets a short
//! reference instead of the full answer. Every entry records the files its reply
//! cites with their content hash; any difference makes the entry a miss.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};
use std::sync::OnceLock;

use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::context_cache::{cache_id, load_body, store_body};
use crate::store::open_pool;
use crate::tokenizer::count_tokens;

const REUSE_TOOLS: &[&str] = &[
    "ax_explore",
    "ax_search",
    "ax_node",
    "ax_callers",
    "ax_callees",
    "ax_impact",
    "ax_path",
    "ax_affected",
    "ax_context",
];
const DEFAULT_CAP_BYTES: i64 = 2 * 1024 * 1024;
const MAX_CITED_FILES: usize = 64;
/// Replies this small cost more as a hit reference plus a preflight line than they save.
const MIN_REUSE_TOKENS: i64 = 200;
const ARGS_SUMMARY_CHARS: usize = 120;
const CONTEXT_ROWS: i64 = 60;
pub const SESSION_CONTEXT_TOKENS: i64 = 1_500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReuseHit {
    pub key: String,
    pub id: String,
    pub tool: String,
    pub turn: i64,
    pub original_tokens: i64,
}

/// Indexed `content_hash` per project-relative path, from the project's `files` table.
pub type IndexHashes = BTreeMap<String, String>;

/// A disk-fresh entry; the caller confirms it against the current index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReuseCandidate {
    pub hit: ReuseHit,
    pub indexed: Vec<(String, Option<String>)>,
    pub index_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextEntry {
    pub tool: String,
    pub args_summary: String,
    pub files: Vec<String>,
    pub id: String,
}

pub fn reuse_cacheable(tool: &str) -> bool {
    REUSE_TOOLS.contains(&tool)
}

pub(crate) fn wants_fresh(args: &Value) -> bool {
    args.get("fresh").and_then(Value::as_bool).unwrap_or(false)
}

/// Compact JSON with object keys sorted at every level; top-level `fresh` dropped.
pub fn canonical_args(args: &Value) -> String {
    let mut args = args.clone();
    if let Some(map) = args.as_object_mut() {
        map.remove("fresh");
    }
    let mut out = String::new();
    write_canonical(&args, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&Value::String((*key).clone()).to_string());
                out.push(':');
                write_canonical(&map[*key], out);
            }
            out.push('}');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        scalar => out.push_str(&scalar.to_string()),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn reuse_key(conversation: &str, tool: &str, args: &Value) -> String {
    let canon = canonical_args(args);
    let framed = format!("{}:{conversation}|{}:{tool}|{canon}", conversation.len(), tool.len());
    hex(&Sha256::digest(framed.as_bytes())[..16])
}

fn safe_relative(path: &str) -> bool {
    let p = Path::new(path);
    !path.is_empty() && p.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// Project-relative paths cited as `path:line` or `path::symbol` that exist under `root`.
/// Stops one past `MAX_CITED_FILES`, so a caller can tell the list is incomplete.
pub(crate) fn cited_files(body: &str, root: &Path) -> Vec<String> {
    let mut found = BTreeSet::new();
    for token in body.split_whitespace() {
        let token = token.trim_start_matches(['`', '(', '[', '<', '"', '\'']);
        let Some(colon) = token.find(':') else { continue };
        let path = &token[..colon];
        let next = token[colon + 1..].chars().next();
        if !matches!(next, Some(c) if c.is_ascii_digit() || c == ':') {
            continue;
        }
        if !path.contains('.') || !safe_relative(path) {
            continue;
        }
        if root.join(path).is_file() {
            found.insert(path.to_string());
            if found.len() > MAX_CITED_FILES {
                break;
            }
        }
    }
    found.into_iter().collect()
}

fn hash_file(root: &Path, path: &str) -> Option<String> {
    if !safe_relative(path) {
        return None;
    }
    let bytes = std::fs::read(root.join(path)).ok()?;
    Some(hex(&Sha256::digest(&bytes)[..16]))
}

/// `None` when there is nothing to verify freshness against, or a file cannot be read.
pub fn snapshot_files(root: &Path, files: &[String]) -> Option<Vec<(String, String)>> {
    if files.is_empty() {
        return None;
    }
    files
        .iter()
        .map(|f| Some((f.clone(), hash_file(root, f)?)))
        .collect()
}

type Snapshot = Vec<(String, String, Option<String>)>;

fn parse_snapshot(files_json: &str) -> Option<Snapshot> {
    serde_json::from_str(files_json).ok()
}

/// Fails closed: unparsable, empty, missing, unreadable, or changed means not fresh.
#[cfg(test)]
fn files_fresh(root: &Path, files_json: &str) -> bool {
    parse_snapshot(files_json).is_some_and(|files| disk_unchanged(root, &files))
}

fn disk_unchanged(root: &Path, files: &Snapshot) -> bool {
    !files.is_empty()
        && files
            .iter()
            .all(|(path, hash, _)| hash_file(root, path).as_deref() == Some(hash.as_str()))
}

/// Each cited file has the indexed hash it had when the reply was stored.
pub(crate) fn index_unchanged(recorded: &[(String, Option<String>)], current: &IndexHashes) -> bool {
    !recorded.is_empty()
        && recorded
            .iter()
            .all(|(path, hash)| current.get(path) == hash.as_ref())
}

/// The cited files and the whole index are as they were when the reply was stored.
pub fn index_matches(candidate: &ReuseCandidate, current: &IndexHashes) -> bool {
    index_unchanged(&candidate.indexed, current) && index_fingerprint(current) == candidate.index_fingerprint
}

/// Hash of every indexed path and content hash. A graph answer can depend on files it does
/// not cite (a new caller, a removed callee), so any change to the index invalidates it.
pub(crate) fn index_fingerprint(index: &IndexHashes) -> String {
    let mut hasher = Sha256::new();
    for (path, hash) in index {
        hasher.update(format!("{}:{path}|{}:{hash}|", path.len(), hash.len()).as_bytes());
    }
    hex(&hasher.finalize()[..16])
}

fn indexed_of(files: &Snapshot) -> Vec<(String, Option<String>)> {
    files.iter().map(|(p, _, i)| (p.clone(), i.clone())).collect()
}

pub fn render_hit(hit: &ReuseHit) -> String {
    format!(
        "[ax cache hit] tool={tool} id={id} turn={turn} original_tokens={tokens}\n\
         Same call already answered in this conversation; cited files unchanged. \
         Use the earlier answer, ax_expand id \"{id}\" to see it again, or fresh: true to rerun.\n",
        tool = hit.tool,
        id = hit.id,
        turn = hit.turn,
        tokens = hit.original_tokens,
    )
}

pub fn format_session_context(entries: &[ContextEntry], max_tokens: i64) -> String {
    if entries.is_empty() {
        return String::new();
    }
    const OPEN: &str = "<ax_session_context>\nAlready answered in this conversation (cited files unchanged). \
                        Repeating one returns [ax cache hit]; ax_expand id returns the answer.";
    const CLOSE: &str = "</ax_session_context>";
    let mut lines = vec![OPEN.to_string()];
    for entry in entries {
        lines.push(format!(
            "- {tool} {args} files={files} id={id}",
            tool = entry.tool,
            args = entry.args_summary,
            files = entry.files.join(","),
            id = entry.id,
        ));
        let draft = format!("{}\n{CLOSE}", lines.join("\n"));
        if count_tokens(&draft) as i64 > max_tokens {
            lines.pop();
            break;
        }
    }
    lines.push(CLOSE.to_string());
    lines.join("\n")
}

/// Active chat id, or one id per MCP process when the IDE gives none.
pub fn conversation_key(active: Option<String>) -> String {
    if let Some(id) = active.filter(|s| !s.is_empty()) {
        return id;
    }
    static PROCESS: OnceLock<String> = OnceLock::new();
    PROCESS
        .get_or_init(|| {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            format!("proc-{}-{nanos}", std::process::id())
        })
        .clone()
}

pub fn reuse_cap_bytes() -> i64 {
    std::env::var("AX_REUSE_CACHE_BYTES")
        .ok()
        .and_then(|raw| raw.trim().parse::<i64>().ok())
        .unwrap_or(DEFAULT_CAP_BYTES)
}

/// Entries belong to one conversation in one project.
fn scope(conversation: &str, root: &Path) -> String {
    format!("{conversation}\u{1f}{}", root.display())
}

fn args_summary(args: &Value) -> String {
    canonical_args(args).chars().take(ARGS_SUMMARY_CHARS).collect()
}

async fn conversation_turn(pool: &SqlitePool, conversation: &str) -> Result<i64, String> {
    let (turn,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM mcp_session_index WHERE session_id = ? AND tool = 'ax_preflight'",
    )
    .bind(conversation)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(turn)
}

/// One successful tool reply as the cache sees it.
pub(crate) struct Reply<'a> {
    pub tool: &'a str,
    pub args: &'a Value,
    pub body: &'a str,
}

/// Store a successful reply. Returns its cache id, or `None` when it is not cacheable.
pub(crate) async fn store_reply(
    pool: &SqlitePool,
    root: &Path,
    conversation: &str,
    reply: Reply<'_>,
    index: &IndexHashes,
    cap_bytes: i64,
) -> Result<Option<String>, String> {
    let Reply { tool, args, body } = reply;
    if !reuse_enabled() || !reuse_cacheable(tool) || body.len() as i64 > cap_bytes {
        return Ok(None);
    }
    let cited = cited_files(body, root);
    if cited.len() > MAX_CITED_FILES {
        return Ok(None);
    }
    let Some(snapshot) = snapshot_files(root, &cited) else {
        return Ok(None);
    };
    let tokens = count_tokens(body) as i64;
    if tokens <= MIN_REUSE_TOKENS {
        return Ok(None);
    }
    let id = cache_id(body);
    store_body(pool, &id, tool, body, tokens).await?;
    let turn = conversation_turn(pool, conversation).await?;
    let snapshot: Snapshot = snapshot
        .into_iter()
        .map(|(path, disk)| {
            let indexed = index.get(&path).cloned();
            (path, disk, indexed)
        })
        .collect();
    let files_json = serde_json::to_string(&snapshot).map_err(|e| e.to_string())?;
    let conversation = scope(conversation, root);
    let conversation = conversation.as_str();
    sqlx::query(
        "INSERT OR REPLACE INTO mcp_reuse_cache
         (reuse_key, conversation, tool, args_summary, cache_id, body_bytes, original_tokens,
          files_json, index_fingerprint, turn, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(reuse_key(conversation, tool, args))
    .bind(conversation)
    .bind(tool)
    .bind(args_summary(args))
    .bind(&id)
    .bind(body.len() as i64)
    .bind(tokens)
    .bind(files_json)
    .bind(index_fingerprint(index))
    .bind(turn)
    .bind(chrono::Utc::now().timestamp())
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query(
        "DELETE FROM mcp_reuse_cache WHERE conversation = ?1 AND rowid IN (
           SELECT rowid FROM (
             SELECT rowid, SUM(body_bytes) OVER (ORDER BY rowid DESC) AS running
             FROM mcp_reuse_cache WHERE conversation = ?1
           ) WHERE running > ?2
         )",
    )
    .bind(conversation)
    .bind(cap_bytes)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(Some(id))
}

/// Any error, stale file, or missing body is a miss.
pub async fn lookup(
    pool: &SqlitePool,
    root: &Path,
    conversation: &str,
    tool: &str,
    args: &Value,
) -> Option<ReuseCandidate> {
    if !reuse_enabled() || !reuse_cacheable(tool) || wants_fresh(args) {
        return None;
    }
    let conversation = scope(conversation, root);
    let conversation = conversation.as_str();
    let key = reuse_key(conversation, tool, args);
    let (id, files_json, fingerprint, turn, original_tokens): (String, String, String, i64, i64) = sqlx::query_as(
        "SELECT cache_id, files_json, index_fingerprint, turn, original_tokens FROM mcp_reuse_cache
         WHERE reuse_key = ? AND conversation = ? AND tool = ?",
    )
    .bind(&key)
    .bind(conversation)
    .bind(tool)
    .fetch_optional(pool)
    .await
    .ok()??;
    let files = parse_snapshot(&files_json)?;
    if !disk_unchanged(root, &files) || load_body(pool, &id).await.is_err() {
        return None;
    }
    Some(ReuseCandidate {
        hit: ReuseHit {
            key,
            id,
            tool: tool.to_string(),
            turn,
            original_tokens,
        },
        indexed: indexed_of(&files),
        index_fingerprint: fingerprint,
    })
}

pub async fn record_hit(pool: &SqlitePool, key: &str, tokens_avoided: i64) -> Result<(), String> {
    sqlx::query(
        "UPDATE mcp_reuse_cache SET hits = hits + 1, tokens_avoided = tokens_avoided + ?
         WHERE reuse_key = ?",
    )
    .bind(tokens_avoided)
    .bind(key)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
pub(crate) async fn session_context(
    pool: &SqlitePool,
    root: &Path,
    conversation: &str,
    max_tokens: i64,
    index: &IndexHashes,
) -> Option<String> {
    let entries = session_entries(pool, root, conversation, index).await?;
    let text = format_session_context(&entries, max_tokens);
    (!text.is_empty()).then_some(text)
}

/// Fresh entries of this conversation, newest first.
pub(crate) async fn session_entries(
    pool: &SqlitePool,
    root: &Path,
    conversation: &str,
    index: &IndexHashes,
) -> Option<Vec<ContextEntry>> {
    let current = index_fingerprint(index);
    let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT tool, args_summary, files_json, index_fingerprint, cache_id FROM mcp_reuse_cache
         WHERE conversation = ? ORDER BY rowid DESC LIMIT ?",
    )
    .bind(scope(conversation, root))
    .bind(CONTEXT_ROWS)
    .fetch_all(pool)
    .await
    .ok()?;
    let entries: Vec<ContextEntry> = rows
        .into_iter()
        .filter_map(|(tool, args_summary, files_json, fingerprint, id)| {
            let files = parse_snapshot(&files_json)?;
            let fresh = fingerprint == current
                && disk_unchanged(root, &files)
                && index_unchanged(&indexed_of(&files), index);
            fresh.then(|| ContextEntry {
                tool,
                args_summary,
                files: files.into_iter().map(|(p, _, _)| p).collect(),
                id,
            })
        })
        .collect();
    Some(entries)
}

pub async fn reuse_lookup(
    root: &Path,
    conversation: &str,
    tool: &str,
    args: &Value,
) -> Option<ReuseCandidate> {
    let pool = open_pool().await.ok()?;
    lookup(&pool, root, conversation, tool, args).await
}

pub async fn reuse_store(
    root: &Path,
    conversation: &str,
    tool: &str,
    args: &Value,
    body: &str,
    index: &IndexHashes,
) -> Result<Option<String>, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    store_reply(&pool, root, conversation, Reply { tool, args, body }, index, reuse_cap_bytes()).await
}

pub async fn reuse_record_hit(key: &str, tokens_avoided: i64) -> Result<(), String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    record_hit(&pool, key, tokens_avoided).await
}

pub async fn reuse_session_entries(root: &Path, conversation: &str, index: &IndexHashes) -> Vec<ContextEntry> {
    let Ok(pool) = open_pool().await else {
        return Vec::new();
    };
    session_entries(&pool, root, conversation, index).await.unwrap_or_default()
}

#[cfg(test)]
mod tests;

/// `AX_CONTEXT_CACHE=off` (or `0`) turns conversation reuse off along with oversized-reply stubs.
pub fn reuse_enabled() -> bool {
    reuse_enabled_from(std::env::var("AX_CONTEXT_CACHE").ok().as_deref())
}

pub(crate) fn reuse_enabled_from(raw: Option<&str>) -> bool {
    !raw.is_some_and(|v| v.eq_ignore_ascii_case("off") || v == "0")
}
