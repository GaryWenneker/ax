//! Named cues, tiers, archive search, graph episodes, and consolidation.
//!
//! MCP and the CLI both call [`search`]. Ranking lives here, not in either wrapper.

use std::path::Path;

use sqlx::SqlitePool;

use ax_utils::errors::AxError;

use crate::history::HistoryEntry;
use crate::store::{self, db_err_pub};
use crate::types::{MemoryMatch, MemoryRow, RememberInput};

pub const TIER_EPISODIC: &str = "t1";
pub const TIER_SEMANTIC: &str = "t2";
pub const TIER_ARCHIVE: &str = "t3";

/// Tier for a memory kind. Git captures and folder docs stay in the archive.
pub fn tier_for_kind(kind: &str) -> &'static str {
    if kind == crate::turns::TURN_KIND {
        TIER_EPISODIC
    } else if kind == "graph_diff"
        || kind == "activity"
        || kind == "git"
        || kind == crate::folder_sync::DOC_KIND
    {
        TIER_ARCHIVE
    } else {
        TIER_SEMANTIC
    }
}
pub const STATUS_ACTIVE: &str = "active";
pub const STATUS_CANDIDATE: &str = "candidate";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    Hot,
    Archive,
}

impl Depth {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "" | "hot" => Ok(Self::Hot),
            "archive" => Ok(Self::Archive),
            other => Err(format!("depth must be hot or archive, got {other}")),
        }
    }
}

/// One search shape for `ax_recall` and `ax recall`.
#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub query: String,
    pub cue: Option<String>,
    pub id: Option<String>,
    pub depth: Depth,
    pub files: Vec<String>,
    pub limit: usize,
}

impl Default for SearchRequest {
    fn default() -> Self {
        Self {
            query: String::new(),
            cue: None,
            id: None,
            depth: Depth::Hot,
            files: Vec::new(),
            limit: 5,
        }
    }
}

pub fn normalize_cue(raw: &str) -> Option<String> {
    let lower = raw.trim().to_lowercase();
    let mut out = String::new();
    let mut dash = false;
    for c in lower.chars() {
        if c.is_alphanumeric() || c == '_' {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let cue = out.trim_matches('-').to_string();
    if cue.chars().count() >= 2 {
        Some(cue)
    } else {
        None
    }
}

fn is_stop_cue(cue: &str) -> bool {
    matches!(
        cue,
        "the"
            | "a"
            | "an"
            | "and"
            | "or"
            | "of"
            | "in"
            | "on"
            | "to"
            | "for"
            | "with"
            | "is"
            | "are"
            | "de"
            | "het"
            | "een"
            | "en"
            | "van"
            | "voor"
            | "met"
            | "dat"
            | "die"
            | "je"
            | "ik"
    )
}

fn automatic_cues(text: &str) -> Vec<String> {
    let mut cues = Vec::new();
    let mut push = |raw: &str| {
        let Some(cue) = normalize_cue(raw) else {
            return;
        };
        let len = cue.chars().count();
        if (3..=80).contains(&len) && !is_stop_cue(&cue) {
            cues.push(cue);
        }
    };
    push(text);
    for token in text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
        push(token);
    }
    cues.sort();
    cues.dedup();
    cues
}

fn cue_tokens(text: &str) -> Vec<String> {
    let mut cues = Vec::new();
    if let Some(whole) = normalize_cue(text) {
        if whole.chars().count() <= 80 {
            cues.push(whole);
        }
    }
    for token in text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
        if let Some(cue) = normalize_cue(token) {
            cues.push(cue);
        }
    }
    cues.sort();
    cues.dedup();
    cues
}

