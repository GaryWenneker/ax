//! CRUD + FTS recall for the memory vault.

use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

use crate::types::{MemoryMatch, MemoryRow, RememberInput};

/// Confidence halves every 90 days unless the memory is updated.
const DECAY_HALF_LIFE_DAYS: f64 = 90.0;

pub(crate) fn db_err_at(path: Option<&std::path::Path>, e: impl std::fmt::Display) -> AxError {
    let message = e.to_string();
    if let Some(path) = path {
        // The SQL error is the result. Failing to write the incident file must not hide it.
        let _ = ax_db::integrity::note_sql_incident(path, &message);
    }
    AxError::Database(DatabaseError::new(message))
}

fn db_err(pool: &SqlitePool, e: impl std::fmt::Display) -> AxError {
    db_err_at(Some(pool.connect_options().get_filename()), e)
}

pub(crate) fn db_err_pub(pool: &SqlitePool, e: impl std::fmt::Display) -> AxError {
    db_err(pool, e)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Retry a locked database, and the one-shot SQLITE_SCHEMA error SQLite
/// returns after an FTS rebuild. The next statement on that connection works.
async fn retry_locked<T, F, Fut>(mut op: F) -> Result<T, sqlx::Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, sqlx::Error>>,
{
    const MAX_ATTEMPTS: u32 = 12;
    for attempt in 0..MAX_ATTEMPTS {
        match op().await {
            Ok(value) => return Ok(value),
            Err(err) if sqlite_retryable(&err) && attempt + 1 < MAX_ATTEMPTS => {
                let backoff = std::time::Duration::from_millis(40 * (attempt as u64 + 1).min(25));
                tokio::time::sleep(backoff).await;
            }
            Err(err) => return Err(err),
        }
    }
    unreachable!("loop exits via return")
}

fn sqlite_retryable(err: &sqlx::Error) -> bool {
    let msg = err.to_string();
    msg.contains("database is locked")
        || msg.contains("(code: 5)")
        || msg.contains("SQLITE_BUSY")
        || msg.contains("(code: 17)")
        || msg.contains("vtable constructor failed")
}

type MemoryDbRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    f64,
    String,
    i64,
    i64,
    i64,
    String,
);

fn row_from_db(
    (
        id,
        kind,
        title,
        body,
        tags,
        files,
        confidence,
        source,
        created_at,
        updated_at,
        enabled,
        tier,
    ): MemoryDbRow,
) -> MemoryRow {
    MemoryRow {
        id,
        kind,
        title,
        body,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        files: serde_json::from_str(&files).unwrap_or_default(),
        confidence,
        source,
        enabled: enabled != 0,
        created_at,
        updated_at,
        tier,
    }
}

const MEMORY_SELECT: &str = r#"SELECT id, kind, title, body, tags, files, confidence, source, created_at, updated_at, enabled, COALESCE(tier, 't2')
           FROM memories"#;

/// `turn` memories created before `before`, enabled or not, oldest first; at most 10,000 per call.
pub(crate) async fn expired_turn_rows(
    pool: &SqlitePool,
    before: i64,
) -> Result<Vec<MemoryRow>, AxError> {
    let rows = sqlx::query_as::<_, MemoryDbRow>(&format!(
        "{MEMORY_SELECT} WHERE kind = ? AND created_at < ? AND COALESCE(tier, 't1') != 't3' ORDER BY created_at, id LIMIT 10000"
    ))
    .bind(crate::turns::TURN_KIND)
    .bind(before)
    .fetch_all(pool)
    .await
    .map_err(|e| db_err(pool, e))?;
    Ok(rows.into_iter().map(row_from_db).collect())
}

