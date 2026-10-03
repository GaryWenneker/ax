//! Small per-conversation working snapshot: facts, files, decisions, open questions.
//!
//! Raw tool bodies stay in the reply caches. This object is what a later turn is shown
//! so it does not rebuild that knowledge from the graph. The agent writes it; ax only
//! stores it, caps it, and marks it stale when the index fingerprint changes.

use std::path::Path;

use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::reuse_cache::reuse_enabled;
use crate::store::open_pool;
use crate::tokenizer::count_tokens;

pub const WORKING_CONTEXT_TOKENS: i64 = 800;
const MAX_ITEMS: usize = 12;
const MAX_ITEM_CHARS: usize = 200;
const MAX_OBJECTIVE_CHARS: usize = 300;

const SECTIONS: [&str; 5] = ["facts", "files", "symbols", "decisions", "open_questions"];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkingContext {
    pub objective: String,
    pub facts: Vec<String>,
    pub files: Vec<String>,
    pub symbols: Vec<String>,
    pub decisions: Vec<String>,
    pub open_questions: Vec<String>,
    pub index_fingerprint: String,
}

impl WorkingContext {
    fn is_empty(&self) -> bool {
        self.objective.is_empty()
            && self.facts.is_empty()
            && self.files.is_empty()
            && self.symbols.is_empty()
            && self.decisions.is_empty()
            && self.open_questions.is_empty()
    }

    fn section_mut(&mut self, name: &str) -> &mut Vec<String> {
        match name {
            "facts" => &mut self.facts,
            "files" => &mut self.files,
            "symbols" => &mut self.symbols,
            "decisions" => &mut self.decisions,
            "open_questions" => &mut self.open_questions,
            _ => unreachable!("section name is checked by the parser"),
        }
    }

    fn section(&self, name: &str) -> &[String] {
        match name {
            "facts" => &self.facts,
            "files" => &self.files,
            "symbols" => &self.symbols,
            "decisions" => &self.decisions,
            "open_questions" => &self.open_questions,
            _ => unreachable!("section name is checked by the parser"),
        }
    }
}