pub async fn register_cues(
    pool: &SqlitePool,
    memory: &MemoryRow,
    triggers: &[String],
) -> Result<(), AxError> {
    sqlx::query("DELETE FROM memory_cues WHERE memory_id = ?")
        .bind(&memory.id)
        .execute(pool)
        .await
        .map_err(db_err_pub)?;
    let mut rows: Vec<(String, &str)> = Vec::new();
    for cue in automatic_cues(&memory.title) {
        rows.push((cue, "title"));
    }
    for tag in &memory.tags {
        for cue in automatic_cues(tag) {
            rows.push((cue, "tag"));
        }
    }
    for file in &memory.files {
        let base = Path::new(file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(file);
        for cue in automatic_cues(base) {
            rows.push((cue, "file"));
        }
        for name in symbol_names(pool, file).await? {
            for cue in automatic_cues(&name) {
                rows.push((cue, "symbol"));
            }
        }
    }
    for trigger in triggers {
        if let Some(cue) = normalize_cue(trigger) {
            rows.push((cue, "trigger"));
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(b.1)));
    rows.dedup();
    for (cue, source) in rows {
        sqlx::query(
            "INSERT OR IGNORE INTO memory_cues (cue, memory_id, weight, source) VALUES (?, ?, 1.0, ?)",
        )
        .bind(&cue)
        .bind(&memory.id)
        .bind(source)
        .execute(pool)
        .await
        .map_err(db_err_pub)?;
    }
    Ok(())
}

pub async fn cues_for(pool: &SqlitePool, memory_id: &str) -> Result<Vec<String>, AxError> {
    sqlx::query_scalar("SELECT cue FROM memory_cues WHERE memory_id = ? ORDER BY cue")
        .bind(memory_id)
        .fetch_all(pool)
        .await
        .map_err(db_err_pub)
}

async fn lookup_cue(
    pool: &SqlitePool,
    cue: &str,
    limit: usize,
) -> Result<Vec<MemoryMatch>, AxError> {
    let Some(cue) = normalize_cue(cue) else {
        return Ok(Vec::new());
    };
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT memory_id FROM memory_cues WHERE cue = ? ORDER BY weight DESC, memory_id LIMIT ?",
    )
    .bind(&cue)
    .bind(limit as i64)
    .fetch_all(pool)
    .await
    .map_err(db_err_pub)?;
    let mut matches = Vec::new();
    for id in ids {
        if let Some(memory) = store::get(pool, &id).await? {
            if memory.enabled {
                matches.push(MemoryMatch {
                    memory,
                    score: 100.0,
                });
            }
        }
    }
    Ok(matches)
}

/// Exact cue, id, or hybrid search. Cue and id see every tier. Free text
/// follows `depth`: hot skips the archive, archive is only the archive.
pub async fn search(
    pool: &SqlitePool,
    request: &SearchRequest,
) -> Result<Vec<MemoryMatch>, AxError> {
    let limit = request.limit.max(1);
    if let Some(id) = request.id.as_deref().filter(|id| !id.is_empty()) {
        return Ok(store::get(pool, id)
            .await?
            .filter(|m| m.enabled)
            .map(|memory| MemoryMatch { memory, score: 1.0 })
            .into_iter()
            .collect());
    }
    if let Some(cue) = request.cue.as_deref().filter(|cue| !cue.is_empty()) {
        return lookup_cue(pool, cue, limit).await;
    }
    let mut matches = store::recall_scoped(
        pool,
        &request.query,
        limit,
        &request.files,
        request.depth == Depth::Archive,
    )
    .await?;
    if request.depth == Depth::Archive {
        matches = spread_archive(pool, matches, limit).await?;
    }
    Ok(matches)
}

async fn spread_archive(
    pool: &SqlitePool,
    mut matches: Vec<MemoryMatch>,
    limit: usize,
) -> Result<Vec<MemoryMatch>, AxError> {
    let seeds: Vec<(String, f64)> = matches
        .iter()
        .map(|hit| (hit.memory.id.clone(), hit.score))
        .collect();
    let mut seen: std::collections::BTreeSet<String> =
        seeds.iter().map(|(id, _)| id.clone()).collect();
    for (id, score) in seeds {
        if matches.len() >= limit {
            break;
        }
        let mut extra = link_neighbors(pool, &id).await?;
        extra.extend(file_neighbors(pool, &id).await?);
        extra.extend(symbol_neighbors(pool, &id).await?);
        for neighbor in extra {
            if matches.len() >= limit || !seen.insert(neighbor.clone()) {
                continue;
            }
            if let Some(memory) = store::get(pool, &neighbor).await? {
                if memory.enabled {
                    matches.push(MemoryMatch {
                        memory,
                        score: (score * 0.5).max(1.0),
                    });
                }
            }
        }
    }
    Ok(matches)
}