pub(crate) async fn turn_rows_by_id(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<MemoryRow>, AxError> {
    let mut out = Vec::new();
    for id in ids {
        let row =
            sqlx::query_as::<_, MemoryDbRow>(&format!("{MEMORY_SELECT} WHERE id = ? AND kind = ?"))
                .bind(id)
                .bind(crate::turns::TURN_KIND)
                .fetch_optional(pool)
                .await
                .map_err(|e| db_err(pool, e))?;
        out.extend(row.map(row_from_db));
    }
    Ok(out)
}

/// Every `turn` memory, oldest first.
pub async fn turn_rows(pool: &SqlitePool) -> Result<Vec<MemoryRow>, AxError> {
    let rows = sqlx::query_as::<_, MemoryDbRow>(&format!(
        "{MEMORY_SELECT} WHERE kind = ? ORDER BY created_at, id"
    ))
    .bind(crate::turns::TURN_KIND)
    .fetch_all(pool)
    .await
    .map_err(|e| db_err(pool, e))?;
    Ok(rows.into_iter().map(row_from_db).collect())
}

/// `LIKE … ESCAPE '\'` pattern for paths ending in `/<suffix>`.
pub(crate) fn path_suffix_pattern(suffix: &str) -> String {
    let escaped = suffix
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%/{escaped}")
}

/// Enabled `turn` memories created at or after `since`, newest first, that have one of `ids`, or
/// changed a file in `files` or ending in `/<suffix>`. At most `limit` rows.
pub(crate) async fn matching_turn_rows(
    pool: &SqlitePool,
    since: Option<i64>,
    ids: &[String],
    files: &[String],
    suffix: Option<&str>,
    limit: usize,
) -> Result<Vec<MemoryRow>, AxError> {
    let as_json = |v: &[String]| serde_json::to_string(v).unwrap_or_else(|_| "[]".into());
    let suffix_like = suffix.map(path_suffix_pattern);
    let rows = sqlx::query_as::<_, MemoryDbRow>(&format!(
        "{MEMORY_SELECT} WHERE kind = ? AND enabled = 1 AND COALESCE(tier, 't1') != 't3' AND created_at >= ? \
         AND (id IN (SELECT value FROM json_each(?)) \
              OR EXISTS (SELECT 1 FROM json_each(memories.files) AS f \
                         WHERE f.value IN (SELECT value FROM json_each(?)) \
                            OR f.value LIKE ? ESCAPE '\\')) \
         ORDER BY created_at DESC, id LIMIT ?"
    ))
    .bind(crate::turns::TURN_KIND)
    .bind(since.unwrap_or(i64::MIN))
    .bind(as_json(ids))
    .bind(as_json(files))
    .bind(suffix_like)
    .bind(i64::try_from(limit).unwrap_or(i64::MAX))
    .fetch_all(pool)
    .await
    .map_err(|e| db_err(pool, e))?;
    Ok(rows.into_iter().map(row_from_db).collect())
}

/// Newest enabled turn memories, with no prompt or file filter.
pub(crate) async fn recent_turn_rows(
    pool: &SqlitePool,
    limit: usize,
) -> Result<Vec<MemoryRow>, AxError> {
    let rows = sqlx::query_as::<_, MemoryDbRow>(&format!(
        "{MEMORY_SELECT} WHERE kind = ? AND enabled = 1 AND COALESCE(tier, 't1') != 't3' ORDER BY created_at DESC, id LIMIT ?"
    ))
    .bind(crate::turns::TURN_KIND)
    .bind(i64::try_from(limit).unwrap_or(i64::MAX))
    .fetch_all(pool)
    .await
    .map_err(|e| db_err(pool, e))?;
    Ok(rows.into_iter().map(row_from_db).collect())
}

/// The enabled turn whose body has `Request: <id>`, newest first.
pub(crate) async fn turn_by_request_id(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<MemoryRow>, AxError> {
    let id = request_id.trim();
    if id.is_empty() || id.contains(['%', '_', '\n', '\r']) {
        return Ok(None);
    }
    let rows = sqlx::query_as::<_, MemoryDbRow>(&format!(
        "{MEMORY_SELECT} WHERE kind = ? AND enabled = 1 AND body LIKE ? \
         ORDER BY created_at DESC LIMIT 20"
    ))
    .bind(crate::turns::TURN_KIND)
    .bind(format!("%\nRequest: {id}%"))
    .fetch_all(pool)
    .await
    .map_err(|e| db_err(pool, e))?;
    Ok(rows
        .into_iter()
        .map(row_from_db)
        .find(|row| crate::turns::request_id_from_body(&row.body) == Some(id)))
}

/// Time-decayed confidence: recently touched memories rank higher.
pub fn effective_confidence(confidence: f64, updated_at_ms: i64, now_ms: i64) -> f64 {
    let age_days = ((now_ms - updated_at_ms).max(0) as f64) / 86_400_000.0;
    confidence * 0.5_f64.powf(age_days / DECAY_HALF_LIFE_DAYS)
}

pub async fn remember(pool: &SqlitePool, input: RememberInput) -> Result<MemoryRow, AxError> {
    remember_with_id(pool, uuid::Uuid::new_v4().to_string(), input).await
}

pub const MEMORY_LINK_KINDS: &[&str] = &[
    "in_session",
    "follows",
    "produced",
    "changed",
    "touches",
    "committed_as",
    "derived_from",
    "supersedes",
    "source",
];

pub async fn link_memories(
    pool: &SqlitePool,
    from_id: &str,
    to_id: &str,
    kind: &str,
) -> Result<(), AxError> {
    if !MEMORY_LINK_KINDS.contains(&kind) {
        return Err(AxError::Other(format!("unknown memory link kind: {kind}")));
    }
    sqlx::query("INSERT OR IGNORE INTO memory_links (from_id, to_id, kind) VALUES (?, ?, ?)")
        .bind(from_id)
        .bind(to_id)
        .bind(kind)
        .execute(pool)
        .await
        .map_err(|e| db_err(pool, e))?;
    Ok(())
}

pub(crate) async fn link_follows_in_session(pool: &SqlitePool, id: &str) -> Result<(), AxError> {
    let row: Option<(String, i64)> =
        sqlx::query_as("SELECT session_id, created_at FROM memories WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|e| db_err(pool, e))?;
    let Some((session, created_at)) = row else {
        return Ok(());
    };
    if session.trim().is_empty() {
        return Ok(());
    }
    let previous: Option<String> = sqlx::query_scalar(
        "SELECT id FROM memories
         WHERE session_id = ? AND id != ? AND created_at <= ?
         ORDER BY created_at DESC, id DESC
         LIMIT 1",
    )
    .bind(&session)
    .bind(id)
    .bind(created_at)
    .fetch_optional(pool)
    .await
    .map_err(|e| db_err(pool, e))?;
    let Some(previous) = previous else {
        return Ok(());
    };
    link_memories(pool, id, &previous, "follows").await
}

pub(crate) async fn stamp_current_turn(pool: &SqlitePool, id: &str) -> Result<(), AxError> {
    let path: Option<String> =
        sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name = 'main'")
            .fetch_optional(pool)
            .await
            .map_err(|e| db_err(pool, e))?;
    let Some(path) = path.filter(|text| !text.is_empty()) else {
        return Ok(());
    };
    let Some(current) = std::path::Path::new(&path)
        .parent()
        .map(|dir| dir.join("turns").join("current.json"))
    else {
        return Ok(());
    };
    let Ok(text) = std::fs::read_to_string(current) else {
        return Ok(());
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Ok(());
    };
    let session = value
        .get("session_id")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .trim();
    let turn = value
        .get("turn_id")
        .and_then(|item| item.as_str())
        .unwrap_or("")
        .trim();
    if session.is_empty() || turn.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "UPDATE memories SET session_id = ?, turn_id = ? WHERE id = ? AND session_id = '' AND turn_id = ''",
    )
    .bind(session)
    .bind(turn)
    .bind(id)
    .execute(pool)
    .await
    .map_err(|e| db_err(pool, e))?;
    Ok(())
}

pub(crate) async fn remember_with_id(
    pool: &SqlitePool,
    id: String,
    input: RememberInput,
) -> Result<MemoryRow, AxError> {
    let now = now_ms();
    let title = if input.title.trim().is_empty() {
        // First line of the body doubles as the title.
        input
            .body
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(120)
            .collect()
    } else {
        input.title.trim().to_string()
    };
    let kind = input.kind.unwrap_or_else(|| "note".into());
    let tier = crate::brain::tier_for_kind(&kind);
    let row = MemoryRow {
        id,
        kind,
        title,
        body: input.body,
        tags: input.tags,
        files: input.files,
        confidence: 1.0,
        source: input.source.unwrap_or_else(|| "manual".into()),
        enabled: true,
        created_at: now,
        updated_at: now,
        tier: tier.into(),
    };
    let (vector, model) = crate::embed::embed_labeled(&format!(
        "{} {} {}",
        row.title,
        row.body,
        row.tags.join(" ")
    ));
    let embedding = crate::embed::embedding_to_blob(&vector);
    let tags = serde_json::to_string(&row.tags).unwrap_or_else(|_| "[]".into());
    let files = serde_json::to_string(&row.files).unwrap_or_else(|_| "[]".into());
    let status = if crate::brain::memory_is_foreign(&row.source, &row.tags, &row.files) {
        "foreign"
    } else {
        "active"
    };
    retry_locked(|| {
        sqlx::query(
            r#"INSERT INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, embedding, enabled, status)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?)"#,
        )
        .bind(&row.id)
        .bind(&row.kind)
        .bind(&row.title)
        .bind(&row.body)
        .bind(&tags)
        .bind(&files)
        .bind(row.confidence)
        .bind(&row.source)
        .bind(row.created_at)
        .bind(row.updated_at)
        .bind(embedding.clone())
        .bind(status)
        .execute(pool)
    })
    .await
    .map_err(|e| db_err(pool, e))?;
    let origin = crate::provenance::origin_for(&row.source);
    let trust = crate::provenance::trust_level_for(&row.source, &row.kind);
    let search = crate::search_text::normalize_search_text(&format!(
        "{} {} {} {}",
        row.title,
        row.body,
        row.tags.join(" "),
        row.files.join(" ")
    ));
    let instruction =
        crate::instruction_like::is_instruction_like(&format!("{} {}", row.title, row.body));
    sqlx::query(
        "UPDATE memories SET origin = ?, trust_level = ?, search_text = ?, instruction_like = ? WHERE id = ?",
    )
    .bind(&origin)
    .bind(trust)
    .bind(&search)
    .bind(if instruction { 1 } else { 0 })
    .bind(&row.id)
    .execute(pool)
    .await
    .map_err(|e| db_err(pool, e))?;
    stamp_current_turn(pool, &row.id).await?;
    crate::brain::stamp_memory(pool, &row.id, &row.kind, model).await?;
    crate::brain::register_cues(pool, &row, &[]).await?;
    let _ = crate::anchors::auto_attach_memory(pool, &row.id).await;
    Ok(row)
}

/// Sanitize free text into an FTS5 OR-query of quoted tokens.
/// Returns `None` when there is nothing searchable.
pub fn fts_query_from_text(text: &str) -> Option<String> {
    fts_query_from_text_mode(text, false)
}

/// Archive queries prefer AND of the raw tokens so common-word OR posting lists
/// do not force a linear pass over a million-row FTS index.
fn fts_query_from_text_mode(text: &str, archive: bool) -> Option<String> {
    const STOP_WORDS: &[&str] = &[
        "the", "a", "an", "and", "or", "of", "in", "on", "to", "for", "with", "is", "are", "de",
        "het", "een", "en", "van", "voor", "met", "dat", "die", "je", "ik",
    ];
    let raw: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .map(str::trim)
        .filter(|t| t.len() >= 3)
        .filter(|t| !STOP_WORDS.contains(&t.to_ascii_lowercase().as_str()))
        .map(|t| t.to_ascii_lowercase())
        .collect();
    let mut tokens: Vec<String> = if archive {
        raw.into_iter()
            .map(|t| format!("\"{}\"", t.replace('"', "")))
            .collect()
    } else {
        raw.into_iter()
            .flat_map(|t| crate::lexicon::expand_token(&t))
            .map(|t| format!("\"{}\"", t.replace('"', "")))
            .collect()
    };
    tokens.sort();
    tokens.dedup();
    tokens.truncate(24);
    if tokens.is_empty() {
        None
    } else if archive && tokens.len() >= 2 {
        Some(tokens.join(" AND "))
    } else {
        Some(tokens.join(" OR "))
    }
}

/// Hybrid recall: FTS5 (BM25) and vector similarity fused with Reciprocal
/// Rank Fusion, then weighted by confidence decay.
pub async fn recall(
    pool: &SqlitePool,
    query: &str,
    limit: usize,
) -> Result<Vec<MemoryMatch>, AxError> {
    recall_scoped(pool, query, limit, &[], false).await
}

pub async fn recall_filtered(
    pool: &SqlitePool,
    query: &str,
    limit: usize,
    filter: &crate::types::MemoryFilter,
) -> Result<Vec<MemoryMatch>, AxError> {
    let keyword = filter
        .keyword
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty());
    let combined;
    let effective = match keyword {
        Some(text) if query.trim().is_empty() => text,
        Some(text) => {
            combined = format!("{query} {text}");
            combined.as_str()
        }
        None => query,
    };
    let files = filter
        .file
        .as_ref()
        .map(|path| vec![path.clone()])
        .unwrap_or_default();
    let symbols = filter
        .symbol
        .as_ref()
        .map(|name| vec![name.clone()])
        .unwrap_or_default();
    let graph = if files.is_empty() { &symbols } else { &files };
    let overfetch = if filter_is_active(filter) {
        (limit * 8).max(40)
    } else {
        limit
    };
    let mut matches = recall_scoped(pool, effective, overfetch, graph, false).await?;
    retain_filtered(pool, &mut matches, filter).await?;
    matches.truncate(limit);
    Ok(matches)
}

fn filter_is_active(filter: &crate::types::MemoryFilter) -> bool {
    filter.session.is_some()
        || filter.turn.is_some()
        || filter.trigger.is_some()
        || filter.kind.is_some()
        || filter.since.is_some()
        || filter.until.is_some()
        || filter.file.is_some()
        || filter.symbol.is_some()
}

pub async fn apply_memory_filter(
    pool: &SqlitePool,
    matches: &mut Vec<MemoryMatch>,
    filter: &crate::types::MemoryFilter,
) -> Result<(), AxError> {
    retain_filtered(pool, matches, filter).await
}

async fn retain_filtered(
    pool: &SqlitePool,
    matches: &mut Vec<MemoryMatch>,
    filter: &crate::types::MemoryFilter,
) -> Result<(), AxError> {
    if matches.is_empty() || !filter_is_active(filter) {
        return Ok(());
    }
    let ids: Vec<String> = matches.iter().map(|item| item.memory.id.clone()).collect();
    let mut kept = std::collections::HashSet::new();
    for id in &ids {
        let row: Option<(String, String, String, String, String, i64)> = sqlx::query_as(
            "SELECT id, session_id, turn_id, kind, files, created_at FROM memories WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| db_err(pool, e))?;
        let Some((id, session, turn, kind, files_json, created_at)) = row else {
            continue;
        };
        if let Some(want) = filter
            .session
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if session != want {
                continue;
            }
        }
        if let Some(want) = filter
            .turn
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if turn != want {
                continue;
            }
        }
        if let Some(want) = filter
            .kind
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if kind != want {
                continue;
            }
        }
        if let Some(since) = filter.since {
            if created_at < since {
                continue;
            }
        }
        if let Some(until) = filter.until {
            if created_at > until {
                continue;
            }
        }
        if let Some(want) = filter
            .file
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let files: Vec<String> = serde_json::from_str(&files_json).unwrap_or_default();
            if !files.iter().any(|path| path == want || path.contains(want)) {
                continue;
            }
        }
        if let Some(want) = filter
            .trigger
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let hit: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM memory_cues WHERE memory_id = ? AND cue = ? AND source = 'trigger' LIMIT 1",
            )
            .bind(&id)
            .bind(want)
            .fetch_optional(pool)
            .await
            .map_err(|e| db_err(pool, e))?;
            if hit.is_none() {
                continue;
            }
        }
        kept.insert(id);
    }
    matches.retain(|item| kept.contains(&item.memory.id));
    Ok(())
}