/// SHA-256 of the objective and the five lists. Identical text, identical hash.
pub fn content_hash(ctx: &WorkingContext) -> String {
    let mut hasher = Sha256::new();
    frame(&mut hasher, "objective", &ctx.objective);
    for name in SECTIONS {
        let items = ctx.section(name);
        hasher.update(format!("{name}:{}\n", items.len()).as_bytes());
        for item in items {
            frame(&mut hasher, "item", item);
        }
    }
    let dig = hasher.finalize();
    dig.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

fn frame(hasher: &mut Sha256, label: &str, text: &str) {
    hasher.update(format!("{label}:{}:{text}\n", text.len()).as_bytes());
}

fn is_stale(ctx: &WorkingContext, current_fingerprint: &str) -> bool {
    !ctx.index_fingerprint.is_empty() && ctx.index_fingerprint != current_fingerprint
}

pub fn render(ctx: &WorkingContext, current_fingerprint: &str) -> String {
    if ctx.is_empty() {
        return "<ax_working_context>\nNo working context for this conversation. Call ax_session with action add \
                once a fact, file, symbol, decision, or open question is established. action compact replaces \
                this with a shorter snapshot you write. Raw tool results stay in the conversation cache and ax_expand.\n\
                </ax_working_context>"
            .to_string();
    }
    let stale = is_stale(ctx, current_fingerprint);
    let mut lines = vec![format!(
        "<ax_working_context hash={} stale={}>",
        content_hash(ctx),
        if stale { "true" } else { "false" }
    )];
    if stale {
        lines.push(
            "Index changed since these notes. Recheck code-dependent facts. ax_session compact confirms them against the current index."
                .to_string(),
        );
    }
    if !ctx.objective.is_empty() {
        lines.push(format!("Objective: {}", ctx.objective));
    }
    for name in SECTIONS {
        let items = ctx.section(name);
        if items.is_empty() {
            continue;
        }
        let title = match name {
            "facts" => "Facts",
            "files" => "Files",
            "symbols" => "Symbols",
            "decisions" => "Decisions",
            "open_questions" => "Open questions",
            _ => unreachable!(),
        };
        lines.push(format!("{title}:"));
        for item in items {
            lines.push(format!("- {item}"));
        }
    }
    lines.push("</ax_working_context>".to_string());
    lines.join("\n")
}

fn parse_objective(request: &Value) -> Result<Option<String>, String> {
    match request.get("objective") {
        None => Ok(None),
        Some(Value::String(raw)) => {
            let text = raw.trim();
            if text.chars().count() > MAX_OBJECTIVE_CHARS {
                return Err(format!(
                    "objective exceeds {MAX_OBJECTIVE_CHARS} characters"
                ));
            }
            if text.contains('\n') {
                return Err("objective must be a single line".into());
            }
            Ok(Some(text.to_string()))
        }
        Some(_) => Err("objective must be a string".into()),
    }
}

fn parse_section(request: &Value, name: &str) -> Result<Option<Vec<String>>, String> {
    match request.get(name) {
        None => Ok(None),
        Some(Value::Array(items)) => {
            let mut out = Vec::new();
            for item in items {
                let Some(raw) = item.as_str() else {
                    return Err(format!("{name} entries must be strings"));
                };
                let text = raw.trim();
                if text.is_empty() {
                    continue;
                }
                if text.chars().count() > MAX_ITEM_CHARS {
                    return Err(format!("{name} entry exceeds {MAX_ITEM_CHARS} characters"));
                }
                if text.contains('\n') {
                    return Err(format!("{name} entries must be a single line"));
                }
                if !out.iter().any(|e| e == text) {
                    out.push(text.to_string());
                }
            }
            if out.len() > MAX_ITEMS {
                return Err(format!(
                    "{name} cannot exceed {MAX_ITEMS} entries; compact first"
                ));
            }
            Ok(Some(out))
        }
        Some(_) => Err(format!("{name} must be an array of strings")),
    }
}

fn ensure_fits(ctx: &WorkingContext, fingerprint: &str) -> Result<(), String> {
    let tokens = count_tokens(&render(ctx, fingerprint)) as i64;
    if tokens > WORKING_CONTEXT_TOKENS {
        return Err(format!(
            "working context would exceed {WORKING_CONTEXT_TOKENS} tokens; call ax_session with action compact and a shorter snapshot"
        ));
    }
    Ok(())
}

/// Returns the stored snapshot after the action. `None` means it was cleared.
fn transition(
    current: &WorkingContext,
    request: &Value,
    fingerprint: &str,
) -> Result<Option<WorkingContext>, String> {
    let action = request
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("get");
    match action {
        "get" => Ok(Some(current.clone())),
        "clear" => Ok(None),
        "add" => {
            let mut next = current.clone();
            let mut touched = false;
            if let Some(objective) = parse_objective(request)? {
                next.objective = objective;
                touched = true;
            }
            for name in SECTIONS {
                if let Some(extra) = parse_section(request, name)? {
                    touched = true;
                    let dest = next.section_mut(name);
                    for item in extra {
                        if !dest.iter().any(|e| e == &item) {
                            dest.push(item);
                        }
                    }
                    if dest.len() > MAX_ITEMS {
                        return Err(format!(
                            "{name} cannot exceed {MAX_ITEMS} entries; compact first"
                        ));
                    }
                }
            }
            if !touched {
                return Err("add needs an objective or a section".into());
            }
            if content_hash(&next) != content_hash(current) {
                next.index_fingerprint = fingerprint.to_string();
            }
            ensure_fits(&next, fingerprint)?;
            Ok(Some(next))
        }
        "update" => {
            let mut next = current.clone();
            let mut touched = false;
            if let Some(objective) = parse_objective(request)? {
                next.objective = objective;
                touched = true;
            }
            for name in SECTIONS {
                if let Some(items) = parse_section(request, name)? {
                    *next.section_mut(name) = items;
                    touched = true;
                }
            }
            if !touched {
                return Err("update needs an objective or a section".into());
            }
            if content_hash(&next) != content_hash(current) {
                next.index_fingerprint = fingerprint.to_string();
            }
            ensure_fits(&next, fingerprint)?;
            Ok(Some(next))
        }
        "compact" => {
            let objective = parse_objective(request)?.ok_or("compact requires objective")?;
            let mut next = WorkingContext {
                objective,
                index_fingerprint: fingerprint.to_string(),
                ..WorkingContext::default()
            };
            for name in SECTIONS {
                let items = parse_section(request, name)?
                    .ok_or_else(|| format!("compact requires {name}"))?;
                *next.section_mut(name) = items;
            }
            ensure_fits(&next, fingerprint)?;
            Ok(Some(next))
        }
        other => Err(format!("unknown ax_session action {other:?}")),
    }
}

fn scope(conversation: &str, root: &Path) -> String {
    format!("{conversation}\u{1f}{}", root.display())
}

async fn load(pool: &SqlitePool, key: &str) -> Result<WorkingContext, String> {
    let row: Option<(String, String, String, String, String, String, String)> = sqlx::query_as(
        "SELECT objective, facts, files, symbols, decisions, open_questions, index_fingerprint
         FROM mcp_working_context WHERE scope = ?",
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    let Some((objective, facts, files, symbols, decisions, open_questions, index_fingerprint)) =
        row
    else {
        return Ok(WorkingContext::default());
    };
    Ok(WorkingContext {
        objective,
        facts: decode_list(&facts)?,
        files: decode_list(&files)?,
        symbols: decode_list(&symbols)?,
        decisions: decode_list(&decisions)?,
        open_questions: decode_list(&open_questions)?,
        index_fingerprint,
    })
}

fn decode_list(raw: &str) -> Result<Vec<String>, String> {
    let value: Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;
    value
        .as_array()
        .ok_or_else(|| "stored section is not an array".to_string())?
        .iter()
        .map(|v| {
            v.as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| "stored entry is not a string".to_string())
        })
        .collect()
}

fn encode_list(items: &[String]) -> String {
    serde_json::to_string(items).unwrap_or_else(|_| "[]".to_string())
}

async fn save(pool: &SqlitePool, key: &str, ctx: &WorkingContext) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        "INSERT INTO mcp_working_context
         (scope, objective, facts, files, symbols, decisions, open_questions, content_hash, index_fingerprint, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(scope) DO UPDATE SET
           objective = excluded.objective,
           facts = excluded.facts,
           files = excluded.files,
           symbols = excluded.symbols,
           decisions = excluded.decisions,
           open_questions = excluded.open_questions,
           content_hash = excluded.content_hash,
           index_fingerprint = excluded.index_fingerprint,
           updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(&ctx.objective)
    .bind(encode_list(&ctx.facts))
    .bind(encode_list(&ctx.files))
    .bind(encode_list(&ctx.symbols))
    .bind(encode_list(&ctx.decisions))
    .bind(encode_list(&ctx.open_questions))
    .bind(content_hash(ctx))
    .bind(&ctx.index_fingerprint)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Turns without an `ax_session` write before preflight asks for a compact.
pub const NUDGE_AFTER_TURNS: u32 = 5;

/// One preflight line asking the agent to write or confirm its notes, or `None`.
/// `turns` counts this preflight; `stale` is whether the notes block says `stale=true`.
pub fn session_nudge(turns: u32, stale: bool) -> Option<String> {
    let ask = if stale {
        "The notes are stale: the index changed since they were written. Check them, then call ax_session with action compact.".to_string()
    } else if turns > NUDGE_AFTER_TURNS {
        format!(
            "{NUDGE_AFTER_TURNS} turns since the notes were last written. Call ax_session with action compact: \
             the objective, facts, files, symbols, decisions and open questions so far."
        )
    } else {
        return None;
    };
    Some(format!("<ax_session_nudge>{ask}</ax_session_nudge>"))
}

/// Snapshots kept per project.
const MAX_SNAPSHOTS: usize = 200;
/// A snapshot not written for this long is deleted on the next write.
const MAX_AGE_SECS: i64 = 30 * 86_400;

/// Age out old snapshots everywhere, then cap this project. `kept` is never evicted.
async fn evict(pool: &SqlitePool, root: &Path, kept: &str) -> Result<(), String> {
    let cutoff = chrono::Utc::now().timestamp() - MAX_AGE_SECS;
    sqlx::query("DELETE FROM mcp_working_context WHERE updated_at < ? AND scope != ?")
        .bind(cutoff)
        .bind(kept)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    let project = format!("\u{1f}{}", root.display());
    sqlx::query(
        "DELETE FROM mcp_working_context WHERE scope IN (
           SELECT scope FROM mcp_working_context
           WHERE substr(scope, -length(?1)) = ?1 AND scope != ?2
           ORDER BY updated_at DESC, scope DESC LIMIT -1 OFFSET ?3)",
    )
    .bind(&project)
    .bind(kept)
    .bind(MAX_SNAPSHOTS as i64 - 1)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

async fn delete(pool: &SqlitePool, key: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM mcp_working_context WHERE scope = ?")
        .bind(key)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) async fn apply_with(
    pool: &SqlitePool,
    root: &Path,
    conversation: &str,
    request: &Value,
    fingerprint: &str,
    enabled: bool,
) -> Result<String, String> {
    if !enabled {
        return Err("working context is off (AX_CONTEXT_CACHE=off)".into());
    }
    let key = scope(conversation, root);
    let current = load(pool, &key).await?;
    let action = request
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("get");
    if fingerprint.is_empty() && matches!(action, "add" | "update" | "compact") {
        return Err("index unavailable; retry after ax_sync".into());
    }
    match transition(&current, request, fingerprint)? {
        Some(_) if action == "get" => Ok(render(&current, fingerprint)),
        Some(next) => {
            save(pool, &key, &next).await?;
            evict(pool, root, &key).await?;
            Ok(render(&next, fingerprint))
        }
        None => {
            delete(pool, &key).await?;
            Ok(render(&WorkingContext::default(), fingerprint))
        }
    }
}

fn branch_header(kind: &str, parent: &str, child: &str, block: &str) -> String {
    format!("<ax_session_{kind} parent={parent} child={child}>\n{block}")
}

fn usable_child(parent: &str, child: &str) -> Result<(), String> {
    if parent == child || crate::reuse_cache::usable_session(child).as_deref() != Some(child) {
        return Err("fork needs a new session id".into());
    }
    Ok(())
}

/// Copy `parent`'s notes onto `child`. The parent row stays. An empty parent writes nothing.
pub(crate) async fn fork_with(
    pool: &SqlitePool,
    root: &Path,
    parent: &str,
    child: &str,
    fingerprint: &str,
    enabled: bool,
) -> Result<String, String> {
    if !enabled {
        return Err("working context is off (AX_CONTEXT_CACHE=off)".into());
    }
    usable_child(parent, child)?;
    let current = load(pool, &scope(parent, root)).await?;
    if current.is_empty() {
        return Err("nothing to fork".into());
    }
    let child_key = scope(child, root);
    save(pool, &child_key, &current).await?;
    evict(pool, root, &child_key).await?;
    let parent_key = scope(parent, root);
    if load(pool, &parent_key).await?.is_empty() {
        save(pool, &parent_key, &current).await?;
        evict(pool, root, &parent_key).await?;
    }
    Ok(branch_header("fork", parent, child, &render(&current, fingerprint)))
}

/// Store `request` (a compact note) as a new session. The parent notes stay readable.
pub(crate) async fn handoff_with(
    pool: &SqlitePool,
    root: &Path,
    parent: &str,
    child: &str,
    request: &Value,
    fingerprint: &str,
    enabled: bool,
) -> Result<String, String> {
    if !enabled {
        return Err("working context is off (AX_CONTEXT_CACHE=off)".into());
    }
    usable_child(parent, child)?;
    if fingerprint.is_empty() {
        return Err("index unavailable; retry after ax_sync".into());
    }
    let mut note = request.clone();
    if let Some(map) = note.as_object_mut() {
        map.insert("action".into(), Value::String("compact".into()));
    }
    let Some(next) = transition(&WorkingContext::default(), &note, fingerprint)? else {
        return Err("handoff needs a note".into());
    };
    if next.is_empty() {
        return Err("handoff needs a note".into());
    }
    let parent_ctx = load(pool, &scope(parent, root)).await?;
    let child_key = scope(child, root);
    save(pool, &child_key, &next).await?;
    evict(pool, root, &child_key).await?;
    let parent_key = scope(parent, root);
    if !parent_ctx.is_empty() && load(pool, &parent_key).await?.is_empty() {
        save(pool, &parent_key, &parent_ctx).await?;
        evict(pool, root, &parent_key).await?;
    }
    Ok(branch_header("handoff", parent, child, &render(&next, fingerprint)))
}

pub async fn fork_working_context(root: &Path, parent: &str, child: &str, fingerprint: &str) -> Result<String, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    fork_with(&pool, root, parent, child, fingerprint, reuse_enabled()).await
}

pub async fn handoff_working_context(
    root: &Path,
    parent: &str,
    child: &str,
    request: &Value,
    fingerprint: &str,
) -> Result<String, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    handoff_with(&pool, root, parent, child, request, fingerprint, reuse_enabled()).await
}