async fn link_neighbors(pool: &SqlitePool, id: &str) -> Result<Vec<String>, AxError> {
    sqlx::query_scalar(
        "SELECT to_id FROM memory_links WHERE from_id = ?
         UNION SELECT from_id FROM memory_links WHERE to_id = ?",
    )
    .bind(id)
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(db_err_pub)
}

async fn file_neighbors(pool: &SqlitePool, id: &str) -> Result<Vec<String>, AxError> {
    let files_json: Option<String> = sqlx::query_scalar("SELECT files FROM memories WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(db_err_pub)?;
    let files: Vec<String> =
        serde_json::from_str(&files_json.unwrap_or_else(|| "[]".into())).unwrap_or_default();
    let mut ids = Vec::new();
    for file in files {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT m.id FROM memories m, json_each(m.files) AS f
             WHERE m.enabled = 1 AND m.id != ? AND f.value = ?",
        )
        .bind(id)
        .bind(&file)
        .fetch_all(pool)
        .await
        .map_err(db_err_pub)?;
        ids.extend(rows);
    }
    Ok(ids)
}

async fn symbol_neighbors(pool: &SqlitePool, id: &str) -> Result<Vec<String>, AxError> {
    sqlx::query_scalar(
        "SELECT DISTINCT other.memory_id
         FROM memory_cues AS mine
         JOIN memory_cues AS other
           ON other.cue = mine.cue AND other.memory_id != mine.memory_id
         WHERE mine.memory_id = ? AND mine.source = 'symbol'",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(db_err_pub)
}

pub async fn prompt_cues(
    pool: &SqlitePool,
    prompt: &str,
    limit: usize,
) -> Result<Vec<MemoryMatch>, AxError> {
    let tokens: Vec<String> = cue_tokens(prompt)
        .into_iter()
        .filter(|token| token.chars().count() >= 3 && !is_stop_cue(token))
        .take(32)
        .collect();
    if tokens.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let json = serde_json::to_string(&tokens).unwrap_or_else(|_| "[]".into());
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT memory_id FROM memory_cues
         WHERE cue IN (SELECT value FROM json_each(?))
         LIMIT ?",
    )
    .bind(json)
    .bind((limit.saturating_mul(4).max(limit)) as i64)
    .fetch_all(pool)
    .await
    .map_err(db_err_pub)?;
    let mut hits = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for id in ids {
        if hits.len() >= limit || !seen.insert(id.clone()) {
            continue;
        }
        if let Some(memory) = store::get(pool, &id).await? {
            if memory.enabled && injectable(&memory) {
                hits.push(MemoryMatch {
                    memory,
                    score: 1_000.0,
                });
            }
        }
    }
    Ok(hits)
}

fn injectable(memory: &MemoryRow) -> bool {
    memory.enabled
        && memory.kind != crate::turns::TURN_KIND
        && memory.kind != crate::folder_sync::DOC_KIND
        && memory.kind != "graph_diff"
        && memory.kind != "activity"
}

pub async fn is_injectable(pool: &SqlitePool, id: &str) -> Result<bool, AxError> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT tier, status FROM memories WHERE id = ? AND enabled = 1")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(db_err_pub)?;
    Ok(row
        .map(|(tier, status)| tier != TIER_ARCHIVE && status == STATUS_ACTIVE)
        .unwrap_or(false))
}

pub async fn stamp_memory(
    pool: &SqlitePool,
    id: &str,
    kind: &str,
    model: &str,
) -> Result<(), AxError> {
    let tier = tier_for_kind(kind);
    sqlx::query(
        "UPDATE memories SET tier = ?, embedding_model = ?, status = COALESCE(NULLIF(status, ''), 'active') WHERE id = ?",
    )
    .bind(tier)
    .bind(model)
    .bind(id)
    .execute(pool)
    .await
    .map_err(db_err_pub)?;
    if tier == TIER_ARCHIVE {
        index_archive_vector(pool, id).await?;
    }
    Ok(())
}