async fn hot_memory_count(pool: &SqlitePool) -> Result<i64, AxError> {
    // `tier IN (...)` keeps idx_memories_tier usable; COALESCE(tier, …) != 't3' does not.
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM memories
         WHERE enabled = 1
           AND tier IN ('t1', 't2')
           AND COALESCE(status, 'active') = 'active'",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| db_err(pool, e))
}

async fn fts_ranking(
    pool: &SqlitePool,
    query: &str,
    overfetch: usize,
    archive: bool,
) -> Result<Vec<String>, AxError> {
    let Some(fts_query) = fts_query_from_text_mode(query, archive) else {
        return Ok(Vec::new());
    };
    // Hot path: constrain FTS5 to hot rowids first. Without that, MATCH walks
    // archive postings for common terms and only then drops every t3 hit.
    if !archive {
        // Include `foreign` and `active` for explicit recall. Preflight still
        // filters via `is_injectable` (active only). Candidates stay out.
        let rowids: Vec<i64> = sqlx::query_scalar(
            "SELECT rowid FROM memories
             WHERE enabled = 1
               AND tier IN ('t1', 't2')
               AND COALESCE(status, 'active') IN ('active', 'foreign')
             ORDER BY rowid
             LIMIT 20000",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| db_err(pool, e))?;
        if rowids.is_empty() {
            return Ok(Vec::new());
        }
        if rowids.len() < 20_000 {
            let list = rowids
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(",");
            // Hot inject must not spend its LIMIT on turns/graph_diff/docs — those
            // kinds are recall-only and would crowd durable memories out of overfetch.
            let sql = format!(
                r#"SELECT m.id
                   FROM memories_fts
                   JOIN memories m ON m.rowid = memories_fts.rowid
                   WHERE memories_fts MATCH ?
                     AND memories_fts.rowid IN ({list})
                     AND m.enabled = 1
                     AND m.kind NOT IN ('turn', 'doc', 'graph_diff', 'activity', 'file_change')
                     AND m.title NOT LIKE 'Candidate:%'
                   ORDER BY bm25(memories_fts, 0.0, 3.0, 1.0, 1.5, 2.0), m.id
                   LIMIT ?"#
            );
            return sqlx::query_scalar::<_, String>(&sql)
                .bind(&fts_query)
                .bind(overfetch as i64)
                .fetch_all(pool)
                .await
                .map_err(|e| db_err(pool, e));
        }
    }
    let sql = if archive {
        r#"SELECT m.id
           FROM memories_fts
           JOIN memories m ON m.rowid = memories_fts.rowid
           WHERE memories_fts MATCH ? AND m.enabled = 1 AND m.tier = 't3'
           ORDER BY bm25(memories_fts, 0.0, 3.0, 1.0, 1.5, 2.0), m.id
           LIMIT ?"#
    } else {
        r#"SELECT m.id
           FROM memories_fts
           JOIN memories m ON m.rowid = memories_fts.rowid
           WHERE memories_fts MATCH ?
             AND m.enabled = 1
             AND m.tier IN ('t1', 't2')
             AND COALESCE(m.status, 'active') IN ('active', 'foreign')
             AND m.kind NOT IN ('turn', 'doc', 'graph_diff', 'activity', 'file_change')
             AND m.title NOT LIKE 'Candidate:%'
           ORDER BY bm25(memories_fts, 0.0, 3.0, 1.0, 1.5, 2.0), m.id
           LIMIT ?"#
    };
    sqlx::query_scalar::<_, String>(sql)
        .bind(&fts_query)
        .bind(overfetch as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| db_err(pool, e))
}

async fn vector_ranking(
    pool: &SqlitePool,
    query: &str,
    overfetch: usize,
    archive: bool,
) -> Result<Vec<String>, AxError> {
    if archive {
        return crate::brain::knn(pool, query, overfetch).await;
    }
    let (query_embedding, _) = crate::embed::embed_labeled(query);
    // Match FTS: foreign rows stay searchable for explicit recall; preflight
    // still gates inject via `is_injectable` (active only).
    let embedding_rows = sqlx::query_as::<_, (String, Vec<u8>)>(
        "SELECT id, embedding FROM memories
         WHERE embedding IS NOT NULL AND enabled = 1
           AND tier IN ('t1', 't2')
           AND COALESCE(status, 'active') IN ('active', 'foreign')
           AND kind NOT IN ('turn', 'doc', 'graph_diff', 'activity', 'file_change')
           AND title NOT LIKE 'Candidate:%'",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| db_err(pool, e))?;
    let mut vector_scored: Vec<(String, f32)> = embedding_rows
        .into_iter()
        .filter_map(|(id, blob)| {
            let e = crate::embed::blob_to_embedding(&blob)?;
            Some((id, crate::embed::cosine(&query_embedding, &e)))
        })
        .filter(|(_, sim)| *sim > crate::embed::MIN_VECTOR_SIMILARITY)
        .collect();
    vector_scored.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    vector_scored.truncate(overfetch);
    Ok(vector_scored.into_iter().map(|(id, _)| id).collect())
}

/// Hot recall skips the archive and inactive rows. Archive recall uses FTS
/// plus sqlite-vec KNN. Keyword search and a parseable hot vector are not
/// hidden by the stored model name.
pub(crate) async fn recall_scoped(
    pool: &SqlitePool,
    query: &str,
    limit: usize,
    files: &[String],
    archive: bool,
) -> Result<Vec<MemoryMatch>, AxError> {
    use std::collections::HashMap;

    let overfetch = (limit * 4).max(20);
    let profile = std::env::var_os("AX_MEMORY_PROFILE").is_some();
    let t0 = std::time::Instant::now();
    let expanded = if archive {
        query.to_string()
    } else {
        // PMI samples up to 2000 rows; skip it when the hot set is already tiny.
        let hot = hot_memory_count(pool).await.unwrap_or(0);
        if hot <= 64 {
            query.to_string()
        } else {
            let pmi_extra = crate::pmi::expand_with_pmi(pool, query, 3)
                .await
                .unwrap_or_default();
            if pmi_extra.is_empty() {
                query.to_string()
            } else {
                format!("{query} {}", pmi_extra.join(" "))
            }
        }
    };
    let t_expand = t0.elapsed();
    // Archive: prefer FTS (bounded by LIMIT) and only run global KNN when FTS
    // finds nothing — full-corpus vec0 MATCH is linear in archive size.
    let t1 = std::time::Instant::now();
    let (fts_ranked, vector_ranked) = if archive {
        let fts = fts_ranking(pool, &expanded, overfetch, true).await?;
        let vec = if fts.is_empty() {
            vector_ranking(pool, query, overfetch, true).await?
        } else {
            Vec::new()
        };
        (fts, vec)
    } else if std::env::var_os("AX_MEMORY_BENCH_FTS_ONLY").is_some() {
        // Public-set evidence compare: FTS/BM25F only, no vector RRF dilution.
        let fts = fts_ranking(pool, &expanded, overfetch, false).await?;
        (fts, Vec::new())
    } else {
        tokio::try_join!(
            fts_ranking(pool, &expanded, overfetch, false),
            vector_ranking(pool, query, overfetch, false),
        )?
    };
    let t_rank = t1.elapsed();
    if profile {
        eprintln!(
            "recall_scoped archive={archive} expand={t_expand:?} rank={t_rank:?} fts={} vec={}",
            fts_ranked.len(),
            vector_ranked.len()
        );
    }

    if fts_ranked.is_empty() && vector_ranked.is_empty() {
        return Ok(Vec::new());
    }

    let mut fused: HashMap<String, f64> = HashMap::new();
    for (rank, id) in fts_ranked.iter().enumerate() {
        *fused.entry(id.clone()).or_default() +=
            crate::embed::WEIGHT_FTS * crate::embed::rrf_score(rank);
    }
    for (rank, id) in vector_ranked.iter().enumerate() {
        *fused.entry(id.clone()).or_default() +=
            crate::embed::WEIGHT_VECTOR * crate::embed::rrf_score(rank);
    }

    let ids: Vec<String> = fused.keys().cloned().collect();
    let symbol_hits = if files.is_empty() {
        std::collections::BTreeSet::new()
    } else {
        crate::brain::memory_ids_for_symbols(pool, files).await?
    };
    let now = now_ms();
    let ppr_boost: std::collections::HashMap<String, f64> = if archive {
        std::collections::HashMap::new()
    } else {
        let mut edges: Vec<(String, String)> = Vec::new();
        let mut seeds: Vec<String> = Vec::new();
        if !files.is_empty() {
            for path in files.iter().take(8) {
                if let Ok(ids) = sqlx::query_scalar::<_, String>(
                    "SELECT id FROM memories WHERE enabled = 1 AND files LIKE ? LIMIT 32",
                )
                .bind(format!("%{path}%"))
                .fetch_all(pool)
                .await
                {
                    seeds.extend(ids);
                }
            }
            seeds.sort();
            seeds.dedup();
            if let Ok(rows) = sqlx::query_as::<_, (String, String)>(
                "SELECT DISTINCT a.memory_id, b.memory_id
                 FROM memory_anchors a
                 JOIN memory_anchors b ON a.node_key = b.node_key AND a.memory_id < b.memory_id
                 LIMIT 2000",
            )
            .fetch_all(pool)
            .await
            {
                edges = rows;
            }
        }
        crate::pagerank::personalized_pagerank(&seeds, &edges, 0.85, 8)
    };
    let query_terms: Vec<String> = query_terms(query);
    let mut matches = Vec::with_capacity(ids.len());
    for id in &ids {
        if let Some(memory) = get(pool, id).await? {
            // The archive exists for old memories, so age must not rank them down there.
            // Same-day memories share one decay bucket so insert-order milliseconds
            // cannot reshuffle otherwise-equal hybrid ranks.
            let same_day = now.saturating_sub(memory.updated_at) < 86_400_000;
            let decayed = if archive || same_day {
                memory.confidence
            } else {
                effective_confidence(memory.confidence, memory.updated_at, now)
            };
            let mut score = fused.get(id).copied().unwrap_or(0.0) * decayed.max(0.05) * 100.0;
            let file_hit = crate::brain::files_overlap(files, &memory.files);
            let symbol_hit = memory.tier == crate::brain::TIER_SEMANTIC && symbol_hits.contains(id);
            if file_hit || symbol_hit {
                score += crate::embed::WEIGHT_GRAPH * 100.0;
            }
            let act = if archive {
                0.0
            } else {
                let access = crate::access::access_times(pool, id)
                    .await
                    .unwrap_or_default();
                let act_raw = crate::act_r::base_level_activation(now, &access, 0.5);
                if act_raw.is_finite() {
                    act_raw.max(0.0)
                } else {
                    0.0
                }
            };
            if act > 0.0 {
                score *= 1.0 + crate::fusion::W_ACT_R * act.min(1.0);
            }
            if let Some(pr) = ppr_boost.get(id) {
                if *pr > 0.0 {
                    score *= 1.0 + crate::fusion::W_GRAPH * pr.min(1.0);
                }
            }
            let why = if archive {
                None
            } else {
                let matched: Vec<String> = query_terms
                    .iter()
                    .filter(|t| {
                        let text = format!("{} {}", memory.title, memory.body).to_lowercase();
                        text.contains(t.as_str())
                    })
                    .cloned()
                    .collect();
                Some(crate::fusion::why_from_hits(
                    &matched, file_hit, symbol_hit, act,
                ))
            };
            matches.push(MemoryMatch { memory, score, why });
        }
    }
    // Coarse score buckets + id: tiny decay/float drift must not reshuffle ties.
    matches.sort_by(|a, b| {
        let sa = (a.score * 100.0).round() as i64;
        let sb = (b.score * 100.0).round() as i64;
        sb.cmp(&sa).then_with(|| a.memory.id.cmp(&b.memory.id))
    });
    // Public-set evidence benches set AX_MEMORY_BENCH_RAW so ax's FTS/BM25F
    // ranking can be compared to plain BM25 without the inject abstention floor.
    if !archive && std::env::var_os("AX_MEMORY_BENCH_RAW").is_none() {
        drop_weak_term_matches(&mut matches, query);
    }
    matches.truncate(limit);
    Ok(matches)
}

/// Keep only hits whose term coverage is close to the best hit. A looser floor
/// (0.6) let near-miss siblings pad top-3 and sink precision@3 on live vaults.
const MIN_RELATIVE_COVERAGE: f64 = 0.85;
/// Below this best-term coverage, hot recall abstains entirely (vector nearest
/// neighbors alone are not enough when the prompt barely overlaps the vault).
const MIN_ABSOLUTE_COVERAGE: f64 = 0.25;
/// Long prompts that only share a single content token with a memory are
/// treated as irrelevant for preflight inject (e.g. incidental `404`).
const MIN_ABSOLUTE_HITS_FOR_LONG_QUERY: usize = 2;
const LONG_QUERY_TERM_COUNT: usize = 4;

const COVERAGE_STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "from", "into", "using", "only", "how", "are",
    "was", "were", "been", "have", "has", "had", "will", "can", "may", "not", "but", "its", "you",
    "your", "our", "all", "any", "few",
];

fn query_terms(query: &str) -> Vec<String> {
    let mut terms: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() >= 3)
        .map(str::to_lowercase)
        .collect();
    terms.sort();
    terms.dedup();
    terms
}