pub async fn working_context_apply(
    root: &Path,
    conversation: &str,
    request: &Value,
    fingerprint: &str,
) -> Result<String, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    apply_with(
        &pool,
        root,
        conversation,
        request,
        fingerprint,
        reuse_enabled(),
    )
    .await
}

/// Preflight block. Empty when there is nothing to repeat, the switch is off, or the db is unreachable.
/// One `unchanged` line when `known` is the current hash and the notes are not stale.
pub async fn working_context_block(root: &Path, conversation: &str, fingerprint: &str, known: Option<&str>) -> String {
    if !reuse_enabled() {
        return String::new();
    }
    let Ok(pool) = open_pool().await else {
        return String::new();
    };
    block_with(&pool, root, conversation, fingerprint, known).await
}

pub(crate) async fn block_with(
    pool: &SqlitePool,
    root: &Path,
    conversation: &str,
    fingerprint: &str,
    known: Option<&str>,
) -> String {
    let Ok(ctx) = load(pool, &scope(conversation, root)).await else {
        return String::new();
    };
    if ctx.is_empty() {
        return String::new();
    }
    let hash = content_hash(&ctx);
    if known == Some(hash.as_str()) && !is_stale(&ctx, fingerprint) {
        return format!("<ax_working_context hash={hash} unchanged/>");
    }
    render(&ctx, fingerprint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::open_pool_at;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SEQ: AtomicUsize = AtomicUsize::new(0);

    fn hash_of(block: &str) -> String {
        let rest = block.split("hash=").nth(1).unwrap_or_else(|| panic!("no hash: {block}"));
        rest.split(|c: char| c.is_whitespace() || c == '>')
            .next()
            .unwrap()
            .to_string()
    }

    async fn pool() -> (std::path::PathBuf, SqlitePool) {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("ax-working-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pool = open_pool_at(&dir.join("usage.db")).await.unwrap();
        (dir, pool)
    }

    #[test]
    fn same_text_has_the_same_hash_and_a_different_fact_does_not() {
        let mut ctx = WorkingContext {
            objective: "Refactor authentication".into(),
            facts: vec!["JWT validation is in JwtValidator".into()],
            ..WorkingContext::default()
        };
        let again = ctx.clone();
        assert_eq!(content_hash(&ctx), content_hash(&again));
        assert_eq!(content_hash(&ctx).len(), 16);
        ctx.facts.push("Claims mapped in ClaimsMapper".into());
        assert_ne!(content_hash(&ctx), content_hash(&again));
    }

    #[test]
    fn the_hash_tracks_the_text_not_only_how_many_items_there_are() {
        let mut ctx = WorkingContext {
            objective: "one".into(),
            facts: vec!["alpha".into()],
            ..WorkingContext::default()
        };
        let first = content_hash(&ctx);
        ctx.facts[0] = "beta".into();
        let second = content_hash(&ctx);
        assert_ne!(second, first, "a different fact with the same count");
        ctx.objective = "two".into();
        assert_ne!(content_hash(&ctx), second, "a different objective with the same lists");
    }

    #[test]
    fn limits_accept_the_last_allowed_character_and_reject_the_next() {
        let max = "a".repeat(MAX_OBJECTIVE_CHARS);
        assert_eq!(parse_objective(&json!({"objective": &max})).unwrap().as_deref(), Some(max.as_str()));
        let err = parse_objective(&json!({"objective": "a".repeat(MAX_OBJECTIVE_CHARS + 1)})).unwrap_err();
        assert!(err.contains("exceeds"), "{err}");

        let item = "b".repeat(MAX_ITEM_CHARS);
        assert_eq!(parse_section(&json!({"facts": [&item]}), "facts").unwrap().unwrap(), vec![item]);
        let err = parse_section(&json!({"facts": ["b".repeat(MAX_ITEM_CHARS + 1)]}), "facts").unwrap_err();
        assert!(err.contains("exceeds"), "{err}");
    }

    fn tokens_of(fact: &str) -> i64 {
        let ctx = WorkingContext { facts: vec![fact.to_string()], ..WorkingContext::default() };
        count_tokens(&render(&ctx, "fp-1")) as i64
    }

    #[test]
    fn the_token_cap_accepts_a_snapshot_of_exactly_the_limit() {
        let mut fact = String::new();
        while tokens_of(&fact) < WORKING_CONTEXT_TOKENS {
            fact.push('a');
            assert!(fact.len() < 20_000, "the cap was never reached");
        }
        if tokens_of(&fact) != WORKING_CONTEXT_TOKENS {
            fact.pop();
            let base = fact.clone();
            fact = "abcdefghijklmnopqrstuvwxyz .,;:!?".chars().find_map(|c| {
                let mut trial = base.clone();
                trial.push(c);
                (tokens_of(&trial) == WORKING_CONTEXT_TOKENS).then_some(trial)
            }).unwrap_or_else(|| panic!("no one-character step lands on {WORKING_CONTEXT_TOKENS} tokens"));
        }
        let ctx = WorkingContext { facts: vec![fact.clone()], ..WorkingContext::default() };
        assert_eq!(tokens_of(&fact), WORKING_CONTEXT_TOKENS);
        ensure_fits(&ctx, "fp-1").expect("exactly the cap is allowed");
    }

    #[test]
    fn a_changed_index_marks_notes_stale_without_dropping_them() {
        let ctx = WorkingContext {
            facts: vec!["JWT validation is in JwtValidator".into()],
            index_fingerprint: "fp-old".into(),
            ..WorkingContext::default()
        };
        let text = render(&ctx, "fp-new");
        assert!(text.contains("stale=true"), "{text}");
        assert!(text.contains("JwtValidator"), "{text}");
        assert!(text.contains("hash="), "{text}");
        let fresh = render(&ctx, "fp-old");
        assert!(fresh.contains("stale=false"), "{fresh}");
    }

    async fn note(pool: &SqlitePool, root: &Path, chat: &str, fact: &str) {
        apply_with(pool, root, chat, &json!({"action": "add", "facts": [fact]}), "fp-1", true)
            .await
            .unwrap();
    }

    async fn chats_in(pool: &SqlitePool, root: &Path) -> Vec<String> {
        let suffix = format!("\u{1f}{}", root.display());
        let rows: Vec<(String,)> = sqlx::query_as("SELECT scope FROM mcp_working_context ORDER BY scope")
            .fetch_all(pool)
            .await
            .unwrap();
        rows.into_iter()
            .filter_map(|(s,)| s.strip_suffix(&suffix).map(str::to_string))
            .collect()
    }

    async fn age(pool: &SqlitePool, root: &Path, chat: &str, updated_at: i64) {
        sqlx::query("UPDATE mcp_working_context SET updated_at = ? WHERE scope = ?")
            .bind(updated_at)
            .bind(scope(chat, root))
            .execute(pool)
            .await
            .unwrap();
    }

    #[test]
    fn the_nudge_asks_for_compact_after_five_quiet_turns_or_stale_notes() {
        for turns in 0..=NUDGE_AFTER_TURNS {
            assert_eq!(session_nudge(turns, false), None, "turn {turns}");
        }
        let quiet = session_nudge(NUDGE_AFTER_TURNS + 1, false).expect("nudge after 5 quiet turns");
        assert!(quiet.starts_with("<ax_session_nudge>") && quiet.ends_with("</ax_session_nudge>"), "{quiet}");
        assert!(quiet.contains("ax_session") && quiet.contains("compact") && quiet.contains("5 turns"), "{quiet}");
        let stale = session_nudge(1, true).expect("nudge for stale notes");
        assert!(stale.contains("stale") && stale.contains("compact"), "{stale}");
        assert_ne!(stale, quiet);
        assert_eq!(session_nudge(NUDGE_AFTER_TURNS + 1, true), Some(stale), "stale wins over quiet");
    }

    #[tokio::test]
    async fn a_known_fresh_hash_gets_one_line_and_anything_else_the_full_block() {
        let (dir, pool) = pool().await;
        assert_eq!(block_with(&pool, &dir, "chat-1", "fp-1", Some("abc")).await, "", "no notes, no block");
        note(&pool, &dir, "chat-1", "alpha returns u64").await;
        let full = block_with(&pool, &dir, "chat-1", "fp-1", None).await;
        let hash = hash_of(&full);
        assert!(full.contains("alpha returns u64"), "{full}");

        let short = block_with(&pool, &dir, "chat-1", "fp-1", Some(&hash)).await;
        assert_eq!(short, format!("<ax_working_context hash={hash} unchanged/>"));

        let other = block_with(&pool, &dir, "chat-1", "fp-1", Some("0000000000000000")).await;
        assert_eq!(other, full, "a different hash gets the full block");
        let stale = block_with(&pool, &dir, "chat-1", "fp-2", Some(&hash)).await;
        assert!(stale.contains("stale=true") && stale.contains("alpha returns u64"), "stale notes are resent: {stale}");
        let cross = block_with(&pool, &dir, "chat-2", "fp-1", Some(&hash)).await;
        assert_eq!(cross, "", "a hash from another chat does not reveal or confirm anything");
    }

    #[tokio::test]
    async fn fork_copies_the_notes_and_handoff_starts_a_new_session_from_the_note() {
        let (dir, pool) = pool().await;
        let err = fork_with(&pool, &dir, "parent", "axs_child", "fp-1", true).await.unwrap_err();
        assert!(err.contains("nothing to fork"), "{err}");
        note(&pool, &dir, "parent", "kept on the parent").await;
        let forked = fork_with(&pool, &dir, "parent", "axs_child", "fp-1", true).await.unwrap();
        assert!(forked.starts_with("<ax_session_fork parent=parent child=axs_child>"), "{forked}");
        assert!(forked.contains("kept on the parent"), "{forked}");
        let parent = apply_with(&pool, &dir, "parent", &json!({"action": "get"}), "fp-1", true).await.unwrap();
        assert!(parent.contains("kept on the parent"), "the parent stays: {parent}");
        let child = apply_with(&pool, &dir, "axs_child", &json!({"action": "get"}), "fp-1", true).await.unwrap();
        assert_eq!(hash_of(&child), hash_of(&parent));

        let note = json!({
            "action": "handoff",
            "objective": "Continue auth",
            "facts": ["JWT is in TokenValidator"],
            "files": [],
            "symbols": [],
            "decisions": [],
            "open_questions": []
        });
        let handed = handoff_with(&pool, &dir, "parent", "axs_next", &note, "fp-1", true).await.unwrap();
        assert!(handed.starts_with("<ax_session_handoff parent=parent child=axs_next>"), "{handed}");
        assert!(handed.contains("TokenValidator") && !handed.contains("kept on the parent"), "{handed}");
        let parent_after = apply_with(&pool, &dir, "parent", &json!({"action": "get"}), "fp-1", true).await.unwrap();
        assert!(parent_after.contains("kept on the parent"), "{parent_after}");
        let empty = json!({"action": "handoff", "objective": "", "facts": [], "files": [], "symbols": [], "decisions": [], "open_questions": []});
        let err = handoff_with(&pool, &dir, "parent", "axs_empty", &empty, "fp-1", true).await.unwrap_err();
        assert!(err.contains("handoff needs a note"), "{err}");
        let err = handoff_with(&pool, &dir, "parent", "axs_empty", &note, "", true).await.unwrap_err();
        assert!(err.contains("index unavailable"), "{err}");
        let err = fork_with(&pool, &dir, "parent", "bad id", "fp-1", true).await.unwrap_err();
        assert!(err.contains("fork needs a new session id"), "{err}");
        let from_empty = handoff_with(&pool, &dir, "nobody", "axs_from_empty", &note, "fp-1", true).await.unwrap();
        assert!(from_empty.contains("TokenValidator"), "{from_empty}");
        let nobody = apply_with(&pool, &dir, "nobody", &json!({"action": "get"}), "fp-1", true).await.unwrap();
        assert!(!nobody.contains("TokenValidator"), "an empty parent is not filled: {nobody}");
    }

    #[tokio::test]
    async fn writes_need_a_readable_index_and_leave_the_snapshot_unchanged() {
        let (dir, pool) = pool().await;
        note(&pool, &dir, "chat-1", "kept").await;
        let before = apply_with(&pool, &dir, "chat-1", &json!({"action": "get"}), "fp-1", true).await.unwrap();
        let writes = [
            json!({"action": "add", "facts": ["new"]}),
            json!({"action": "update", "facts": ["new"]}),
            json!({"action": "compact", "objective": "", "facts": ["new"], "files": [], "symbols": [], "decisions": [], "open_questions": []}),
        ];
        for request in writes {
            let err = apply_with(&pool, &dir, "chat-1", &request, "", true).await.unwrap_err();
            assert!(err.contains("index unavailable"), "{err}");
        }
        let after = apply_with(&pool, &dir, "chat-1", &json!({"action": "get"}), "fp-1", true).await.unwrap();
        assert_eq!(after, before);
        assert!(apply_with(&pool, &dir, "chat-1", &json!({"action": "get"}), "", true).await.unwrap().contains("kept"));
        apply_with(&pool, &dir, "chat-1", &json!({"action": "clear"}), "", true).await.unwrap();
        assert!(chats_in(&pool, &dir).await.is_empty(), "clear works without an index");
    }

    #[tokio::test]
    async fn the_snapshot_past_the_cap_evicts_the_oldest_of_that_project_only() {
        let (dir, pool) = pool().await;
        let other = dir.join("other-project");
        let now = chrono::Utc::now().timestamp();
        note(&pool, &other, "elsewhere", "other project").await;
        age(&pool, &other, "elsewhere", now - 10_000).await;
        for i in 0..MAX_SNAPSHOTS {
            let chat = format!("chat-{i:03}");
            note(&pool, &dir, &chat, "x").await;
            age(&pool, &dir, &chat, now - 5_000 + i as i64).await;
        }
        assert_eq!(chats_in(&pool, &dir).await.len(), MAX_SNAPSHOTS);
        note(&pool, &dir, "chat-new", "x").await;
        let kept = chats_in(&pool, &dir).await;
        assert_eq!(kept.len(), MAX_SNAPSHOTS);
        assert!(!kept.contains(&"chat-000".to_string()), "the oldest goes");
        assert!(kept.contains(&"chat-001".to_string()));
        assert!(kept.contains(&"chat-new".to_string()), "the snapshot just written stays");
        assert_eq!(chats_in(&pool, &other).await, vec!["elsewhere".to_string()]);
    }

    #[tokio::test]
    async fn fork_at_the_cap_keeps_the_parent_and_the_child_inside_the_cap() {
        let (dir, pool) = pool().await;
        let now = chrono::Utc::now().timestamp();
        note(&pool, &dir, "parent", "kept on the parent").await;
        age(&pool, &dir, "parent", now - 5_000).await;
        for i in 1..MAX_SNAPSHOTS {
            let chat = format!("chat-{i:03}");
            note(&pool, &dir, &chat, "x").await;
            age(&pool, &dir, &chat, now - 5_000 + i as i64).await;
        }
        assert_eq!(chats_in(&pool, &dir).await.len(), MAX_SNAPSHOTS);
        let forked = fork_with(&pool, &dir, "parent", "axs_child", "fp-1", true).await.unwrap();
        assert!(forked.contains("kept on the parent"), "{forked}");
        let kept = chats_in(&pool, &dir).await;
        assert_eq!(kept.len(), MAX_SNAPSHOTS, "{kept:?}");
        assert!(kept.contains(&"parent".to_string()), "the parent stays: {kept:?}");
        assert!(kept.contains(&"axs_child".to_string()), "the child stays: {kept:?}");
    }

    #[tokio::test]
    async fn snapshots_unused_for_30_days_go_on_the_next_write() {
        let (dir, pool) = pool().await;
        let other = dir.join("other-project");
        let now = chrono::Utc::now().timestamp();
        note(&pool, &dir, "old", "x").await;
        note(&pool, &other, "old-elsewhere", "x").await;
        note(&pool, &dir, "recent", "x").await;
        age(&pool, &dir, "old", now - MAX_AGE_SECS - 60).await;
        age(&pool, &other, "old-elsewhere", now - MAX_AGE_SECS - 60).await;
        age(&pool, &dir, "recent", now - MAX_AGE_SECS + 3_600).await;
        note(&pool, &dir, "writer", "x").await;
        assert_eq!(chats_in(&pool, &dir).await, vec!["recent".to_string(), "writer".to_string()]);
        assert!(chats_in(&pool, &other).await.is_empty(), "age-out is not limited to one project");
    }

    #[tokio::test]
    async fn add_then_get_round_trips_and_dedupes() {
        let (dir, pool) = pool().await;
        let added = apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({"action": "add", "objective": "Refactor authentication", "facts": ["JWT validation is in JwtValidator"], "files": ["src/Auth/JwtValidator.cs"]}),
            "fp-1",
            true,
        )
        .await
        .unwrap();
        assert!(added.contains("stale=false"), "{added}");
        assert!(
            added.contains("Objective: Refactor authentication"),
            "{added}"
        );
        let again = apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({"action": "add", "facts": ["JWT validation is in JwtValidator", "Middleware injects claims"]}),
            "fp-1",
            true,
        )
        .await
        .unwrap();
        assert_eq!(
            again.matches("JWT validation is in JwtValidator").count(),
            1,
            "{again}"
        );
        assert!(again.contains("Middleware injects claims"), "{again}");
        let got = apply_with(&pool, &dir, "chat-1", &json!({}), "fp-1", true)
            .await
            .unwrap();
        assert!(got.contains("Middleware injects claims"), "{got}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn another_conversation_does_not_see_the_snapshot() {
        let (dir, pool) = pool().await;
        apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({"action": "add", "facts": ["only chat-1"]}),
            "fp-1",
            true,
        )
        .await
        .unwrap();
        let other = apply_with(
            &pool,
            &dir,
            "chat-2",
            &json!({"action": "get"}),
            "fp-1",
            true,
        )
        .await
        .unwrap();
        assert!(other.contains("No working context"), "{other}");
        assert!(!other.contains("only chat-1"), "{other}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn over_limit_add_leaves_the_stored_snapshot_unchanged() {
        let (dir, pool) = pool().await;
        apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({"action": "add", "facts": ["kept"]}),
            "fp-1",
            true,
        )
        .await
        .unwrap();
        let long = |prefix: &str| -> Vec<String> {
            (0..12)
                .map(|i| format!("{prefix} {i} {}", "y".repeat(80)))
                .collect()
        };
        let err = apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({
                "action": "update",
                "facts": long("fact"),
                "files": long("file"),
                "symbols": long("sym"),
                "decisions": long("decision"),
                "open_questions": long("question")
            }),
            "fp-1",
            true,
        )
        .await
        .unwrap_err();
        assert!(err.contains("800"), "{err}");
        let got = apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({"action": "get"}),
            "fp-1",
            true,
        )
        .await
        .unwrap();
        assert!(got.contains("- kept"), "{got}");
        assert!(!got.contains("fact 0"), "{got}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn thirteenth_short_fact_is_rejected() {
        let (dir, pool) = pool().await;
        let facts: Vec<_> = (0..12).map(|i| format!("fact {i}")).collect();
        apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({"action": "add", "facts": facts}),
            "fp-1",
            true,
        )
        .await
        .unwrap();
        let err = apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({"action": "add", "facts": ["fact 12"]}),
            "fp-1",
            true,
        )
        .await
        .unwrap_err();
        assert!(err.contains("12"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn duplicate_add_does_not_clear_a_stale_fingerprint() {
        let (dir, pool) = pool().await;
        apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({"action": "add", "facts": ["kept"]}),
            "fp-1",
            true,
        )
        .await
        .unwrap();
        let stale = apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({"action": "add", "facts": ["kept"]}),
            "fp-2",
            true,
        )
        .await
        .unwrap();
        assert!(stale.contains("stale=true"), "{stale}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn compact_replaces_everything_and_confirms_the_index() {
        let (dir, pool) = pool().await;
        apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({"action": "add", "facts": ["verbose finding that should not survive compact"]}),
            "fp-1",
            true,
        )
        .await
        .unwrap();
        let text = apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({
                "action": "compact",
                "objective": "Understand authentication",
                "facts": ["JWT validation is in JwtValidator"],
                "files": ["src/Auth/JwtValidator.cs"],
                "symbols": ["JwtValidator"],
                "decisions": ["Do not change ClaimsMapper"],
                "open_questions": ["Where are refresh tokens generated?"]
            }),
            "fp-2",
            true,
        )
        .await
        .unwrap();
        assert!(text.contains("stale=false"), "{text}");
        assert!(text.contains("Do not change ClaimsMapper"), "{text}");
        assert!(!text.contains("verbose finding"), "{text}");
        let missing = apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({"action": "compact", "objective": "x", "facts": []}),
            "fp-2",
            true,
        )
        .await
        .unwrap_err();
        assert!(missing.contains("requires"), "{missing}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn update_replaces_one_section_and_clear_removes_the_row() {
        let (dir, pool) = pool().await;
        apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({"action": "add", "facts": ["old"], "decisions": ["keep"]}),
            "fp-1",
            true,
        )
        .await
        .unwrap();
        let updated = apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({"action": "update", "facts": ["new"]}),
            "fp-1",
            true,
        )
        .await
        .unwrap();
        assert!(updated.contains("- new"), "{updated}");
        assert!(!updated.contains("- old"), "{updated}");
        assert!(updated.contains("- keep"), "{updated}");
        let cleared = apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({"action": "clear"}),
            "fp-1",
            true,
        )
        .await
        .unwrap();
        assert!(cleared.contains("No working context"), "{cleared}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn off_switch_rejects_and_writes_nothing() {
        let (dir, pool) = pool().await;
        let err = apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({"action": "add", "facts": ["nope"]}),
            "fp-1",
            false,
        )
        .await
        .unwrap_err();
        assert!(err.contains("AX_CONTEXT_CACHE=off"), "{err}");
        let stored = apply_with(
            &pool,
            &dir,
            "chat-1",
            &json!({"action": "get"}),
            "fp-1",
            true,
        )
        .await
        .unwrap();
        assert!(stored.contains("No working context"), "{stored}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn later_prompts_reuse_the_same_content_hash() {
        let (dir, pool) = pool().await;
        let first = apply_with(
            &pool,
            &dir,
            "chat",
            &json!({
                "action": "add",
                "objective": "Understand authentication",
                "facts": ["JWT validation is in JwtValidator"],
                "files": ["src/Auth/JwtValidator.cs"]
            }),
            "fp-1",
            true,
        )
        .await
        .unwrap();
        let hash = hash_of(&first);
        let second = apply_with(&pool, &dir, "chat", &json!({"action": "get"}), "fp-1", true)
            .await
            .unwrap();
        assert_eq!(hash_of(&second), hash, "prompt 2 must return the same snapshot: {second}");
        assert!(second.contains("JWT validation is in JwtValidator"), "{second}");
        let third = apply_with(
            &pool,
            &dir,
            "chat",
            &json!({"action": "add", "facts": ["JWT validation is in JwtValidator"]}),
            "fp-1",
            true,
        )
        .await
        .unwrap();
        assert_eq!(hash_of(&third), hash, "a repeated fact must not fork the snapshot: {third}");
        let fourth = apply_with(
            &pool,
            &dir,
            "chat",
            &json!({"action": "add", "decisions": ["Do not change ClaimsMapper"]}),
            "fp-1",
            true,
        )
        .await
        .unwrap();
        assert_ne!(hash_of(&fourth), hash);
        assert!(fourth.contains("JWT validation is in JwtValidator"), "{fourth}");
        assert!(fourth.contains("Do not change ClaimsMapper"), "{fourth}");
        let fifth = apply_with(&pool, &dir, "chat", &json!({"action": "get"}), "fp-2", true)
            .await
            .unwrap();
        assert_eq!(hash_of(&fifth), hash_of(&fourth), "{fifth}");
        assert!(fifth.contains("stale=true"), "a new index marks the same notes stale: {fifth}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn newline_and_overlong_entries_are_rejected() {
        let current = WorkingContext::default();
        let newline =
            transition(&current, &json!({"action": "add", "facts": ["a\nb"]}), "fp").unwrap_err();
        assert!(newline.contains("single line"), "{newline}");
        let long = "x".repeat(201);
        let over =
            transition(&current, &json!({"action": "add", "facts": [long]}), "fp").unwrap_err();
        assert!(over.contains("200"), "{over}");
    }
}