async fn ensure_vec_table(pool: &SqlitePool) -> Result<(), AxError> {
    sqlx::query("CREATE VIRTUAL TABLE IF NOT EXISTS memory_vec USING vec0(embedding float[256])")
        .execute(pool)
        .await
        .map_err(db_err_pub)?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS memory_vec_map (memory_id TEXT PRIMARY KEY, rowid INTEGER NOT NULL)",
    )
    .execute(pool)
    .await
    .map_err(db_err_pub)?;
    Ok(())
}

fn embedding_json(values: &[f32]) -> String {
    let parts: Vec<String> = values.iter().map(|v| format!("{v:.6}")).collect();
    format!("[{}]", parts.join(","))
}

pub async fn index_archive_vector(pool: &SqlitePool, id: &str) -> Result<(), AxError> {
    let row: Option<(Vec<u8>, String)> = sqlx::query_as(
        "SELECT embedding, embedding_model FROM memories WHERE id = ? AND embedding IS NOT NULL",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(db_err_pub)?;
    let Some((blob, model)) = row else {
        return Ok(());
    };
    let (_, active) = crate::embed::embed_labeled("ax");
    if model != active {
        let existing: Option<i64> =
            match sqlx::query_scalar("SELECT rowid FROM memory_vec_map WHERE memory_id = ?")
                .bind(id)
                .fetch_optional(pool)
                .await
            {
                Ok(row) => row,
                Err(error) if error.to_string().contains("no such table") => None,
                Err(error) => return Err(db_err_pub(error)),
            };
        if let Some(rowid) = existing {
            sqlx::query("DELETE FROM memory_vec WHERE rowid = ?")
                .bind(rowid)
                .execute(pool)
                .await
                .map_err(db_err_pub)?;
            sqlx::query("DELETE FROM memory_vec_map WHERE memory_id = ?")
                .bind(id)
                .execute(pool)
                .await
                .map_err(db_err_pub)?;
        }
        return Ok(());
    }
    let Some(vector) = crate::embed::blob_to_embedding(&blob) else {
        return Ok(());
    };
    if vector.len() != crate::embed::EMBED_DIM {
        return Ok(());
    }
    ensure_vec_table(pool).await?;
    let existing: Option<i64> =
        sqlx::query_scalar("SELECT rowid FROM memory_vec_map WHERE memory_id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(db_err_pub)?;
    if let Some(rowid) = existing {
        sqlx::query("DELETE FROM memory_vec WHERE rowid = ?")
            .bind(rowid)
            .execute(pool)
            .await
            .map_err(db_err_pub)?;
    }
    sqlx::query("INSERT INTO memory_vec(embedding) VALUES (?)")
        .bind(embedding_json(&vector))
        .execute(pool)
        .await
        .map_err(db_err_pub)?;
    let rowid: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
        .fetch_one(pool)
        .await
        .map_err(db_err_pub)?;
    sqlx::query(
        "INSERT INTO memory_vec_map (memory_id, rowid) VALUES (?, ?)
         ON CONFLICT(memory_id) DO UPDATE SET rowid = excluded.rowid",
    )
    .bind(id)
    .bind(rowid)
    .execute(pool)
    .await
    .map_err(db_err_pub)?;
    Ok(())
}

pub async fn knn(pool: &SqlitePool, query: &str, limit: usize) -> Result<Vec<String>, AxError> {
    let (vector, active) = crate::embed::embed_labeled(query);
    if vector.iter().all(|v| *v == 0.0) {
        return Ok(Vec::new());
    }
    ensure_vec_table(pool).await?;
    let rows: Vec<(i64,)> =
        sqlx::query_as("SELECT rowid FROM memory_vec WHERE embedding MATCH ? AND k = ?")
            .bind(embedding_json(&vector))
            .bind(limit.max(1) as i64)
            .fetch_all(pool)
            .await
            .map_err(db_err_pub)?;
    let mut ids = Vec::new();
    for (rowid,) in rows {
        let memory_id: Option<(String, String)> = sqlx::query_as(
            "SELECT m.id, m.embedding_model FROM memory_vec_map map
             JOIN memories m ON m.id = map.memory_id
             WHERE map.rowid = ? AND m.enabled = 1 AND m.tier = 't3'",
        )
        .bind(rowid)
        .fetch_optional(pool)
        .await
        .map_err(db_err_pub)?;
        if let Some((id, model)) = memory_id {
            if model == active {
                ids.push(id);
            }
        }
    }
    Ok(ids)
}

pub async fn demote_turn(pool: &SqlitePool, id: &str) -> Result<(), AxError> {
    sqlx::query("UPDATE memories SET tier = ? WHERE id = ? AND kind = ?")
        .bind(TIER_ARCHIVE)
        .bind(id)
        .bind(crate::turns::TURN_KIND)
        .execute(pool)
        .await
        .map_err(db_err_pub)?;
    index_archive_vector(pool, id).await
}

pub async fn confirm_memory(pool: &SqlitePool, id: &str) -> Result<bool, AxError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let result = sqlx::query(
        "UPDATE memories SET status = ?, confidence = 1.0, updated_at = ? WHERE id = ? AND status = ?",
    )
    .bind(STATUS_ACTIVE)
    .bind(now)
    .bind(id)
    .bind(STATUS_CANDIDATE)
    .execute(pool)
    .await
    .map_err(db_err_pub)?;
    Ok(result.rows_affected() > 0)
}

pub async fn consolidate(pool: &SqlitePool) -> Result<usize, AxError> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, title, body FROM memories
         WHERE enabled = 1
           AND ((kind = ? AND tier = ?) OR kind = 'git')
           AND (lower(body) LIKE '%because%' OR lower(body) LIKE '%decided%' OR lower(body) LIKE '%decision%')
         ORDER BY updated_at DESC LIMIT 20",
    )
    .bind(crate::turns::TURN_KIND)
    .bind(TIER_EPISODIC)
    .fetch_all(pool)
    .await
    .map_err(db_err_pub)?;
    let mut created = 0usize;
    for (source_id, title, body) in rows {
        let linked: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM memory_links WHERE to_id = ? AND kind = 'source'",
        )
        .bind(&source_id)
        .fetch_one(pool)
        .await
        .map_err(db_err_pub)?;
        if linked > 0 {
            continue;
        }
        let row = store::remember(
            pool,
            RememberInput {
                title: format!("Candidate: {title}"),
                body: body.clone(),
                kind: Some("decision".into()),
                tags: vec!["consolidated".into()],
                files: vec![],
                source: Some("consolidation".into()),
            },
        )
        .await?;
        sqlx::query("UPDATE memories SET status = ? WHERE id = ?")
            .bind(STATUS_CANDIDATE)
            .bind(&row.id)
            .execute(pool)
            .await
            .map_err(db_err_pub)?;
        sqlx::query(
            "INSERT OR IGNORE INTO memory_links (from_id, to_id, kind) VALUES (?, ?, 'source')",
        )
        .bind(&row.id)
        .bind(&source_id)
        .execute(pool)
        .await
        .map_err(db_err_pub)?;
        let similar = store::find_similar(pool, &format!("{title} {body}"), Some(&row.id), 0.80, 3)
            .await
            .unwrap_or_default();
        for other in &similar {
            if other.memory.id == source_id {
                continue;
            }
            sqlx::query(
                "INSERT OR IGNORE INTO memory_links (from_id, to_id, kind) VALUES (?, ?, 'supersedes')",
            )
            .bind(&row.id)
            .bind(&other.memory.id)
            .execute(pool)
            .await
            .map_err(db_err_pub)?;
            if other.memory.kind != crate::turns::TURN_KIND {
                let _ = confirm_memory(pool, &row.id).await?;
            }
        }
        created += 1;
    }
    Ok(created)
}