fn coverage_terms(query: &str) -> Vec<String> {
    let mut terms: Vec<String> = query_terms(query)
        .into_iter()
        .filter(|t| {
            if COVERAGE_STOPWORDS.contains(&t.as_str()) {
                return false;
            }
            // Pure numerics (`404`) match too many incidental status codes.
            if t.chars().all(|c| c.is_ascii_digit()) {
                return false;
            }
            true
        })
        .collect();
    terms.sort();
    terms.dedup();
    terms
}

/// "dropdowns" covers a memory that says "dropdown".
fn singular(term: &str) -> Option<&str> {
    term.strip_suffix('s').filter(|s| s.chars().count() >= 3)
}

fn term_hit_count(terms: &[String], memory: &MemoryRow) -> usize {
    let stem = std::env::var_os("AX_MEMORY_BENCH_IDENTIFIER_STEM").is_some();
    let boundary = !stem && std::env::var_os("AX_MEMORY_BENCH_IDENTIFIER_BOUNDARY").is_some();
    let affix =
        !stem && !boundary && std::env::var_os("AX_MEMORY_BENCH_IDENTIFIER_AFFIX").is_some();
    term_hits(terms, memory, stem, boundary, affix)
}

fn term_hits(
    terms: &[String],
    memory: &MemoryRow,
    stem: bool,
    boundary: bool,
    affix: bool,
) -> usize {
    let raw = format!("{} {}", memory.title, memory.body);
    let text = raw.to_lowercase();
    let words: Vec<String> = if stem {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|word| word.chars().count() >= 3)
            .map(str::to_string)
            .collect()
    } else {
        Vec::new()
    };
    let raw_tokens: Vec<String> = if boundary || affix {
        raw.split(|c: char| {
            c.is_whitespace() || matches!(c, '/' | '\\' | '.' | ':' | ',' | ';' | '|' | '(' | ')')
        })
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
    } else {
        Vec::new()
    };
    terms
        .iter()
        .filter(|term| {
            text.contains(term.as_str())
                || singular(term).is_some_and(|piece| text.contains(piece))
                || (stem && words.iter().any(|word| shares_identifier_stem(term, word)))
                || (boundary
                    && raw_tokens.iter().any(|token| {
                        (identifier_shaped(term) || identifier_shaped(token))
                            && shares_identifier_stem(term, token)
                    }))
                || (affix
                    && raw_tokens.iter().any(|token| {
                        (identifier_shaped(term) || identifier_shaped(token))
                            && shares_identifier_affix(term, token)
                    }))
        })
        .count()
}