pub async fn capture_graph_episodes(pool: &SqlitePool) -> Result<usize, AxError> {
    let nodes: Vec<(String, String, String, String, Option<String>, i64, i64)> = sqlx::query_as(
        "SELECT id, name, kind, file_path, signature, start_line, end_line FROM nodes",
    )
    .fetch_all(pool)
    .await
    .map_err(db_err_pub)?;
    let known: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_symbol_state")
        .fetch_one(pool)
        .await
        .map_err(db_err_pub)?;
    let mut written = 0usize;
    for (id, name, kind, file_path, signature, start, end) in nodes {
        let fingerprint = format!(
            "{name}|{kind}|{file_path}|{}|{start}|{end}",
            signature.unwrap_or_default()
        );
        let previous: Option<String> =
            sqlx::query_scalar("SELECT fingerprint FROM graph_symbol_state WHERE symbol_id = ?")
                .bind(&id)
                .fetch_optional(pool)
                .await
                .map_err(db_err_pub)?;
        let change = match &previous {
            None if known == 0 => None,
            None => Some("added"),
            Some(old) if old == &fingerprint => None,
            Some(_) => Some("changed"),
        };
        if let Some(change) = change {
            write_graph_episode(pool, &id, change, &name, &kind, &file_path, &fingerprint).await?;
            written += 1;
        }
        sqlx::query(
            "INSERT INTO graph_symbol_state (symbol_id, fingerprint) VALUES (?, ?)
             ON CONFLICT(symbol_id) DO UPDATE SET fingerprint = excluded.fingerprint",
        )
        .bind(&id)
        .bind(&fingerprint)
        .execute(pool)
        .await
        .map_err(db_err_pub)?;
    }
    if known == 0 {
        return Ok(0);
    }
    let removed: Vec<(String, String)> = sqlx::query_as(
        "SELECT symbol_id, fingerprint FROM graph_symbol_state
         WHERE symbol_id NOT IN (SELECT id FROM nodes)",
    )
    .fetch_all(pool)
    .await
    .map_err(db_err_pub)?;
    for (id, fingerprint) in removed {
        let mut parts = fingerprint.split('|');
        let name = parts.next().unwrap_or(id.as_str()).to_string();
        let kind = parts.next().unwrap_or("symbol").to_string();
        let file_path = parts.next().unwrap_or("").to_string();
        write_graph_episode(pool, &id, "removed", &name, &kind, &file_path, &fingerprint).await?;
        sqlx::query("DELETE FROM graph_symbol_state WHERE symbol_id = ?")
            .bind(&id)
            .execute(pool)
            .await
            .map_err(db_err_pub)?;
        written += 1;
    }
    Ok(written)
}

async fn write_graph_episode(
    pool: &SqlitePool,
    symbol_id: &str,
    change: &str,
    name: &str,
    kind: &str,
    file_path: &str,
    fingerprint: &str,
) -> Result<(), AxError> {
    let memory_id = format!(
        "graph-{}",
        stable_id(&format!("{symbol_id}|{change}|{fingerprint}"))
    );
    store::remember_with_id(
        pool,
        memory_id,
        RememberInput {
            title: format!("{change} {kind} {name}"),
            body: format!(
                "{change} symbol {name} ({kind}) in {file_path}. Fingerprint {fingerprint}."
            ),
            kind: Some("graph_diff".into()),
            tags: vec![change.into(), name.into()],
            files: if file_path.is_empty() {
                vec![]
            } else {
                vec![file_path.into()]
            },
            source: Some("graph-sync".into()),
        },
    )
    .await?;
    Ok(())
}

pub async fn capture_activity_engrams(pool: &SqlitePool) -> Result<usize, AxError> {
    let exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'ax_activity_event'",
    )
    .fetch_one(pool)
    .await
    .map_err(db_err_pub)?;
    if exists == 0 {
        return Ok(0);
    }
    let rows: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT event_id, operation, summary, timestamp_ms FROM ax_activity_event
         WHERE operation IN ('memory','policy','ship','sync','remember','recall')
            OR operation LIKE 'memory%' OR operation LIKE 'policy%'
            OR operation LIKE 'ship%' OR operation LIKE 'sync%'
         ORDER BY timestamp_ms DESC LIMIT 100",
    )
    .fetch_all(pool)
    .await
    .map_err(db_err_pub)?;
    let mut written = 0usize;
    for (event_id, operation, summary, _at) in rows {
        if operation.contains("token") {
            continue;
        }
        let memory_id = format!("activity-{event_id}");
        let existing = store::get(pool, &memory_id).await?;
        if existing.is_some() {
            continue;
        }
        store::remember_with_id(
            pool,
            memory_id,
            RememberInput {
                title: format!("{operation}: {summary}"),
                body: summary,
                kind: Some("activity".into()),
                tags: vec![operation],
                files: vec![],
                source: Some("activity".into()),
            },
        )
        .await?;
        written += 1;
    }
    Ok(written)
}