/// Content-token overlap of a query against one memory. `hits` is how many
/// coverage terms occur in the title or body. `terms` is the query's coverage
/// term count after stopwords and pure numerics are removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentOverlap {
    pub hits: usize,
    pub terms: usize,
}

pub fn content_overlap(query: &str, title: &str, body: &str) -> ContentOverlap {
    let terms = coverage_terms(query);
    let memory = MemoryRow {
        id: String::new(),
        kind: String::new(),
        title: title.to_string(),
        body: body.to_string(),
        tags: Vec::new(),
        files: Vec::new(),
        confidence: 0.0,
        source: String::new(),
        enabled: true,
        created_at: 0,
        updated_at: 0,
        tier: String::new(),
    };
    ContentOverlap {
        hits: term_hit_count(&terms, &memory),
        terms: terms.len(),
    }
}

fn normalized_identifier(token: &str) -> String {
    token
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn longest_common_substring_len(left: &str, right: &str) -> usize {
    let a = left.as_bytes();
    let b = right.as_bytes();
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    let mut best = 0usize;
    let mut prev = vec![0usize; b.len() + 1];
    let mut curr = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            if a[i - 1] == b[j - 1] {
                curr[j] = prev[j - 1] + 1;
                if curr[j] > best {
                    best = curr[j];
                }
            } else {
                curr[j] = 0;
            }
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.fill(0);
    }
    best
}

fn identifier_shaped(token: &str) -> bool {
    token.contains('_')
        || token.contains('-')
        || token.chars().skip(1).any(|c| c.is_ascii_uppercase())
}

/// Stem match only when the query token or the raw memory token has an
/// identifier boundary (`_`, `-`, or an internal capital). Plain words stay
/// on whole-token overlap. Not used by production recall.
pub fn identifier_boundary_hits(query: &str, title: &str, body: &str) -> usize {
    let terms = coverage_terms(query);
    let memory = MemoryRow {
        id: String::new(),
        kind: String::new(),
        title: title.to_string(),
        body: body.to_string(),
        tags: Vec::new(),
        files: Vec::new(),
        confidence: 0.0,
        source: String::new(),
        enabled: true,
        created_at: 0,
        updated_at: 0,
        tier: String::new(),
    };
    term_hits(&terms, &memory, false, true, false)
}

fn shares_identifier_affix(query_term: &str, memory_word: &str) -> bool {
    let left = normalized_identifier(query_term);
    let right = normalized_identifier(memory_word);
    if left.len() < 4 || right.len() < 4 {
        return false;
    }
    let prefix = left
        .bytes()
        .zip(right.bytes())
        .take_while(|(left_byte, right_byte)| left_byte == right_byte)
        .count();
    let suffix = left
        .bytes()
        .rev()
        .zip(right.bytes().rev())
        .take_while(|(left_byte, right_byte)| left_byte == right_byte)
        .count();
    let shared = prefix.max(suffix);
    let shorter = left.len().min(right.len());
    shared >= 4 && shared * 2 >= shorter
}

/// Shared prefix or suffix only, and only for an identifier-shaped token.
/// Production recall uses it only when `AX_MEMORY_BENCH_IDENTIFIER_AFFIX` is set.
pub fn identifier_affix_hits(query: &str, title: &str, body: &str) -> usize {
    let terms = coverage_terms(query);
    let memory = MemoryRow {
        id: String::new(),
        kind: String::new(),
        title: title.to_string(),
        body: body.to_string(),
        tags: Vec::new(),
        files: Vec::new(),
        confidence: 0.0,
        source: String::new(),
        enabled: true,
        created_at: 0,
        updated_at: 0,
        tier: String::new(),
    };
    term_hits(&terms, &memory, false, false, true)
}

fn shares_identifier_stem(query_term: &str, memory_word: &str) -> bool {
    let left = normalized_identifier(query_term);
    let right = normalized_identifier(memory_word);
    if left.len() < 4 || right.len() < 4 {
        return false;
    }
    let shared = longest_common_substring_len(&left, &right);
    let shorter = left.len().min(right.len());
    shared >= 4 && shared * 2 >= shorter
}

/// Experimental stem overlap. Production recall uses it only when
/// `AX_MEMORY_BENCH_IDENTIFIER_STEM` is set.
/// A shared piece must be at least 4 characters, one longer than the
/// tokenizer minimum of 3, and at least half the shorter identifier.
pub fn identifier_stem_hits(query: &str, title: &str, body: &str) -> usize {
    let terms = coverage_terms(query);
    let memory = MemoryRow {
        id: String::new(),
        kind: String::new(),
        title: title.to_string(),
        body: body.to_string(),
        tags: Vec::new(),
        files: Vec::new(),
        confidence: 0.0,
        source: String::new(),
        enabled: true,
        created_at: 0,
        updated_at: 0,
        tier: String::new(),
    };
    term_hits(&terms, &memory, true, false, false)
}

/// Rank fusion scores rank 2 almost like rank 1, so a hit sharing one word with the query
/// survives next to a hit that matches all of them. Keep hits close to the best coverage.
/// Coverage is substring-based, so a typo like "reinstal" still covers "reinstall"; a best
/// coverage of zero (or below [`MIN_ABSOLUTE_COVERAGE`]) means nothing matches and the
/// vector leg only found noise — abstain.
fn drop_weak_term_matches(matches: &mut Vec<MemoryMatch>, query: &str) {
    let relative = std::env::var_os("AX_MEMORY_BENCH_SKIP_RELATIVE_COVERAGE").is_none();
    drop_weak_term_matches_with_relative(matches, query, relative);
}

fn drop_weak_term_matches_with_relative(
    matches: &mut Vec<MemoryMatch>,
    query: &str,
    relative: bool,
) {
    let terms = coverage_terms(query);
    if terms.is_empty() {
        matches.clear();
        return;
    }
    let hits: Vec<usize> = matches
        .iter()
        .map(|m| term_hit_count(&terms, &m.memory))
        .collect();
    let coverage: Vec<f64> = hits
        .iter()
        .map(|h| *h as f64 / terms.len() as f64)
        .collect();
    let best = coverage.iter().copied().fold(0.0, f64::max);
    let best_hits = hits.iter().copied().max().unwrap_or(0);
    if best == 0.0 || best < MIN_ABSOLUTE_COVERAGE {
        matches.clear();
        return;
    }
    if terms.len() >= LONG_QUERY_TERM_COUNT && best_hits < MIN_ABSOLUTE_HITS_FOR_LONG_QUERY {
        matches.clear();
        return;
    }
    if !relative {
        return;
    }
    let mut index = 0;
    matches.retain(|_| {
        let keep = coverage[index] >= best * MIN_RELATIVE_COVERAGE;
        index += 1;
        keep
    });
}

pub async fn list(
    pool: &SqlitePool,
    limit: usize,
    offset: usize,
) -> Result<(Vec<MemoryRow>, i64), AxError> {
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories")
        .fetch_one(pool)
        .await
        .map_err(|e| db_err(pool, e))?;
    let rows = sqlx::query_as::<_, MemoryDbRow>(&format!(
        "{MEMORY_SELECT} ORDER BY updated_at DESC LIMIT ? OFFSET ?"
    ))
    .bind(limit as i64)
    .bind(offset as i64)
    .fetch_all(pool)
    .await
    .map_err(|e| db_err(pool, e))?;
    let memories = rows.into_iter().map(row_from_db).collect();
    Ok((memories, total))
}