pub async fn archive_history(
    pool: &SqlitePool,
    text: &str,
    since_ms: Option<i64>,
) -> Result<Vec<HistoryEntry>, AxError> {
    let request = SearchRequest {
        query: text.to_string(),
        depth: Depth::Archive,
        limit: 20,
        ..SearchRequest::default()
    };
    let matches = search(pool, &request).await?;
    let mut entries = Vec::new();
    for hit in matches {
        if since_ms.is_some_and(|since| hit.memory.created_at < since) {
            continue;
        }
        entries.push(HistoryEntry {
            at_ms: hit.memory.created_at,
            kind: hit.memory.kind,
            id: hit.memory.id,
            title: hit.memory.title,
            outcome: Some(hit.memory.body.chars().take(600).collect()),
            files: hit.memory.files,
            commits: vec![],
            request_id: None,
        });
    }
    Ok(entries)
}

fn stable_id(input: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// Rewrites rows left on an older embedding model, then indexes archive rows
/// that have no vector yet. Sync calls this after consolidation.
pub async fn reembed_stale(pool: &SqlitePool, limit: usize) -> Result<usize, AxError> {
    let limit = limit.max(1) as i64;
    let (_, active) = crate::embed::embed_labeled("ax");
    let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT id, title, body, COALESCE(tags, '[]'), COALESCE(tier, 't2')
         FROM memories WHERE embedding_model != ? AND enabled = 1 LIMIT ?",
    )
    .bind(active)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(db_err_pub)?;
    let mut updated = 0usize;
    for (id, title, body, tags, tier) in rows {
        let (vector, model) = crate::embed::embed_labeled(&format!("{title} {body} {tags}"));
        sqlx::query("UPDATE memories SET embedding = ?, embedding_model = ? WHERE id = ?")
            .bind(crate::embed::embedding_to_blob(&vector))
            .bind(model)
            .bind(&id)
            .execute(pool)
            .await
            .map_err(db_err_pub)?;
        if tier == TIER_ARCHIVE {
            index_archive_vector(pool, &id).await?;
        }
        updated += 1;
    }
    ensure_vec_table(pool).await?;
    let missing: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM memories
         WHERE tier = 't3' AND enabled = 1 AND embedding IS NOT NULL AND embedding_model = ?
           AND id NOT IN (SELECT memory_id FROM memory_vec_map)
         LIMIT ?",
    )
    .bind(active)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(db_err_pub)?;
    for id in missing {
        index_archive_vector(pool, &id).await?;
    }
    Ok(updated)
}

async fn symbol_names(pool: &SqlitePool, file: &str) -> Result<Vec<String>, AxError> {
    sqlx::query_scalar(
        "SELECT DISTINCT name FROM nodes
         WHERE file_path = ? OR file_path LIKE '%/' || ? OR ? LIKE '%/' || file_path",
    )
    .bind(file)
    .bind(file)
    .bind(file)
    .fetch_all(pool)
    .await
    .map_err(db_err_pub)
}

/// Memory ids whose cue set contains a symbol defined in one of the open files.
pub async fn memory_ids_for_symbols(
    pool: &SqlitePool,
    files: &[String],
) -> Result<std::collections::BTreeSet<String>, AxError> {
    let mut cues = Vec::new();
    for file in files {
        for name in symbol_names(pool, file).await? {
            cues.extend(automatic_cues(&name));
        }
    }
    cues.sort();
    cues.dedup();
    if cues.is_empty() {
        return Ok(std::collections::BTreeSet::new());
    }
    let json = serde_json::to_string(&cues).unwrap_or_else(|_| "[]".into());
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT memory_id FROM memory_cues WHERE cue IN (SELECT value FROM json_each(?))",
    )
    .bind(json)
    .fetch_all(pool)
    .await
    .map_err(db_err_pub)?;
    Ok(ids.into_iter().collect())
}

pub fn files_overlap(open: &[String], memory_files: &[String]) -> bool {
    open.iter().any(|file| {
        memory_files
            .iter()
            .any(|stored| stored == file || stored.ends_with(file) || file.ends_with(stored))
    })
}