/// Lightweight title catalog for stable wiki-link disambiguation. Bodies, tags
/// and file lists are fetched by ID only for selected targets.
pub async fn link_catalog(pool: &SqlitePool) -> Result<Vec<MemoryRow>, AxError> {
    let rows: Vec<(String, String, String, i64, i64, i64, String)> = sqlx::query_as(
        "SELECT id, kind, title, created_at, updated_at, enabled, COALESCE(tier, 't2') FROM memories ORDER BY created_at, id"
    ).fetch_all(pool).await.map_err(|e| db_err(pool, e))?;
    Ok(rows
        .into_iter()
        .map(
            |(id, kind, title, created_at, updated_at, enabled, tier)| MemoryRow {
                id,
                kind,
                title,
                body: String::new(),
                tags: vec![],
                files: vec![],
                confidence: 1.0,
                source: String::new(),
                enabled: enabled != 0,
                created_at,
                updated_at,
                tier,
            },
        )
        .collect())
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<MemoryRow>, AxError> {
    let row = sqlx::query_as::<_, MemoryDbRow>(&format!("{MEMORY_SELECT} WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| db_err(pool, e))?;
    Ok(row.map(row_from_db))
}

pub async fn set_enabled(pool: &SqlitePool, id: &str, enabled: bool) -> Result<bool, AxError> {
    let result = sqlx::query("UPDATE memories SET enabled = ? WHERE id = ?")
        .bind(if enabled { 1 } else { 0 })
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| db_err(pool, e))?;
    Ok(result.rows_affected() > 0)
}

pub async fn update(pool: &SqlitePool, id: &str, input: RememberInput) -> Result<bool, AxError> {
    let (vector, model) = crate::embed::embed_labeled(&format!(
        "{} {} {}",
        input.title,
        input.body,
        input.tags.join(" ")
    ));
    let embedding = crate::embed::embedding_to_blob(&vector);
    let title = input.title.trim().to_string();
    let kind = input.kind.unwrap_or_default();
    let tags = serde_json::to_string(&input.tags).unwrap_or_else(|_| "[]".into());
    let files = serde_json::to_string(&input.files).unwrap_or_else(|_| "[]".into());
    let updated_at = now_ms();
    let result = retry_locked(|| {
        sqlx::query(
            r#"UPDATE memories
               SET title = ?, body = ?, kind = COALESCE(NULLIF(?, ''), kind),
                   tags = ?, files = ?, confidence = 1.0, updated_at = ?,
                   embedding = ?, embedding_model = ?
               WHERE id = ?"#,
        )
        .bind(&title)
        .bind(&input.body)
        .bind(&kind)
        .bind(&tags)
        .bind(&files)
        .bind(updated_at)
        .bind(embedding.clone())
        .bind(model)
        .bind(id)
        .execute(pool)
    })
    .await
    .map_err(|e| db_err(pool, e))?;
    if result.rows_affected() > 0 {
        if let Some(row) = get(pool, id).await? {
            crate::brain::register_cues(pool, &row, &[]).await?;
            if row.tier == crate::brain::TIER_ARCHIVE {
                crate::brain::index_archive_vector(pool, id).await?;
            }
        }
    }
    Ok(result.rows_affected() > 0)
}

/// Memories whose embedding is very close to `text` — used to flag possible
/// duplicates or contradictions when saving new knowledge.
pub async fn find_similar(
    pool: &SqlitePool,
    text: &str,
    exclude_id: Option<&str>,
    threshold: f32,
    limit: usize,
) -> Result<Vec<MemoryMatch>, AxError> {
    let (query_embedding, _) = crate::embed::embed_labeled(text);
    let rows = sqlx::query_as::<_, (String, Vec<u8>)>(
        "SELECT id, embedding FROM memories
         WHERE embedding IS NOT NULL AND enabled = 1
           AND COALESCE(tier, 't2') != 't3'",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| db_err(pool, e))?;

    let mut scored: Vec<(String, f32)> = rows
        .into_iter()
        .filter(|(id, _)| exclude_id != Some(id.as_str()))
        .filter_map(|(id, blob)| {
            let e = crate::embed::blob_to_embedding(&blob)?;
            Some((id, crate::embed::cosine(&query_embedding, &e)))
        })
        .filter(|(_, sim)| *sim >= threshold)
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.truncate(limit);

    let mut out = Vec::with_capacity(scored.len());
    for (id, sim) in scored {
        if let Some(memory) = get(pool, &id).await? {
            out.push(MemoryMatch {
                memory,
                score: sim as f64,
                why: None,
            });
        }
    }
    Ok(out)
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<bool, AxError> {
    let result = retry_locked(|| {
        sqlx::query("DELETE FROM memories WHERE id = ?")
            .bind(id)
            .execute(pool)
    })
    .await
    .map_err(|e| db_err(pool, e))?;
    Ok(result.rows_affected() > 0)
}

/// Remove a memory and every index row that points at it, then tombstone the id
/// so import/restore/repair/sync cannot bring it back.
pub async fn purge_memory(pool: &SqlitePool, id: &str) -> Result<bool, AxError> {
    let id = id.trim();
    if id.is_empty() {
        return Ok(false);
    }
    let existing: Option<String> = sqlx::query_scalar("SELECT id FROM memories WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| db_err(pool, e))?;
    if existing.is_none() {
        return Ok(false);
    }

    let vec_rowid: Option<i64> =
        match sqlx::query_scalar("SELECT rowid FROM memory_vec_map WHERE memory_id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
        {
            Ok(row) => row,
            Err(error) if error.to_string().contains("no such table") => None,
            Err(error) => return Err(db_err(pool, error)),
        };
    if let Some(rowid) = vec_rowid {
        let _ = sqlx::query("DELETE FROM memory_vec WHERE rowid = ?")
            .bind(rowid)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM memory_vec_map WHERE memory_id = ?")
            .bind(id)
            .execute(pool)
            .await;
    }

    sqlx::query("DELETE FROM memory_cues WHERE memory_id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| db_err(pool, e))?;
    sqlx::query("DELETE FROM memory_links WHERE from_id = ? OR to_id = ?")
        .bind(id)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| db_err(pool, e))?;

    let deleted = delete(pool, id).await?;
    if !deleted {
        return Ok(false);
    }

    let now = now_ms();
    sqlx::query("INSERT OR IGNORE INTO memory_tombstones (memory_id, purged_at) VALUES (?, ?)")
        .bind(id)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| db_err(pool, e))?;
    Ok(true)
}

/// Backfill empty `search_text` and rebuild `memories_fts` from the memories table.
pub async fn reindex_memory_fts(pool: &SqlitePool) -> Result<u64, AxError> {
    let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT id, title, body, tags, files FROM memories
         WHERE search_text IS NULL OR trim(search_text) = ''",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| db_err(pool, e))?;
    let mut updated = 0u64;
    for (id, title, body, tags_json, files_json) in rows {
        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
        let files: Vec<String> = serde_json::from_str(&files_json).unwrap_or_default();
        let search = crate::search_text::normalize_search_text(&format!(
            "{} {} {} {}",
            title,
            body,
            tags.join(" "),
            files.join(" ")
        ));
        let result = sqlx::query("UPDATE memories SET search_text = ? WHERE id = ?")
            .bind(&search)
            .bind(&id)
            .execute(pool)
            .await
            .map_err(|e| db_err(pool, e))?;
        updated += result.rows_affected();
    }
    sqlx::query("INSERT INTO memories_fts(memories_fts) VALUES('rebuild')")
        .execute(pool)
        .await
        .map_err(|e| db_err(pool, e))?;
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_query_drops_stop_words_and_short_tokens() {
        let q = fts_query_from_text("the fix of db in x").unwrap();
        assert!(q.contains("\"fix\""), "got {q}");
        // Lexicon expands fix → repair/herstel/oplossen for bilingual recall.
        assert!(
            q.contains("\"repair\"") || q.contains("\"herstel\""),
            "got {q}"
        );
        assert!(!q.contains("\"the\""), "got {q}");
        assert!(fts_query_from_text("a of in").is_none());
    }

    #[test]
    fn decay_halves_after_half_life() {
        let now = 1_000_000_000_000_i64;
        let ninety_days_ago = now - (90 * 86_400_000);
        let decayed = effective_confidence(1.0, ninety_days_ago, now);
        assert!((decayed - 0.5).abs() < 0.01, "got {decayed}");
    }
    #[test]
    fn coverage_terms_drop_stopwords_and_pure_numerics() {
        let terms = coverage_terms("the medieval lute for symphony #404 using only tabs");
        assert!(!terms.iter().any(|t| t == "the"
            || t == "for"
            || t == "using"
            || t == "only"
            || t == "404"));
        assert!(terms.iter().any(|t| t == "medieval"));
        assert!(terms.iter().any(|t| t == "symphony"));
    }

    #[test]
    fn skipping_relative_coverage_keeps_a_partial_hit_and_still_abstains_on_digits() {
        fn row(id: &str, title: &str) -> MemoryRow {
            MemoryRow {
                id: id.into(),
                kind: "note".into(),
                title: title.into(),
                body: String::new(),
                tags: vec![],
                files: vec![],
                confidence: 1.0,
                source: "test".into(),
                enabled: true,
                created_at: 1,
                updated_at: 1,
                tier: "t2".into(),
            }
        }
        let mut matches = vec![
            MemoryMatch {
                memory: row("strong", "alpha beta gamma delta"),
                score: 1.0,
                why: None,
            },
            MemoryMatch {
                memory: row("weak", "alpha"),
                score: 0.4,
                why: None,
            },
        ];
        drop_weak_term_matches_with_relative(&mut matches, "alpha beta gamma delta", false);
        assert_eq!(matches.len(), 2);
        let mut digits = vec![MemoryMatch {
            memory: row("spotify", "callback 404"),
            score: 0.9,
            why: None,
        }];
        drop_weak_term_matches_with_relative(
            &mut digits,
            "medieval lute tablature for imaginary symphony 404",
            false,
        );
        assert!(digits.is_empty());
    }

    #[test]
    fn drop_weak_clears_single_token_overlap_on_long_prompts() {
        let memory = MemoryRow {
            id: "m1".into(),
            kind: "note".into(),
            title: "Spotify callback".into(),
            body: "smoke observed 404 on /callback".into(),
            tags: vec![],
            files: vec![],
            confidence: 1.0,
            source: "test".into(),
            enabled: true,
            created_at: 1,
            updated_at: 1,
            tier: "t2".into(),
        };
        let mut matches = vec![MemoryMatch {
            memory,
            score: 0.9,
            why: None,
        }];
        drop_weak_term_matches(
            &mut matches,
            "medieval lute tablature for imaginary symphony #404",
        );
        assert!(matches.is_empty());
    }

    #[test]
    fn content_overlap_separates_digits_one_token_and_two_tokens() {
        let digits = content_overlap(
            "medieval lute tablature for imaginary symphony 404",
            "callback 404",
            "",
        );
        assert_eq!(digits.hits, 0);
        assert!(digits.terms >= 4);

        let one = content_overlap(
            "bake a cake using only hexadecimal integers",
            "hexadecimal fingerprints",
            "",
        );
        assert_eq!(one.hits, 1);
        assert!(one.terms >= 4);

        let two = content_overlap(
            "quartz basalt granite marble schist gneiss slate chalk flint",
            "quartz basalt",
            "",
        );
        assert_eq!(two.hits, 2);
        assert_eq!(two.terms, 9);
        assert!((two.hits as f64) / (two.terms as f64) < 0.25);
    }

    #[test]
    fn identifier_stem_keeps_loadturn_and_rejects_lute_and_cake() {
        let rename = identifier_stem_hits(
            "loadturn",
            "Turn persistence function",
            "save_turn writes one turn row.",
        );
        assert!(
            rename >= 1,
            "loadturn should share a stem with save_turn, got {rename}"
        );
        let lute = identifier_stem_hits(
            "medieval lute tablature for imaginary symphony #404",
            "Blossom Spotify OAuth HTTPS callback",
            "Independent smoke observed 404 on /callback, so the user must pull latest main.",
        );
        assert_eq!(lute, 0, "lute/404 must not gain a stem hit");
        let cake = identifier_stem_hits(
            "bake a cake using only hexadecimal integers",
            "Music DNA deterministic replay contract",
            "Regression fixture locks hexadecimal fingerprints for arrangement analysis.",
        );
        assert_eq!(
            cake, 1,
            "one hexadecimal token must stay one hit, got {cake}"
        );
    }

    #[test]
    fn identifier_boundary_keeps_loadturn_and_skips_plain_words() {
        let rename = identifier_boundary_hits(
            "loadturn",
            "Turn persistence function",
            "save_turn writes one turn row.",
        );
        assert!(
            rename >= 1,
            "loadturn should match the save_turn token, got {rename}"
        );
        let lute = identifier_boundary_hits(
            "medieval lute tablature for imaginary symphony #404",
            "Blossom Spotify OAuth HTTPS callback",
            "Independent smoke observed 404 on /callback, so the user must pull latest main.",
        );
        assert_eq!(lute, 0, "lute/404 must not gain a boundary hit");
        let cake = identifier_boundary_hits(
            "bake a cake using only hexadecimal integers",
            "Music DNA deterministic replay contract",
            "Regression fixture locks hexadecimal fingerprints for arrangement analysis.",
        );
        assert_eq!(
            cake, 1,
            "one hexadecimal token must stay one hit, got {cake}"
        );
        let plain = identifier_boundary_hits(
            "pudding",
            "Finish full ship pipeline including git push",
            "",
        );
        assert_eq!(plain, 0, "plain words must not stem-match");
    }

    #[test]
    fn identifier_affix_keeps_loadturn_and_rejects_content_infix() {
        let rename = identifier_affix_hits(
            "loadturn",
            "Turn persistence function",
            "save_turn writes one turn row.",
        );
        assert!(
            rename >= 1,
            "loadturn should match the save_turn suffix, got {rename}"
        );
        let infix = identifier_affix_hits("content", "WebDAV vault guard ax-context", "");
        assert_eq!(infix, 0, "content must not match inside ax-context");
        let lute = identifier_affix_hits(
            "medieval lute tablature for imaginary symphony #404",
            "Blossom Spotify OAuth HTTPS callback",
            "Independent smoke observed 404 on /callback, so the user must pull latest main.",
        );
        assert_eq!(lute, 0, "lute/404 must not gain an affix hit");
        let cake = identifier_affix_hits(
            "bake a cake using only hexadecimal integers",
            "Music DNA deterministic replay contract",
            "Regression fixture locks hexadecimal fingerprints for arrangement analysis.",
        );
        assert_eq!(
            cake, 1,
            "one hexadecimal token must stay one hit, got {cake}"
        );
        let plain = identifier_affix_hits(
            "pudding",
            "Finish full ship pipeline including git push",
            "",
        );
        assert_eq!(plain, 0, "plain words must not affix-match");
    }

    #[test]
    fn identifier_affix_cross_matches_on_live_probe_titles() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/memory/live-probes.json");
        let text = std::fs::read_to_string(path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let probes = value["probes"].as_array().unwrap();
        let mut delivery = Vec::new();
        let mut abstain = Vec::new();
        for probe in probes {
            if probe["family"] == "abstention" {
                abstain.push(probe["query"].as_str().unwrap().to_string());
            } else if let (Some(id), Some(query), Some(title)) = (
                probe["id"].as_str(),
                probe["query"].as_str(),
                probe["title"].as_str(),
            ) {
                delivery.push((id.to_string(), query.to_string(), title.to_string()));
            }
        }
        let mut abstain_cross = Vec::new();
        let mut delivery_cross = Vec::new();
        for query in &abstain {
            for (id, _, title) in &delivery {
                let whole = content_overlap(query, title, "").hits;
                let affix = identifier_affix_hits(query, title, "");
                if affix > whole {
                    abstain_cross.push(format!("{query} -> {id}"));
                }
            }
        }
        for (id, query, own_title) in &delivery {
            let own_whole = content_overlap(query, own_title, "").hits;
            let own_affix = identifier_affix_hits(query, own_title, "");
            assert!(own_affix >= own_whole, "{id}");
            for (other_id, _, title) in &delivery {
                if other_id == id {
                    continue;
                }
                let whole = content_overlap(query, title, "").hits;
                let affix = identifier_affix_hits(query, title, "");
                if affix > whole {
                    delivery_cross.push(format!("{id} -> {other_id}"));
                }
            }
        }
        assert_eq!(abstain.len(), 10);
        assert_eq!(delivery.len(), 15);
        assert!(abstain_cross.is_empty());
        assert!(delivery_cross.is_empty());
    }

    #[test]
    fn identifier_boundary_cross_matches_on_live_probe_titles() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/memory/live-probes.json");
        let text = std::fs::read_to_string(path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let probes = value["probes"].as_array().unwrap();
        let mut delivery = Vec::new();
        let mut abstain = Vec::new();
        for probe in probes {
            if probe["family"] == "abstention" {
                abstain.push(probe["query"].as_str().unwrap().to_string());
            } else if let (Some(id), Some(query), Some(title)) = (
                probe["id"].as_str(),
                probe["query"].as_str(),
                probe["title"].as_str(),
            ) {
                delivery.push((id.to_string(), query.to_string(), title.to_string()));
            }
        }
        let mut abstain_cross = Vec::new();
        let mut delivery_cross = Vec::new();
        for query in &abstain {
            for (id, _, title) in &delivery {
                let whole = content_overlap(query, title, "").hits;
                let boundary = identifier_boundary_hits(query, title, "");
                if boundary > whole {
                    abstain_cross.push(format!("{query} -> {id}"));
                }
            }
        }
        for (id, query, own_title) in &delivery {
            let own_whole = content_overlap(query, own_title, "").hits;
            let own_boundary = identifier_boundary_hits(query, own_title, "");
            assert!(own_boundary >= own_whole, "{id}");
            for (other_id, _, title) in &delivery {
                if other_id == id {
                    continue;
                }
                let whole = content_overlap(query, title, "").hits;
                let boundary = identifier_boundary_hits(query, title, "");
                if boundary > whole {
                    delivery_cross.push(format!("{id} -> {other_id}"));
                }
            }
        }
        assert_eq!(abstain.len(), 10);
        assert_eq!(delivery.len(), 15);
        assert!(abstain_cross.is_empty());
        assert_eq!(
            delivery_cross,
            vec!["del-bug_fix-208ef9e0 -> del-architecture-c947ca38".to_string()]
        );
    }

    #[test]
    fn identifier_stem_cross_matches_on_live_probe_titles() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/memory/live-probes.json");
        let text = std::fs::read_to_string(path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let probes = value["probes"].as_array().unwrap();
        let mut delivery = Vec::new();
        let mut abstain = Vec::new();
        for probe in probes {
            if probe["family"] == "abstention" {
                abstain.push(probe["query"].as_str().unwrap().to_string());
            } else if let (Some(id), Some(query), Some(title)) = (
                probe["id"].as_str(),
                probe["query"].as_str(),
                probe["title"].as_str(),
            ) {
                delivery.push((id.to_string(), query.to_string(), title.to_string()));
            }
        }
        let mut abstain_cross = Vec::new();
        for query in &abstain {
            for (id, _, title) in &delivery {
                let whole = content_overlap(query, title, "").hits;
                let stem = identifier_stem_hits(query, title, "");
                if stem > whole {
                    abstain_cross.push(format!("{query} -> {id}"));
                }
            }
        }
        let mut delivery_cross = Vec::new();
        for (id, query, own_title) in &delivery {
            let own_whole = content_overlap(query, own_title, "").hits;
            let own_stem = identifier_stem_hits(query, own_title, "");
            assert!(own_stem >= own_whole, "{id}");
            for (other_id, _, title) in &delivery {
                if other_id == id {
                    continue;
                }
                let whole = content_overlap(query, title, "").hits;
                let stem = identifier_stem_hits(query, title, "");
                if stem > whole {
                    delivery_cross.push(format!("{id} -> {other_id}"));
                }
            }
        }
        assert_eq!(abstain.len(), 10);
        assert_eq!(delivery.len(), 15);
        assert_eq!(
            abstain_cross,
            vec![
                "xyzzy plum pudding quantum zucchini neon trombone -> del-decision-1dc0989a".to_string(),
                "completely invented API called ZorpulateX without any docs -> del-architecture-66b6c786".to_string(),
                "completely invented API called ZorpulateX without any docs -> del-bug_fix-208ef9e0".to_string(),
                "phone number of the queen of mars directory listing -> del-bug_fix-15b7fe94".to_string(),
            ]
        );
        assert_eq!(delivery_cross.len(), 12);
        assert!(
            identifier_stem_hits(
                "pudding",
                "Finish full ship pipeline including git push",
                "",
            ) >= 1
        );
    }
}
