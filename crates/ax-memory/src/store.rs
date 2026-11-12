//! CRUD + FTS recall for the memory vault.

use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

use crate::types::{MemoryMatch, MemoryRow, RememberInput};

/// Confidence halves every 90 days unless the memory is updated.
const DECAY_HALF_LIFE_DAYS: f64 = 90.0;

pub(crate) fn db_err(e: impl std::fmt::Display) -> AxError {
    AxError::Database(DatabaseError::new(e.to_string()))
}

pub(crate) fn db_err_pub(e: impl std::fmt::Display) -> AxError {
    db_err(e)
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
    .map_err(db_err)?;
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
                .map_err(db_err)?;
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
    .map_err(db_err)?;
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
    .map_err(db_err)?;
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
    .map_err(db_err)?;
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
    .map_err(db_err)?;
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
    retry_locked(|| {
        sqlx::query(
            r#"INSERT INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, embedding, enabled)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)"#,
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
        .execute(pool)
    })
    .await
    .map_err(db_err)?;
    crate::brain::stamp_memory(pool, &row.id, &row.kind, model).await?;
    crate::brain::register_cues(pool, &row, &[]).await?;
    Ok(row)
}

/// Sanitize free text into an FTS5 OR-query of quoted tokens.
/// Returns `None` when there is nothing searchable.
pub fn fts_query_from_text(text: &str) -> Option<String> {
    const STOP_WORDS: &[&str] = &[
        "the", "a", "an", "and", "or", "of", "in", "on", "to", "for", "with", "is", "are", "de",
        "het", "een", "en", "van", "voor", "met", "dat", "die", "je", "ik",
    ];
    let mut tokens: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .map(str::trim)
        .filter(|t| t.len() >= 3)
        .filter(|t| !STOP_WORDS.contains(&t.to_ascii_lowercase().as_str()))
        .map(|t| format!("\"{}\"", t.replace('"', "")))
        .collect();
    tokens.dedup();
    tokens.truncate(24);
    if tokens.is_empty() {
        None
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

/// Hot recall skips the archive and inactive rows. Archive recall uses FTS
/// plus sqlite-vec KNN and never cosine-compares a different embedding model.
pub(crate) async fn recall_scoped(
    pool: &SqlitePool,
    query: &str,
    limit: usize,
    files: &[String],
    archive: bool,
) -> Result<Vec<MemoryMatch>, AxError> {
    use std::collections::HashMap;

    let overfetch = (limit * 4).max(20);
    let (_, model) = crate::embed::embed_labeled(query);
    let tier_sql = if archive {
        "m.tier = 't3'"
    } else {
        "COALESCE(m.tier, 't2') != 't3' AND COALESCE(m.status, 'active') = 'active'"
    };

    let mut fts_ranked: Vec<String> = Vec::new();
    if let Some(fts_query) = fts_query_from_text(query) {
        let sql = format!(
            r#"SELECT m.id
               FROM memories_fts
               JOIN memories m ON m.rowid = memories_fts.rowid
               WHERE memories_fts MATCH ? AND m.enabled = 1 AND {tier_sql}
                 AND m.embedding_model = ?
               ORDER BY bm25(memories_fts)
               LIMIT ?"#
        );
        fts_ranked = sqlx::query_scalar::<_, String>(&sql)
            .bind(&fts_query)
            .bind(model)
            .bind(overfetch as i64)
            .fetch_all(pool)
            .await
            .map_err(db_err)?;
    }

    let vector_ranked: Vec<String> = if archive {
        crate::brain::knn(pool, query, overfetch).await?
    } else {
        let (query_embedding, _) = crate::embed::embed_labeled(query);
        let embedding_rows = sqlx::query_as::<_, (String, Vec<u8>)>(
            "SELECT id, embedding FROM memories
             WHERE embedding IS NOT NULL AND enabled = 1
               AND COALESCE(tier, 't2') != 't3'
               AND COALESCE(status, 'active') = 'active'
               AND embedding_model = ?",
        )
        .bind(model)
        .fetch_all(pool)
        .await
        .map_err(db_err)?;
        let mut vector_scored: Vec<(String, f32)> = embedding_rows
            .into_iter()
            .filter_map(|(id, blob)| {
                let e = crate::embed::blob_to_embedding(&blob)?;
                Some((id, crate::embed::cosine(&query_embedding, &e)))
            })
            .filter(|(_, sim)| *sim > 0.1)
            .collect();
        vector_scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        vector_scored.truncate(overfetch);
        vector_scored.into_iter().map(|(id, _)| id).collect()
    };

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
    let mut matches = Vec::with_capacity(ids.len());
    for id in &ids {
        if let Some(memory) = get(pool, id).await? {
            let decayed = effective_confidence(memory.confidence, memory.updated_at, now);
            let mut score = fused.get(id).copied().unwrap_or(0.0) * decayed.max(0.05) * 100.0;
            let file_hit = crate::brain::files_overlap(files, &memory.files);
            let symbol_hit = memory.tier == crate::brain::TIER_SEMANTIC && symbol_hits.contains(id);
            if file_hit || symbol_hit {
                score += crate::embed::WEIGHT_GRAPH * 100.0;
            }
            matches.push(MemoryMatch { memory, score });
        }
    }
    matches.sort_by(|a, b| b.score.total_cmp(&a.score));
    if !archive {
        drop_weak_term_matches(&mut matches, query);
    }
    matches.truncate(limit);
    Ok(matches)
}

const MIN_RELATIVE_COVERAGE: f64 = 0.6;

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

/// "dropdowns" covers a memory that says "dropdown".
fn singular(term: &str) -> Option<&str> {
    term.strip_suffix('s').filter(|s| s.chars().count() >= 3)
}

fn term_coverage(terms: &[String], memory: &MemoryRow) -> f64 {
    let text = format!("{} {}", memory.title, memory.body).to_lowercase();
    let hits = terms
        .iter()
        .filter(|t| text.contains(t.as_str()) || singular(t).is_some_and(|s| text.contains(s)))
        .count();
    hits as f64 / terms.len() as f64
}

/// Rank fusion scores rank 2 almost like rank 1, so a hit sharing one word with the query
/// survives next to a hit that matches all of them. Keep hits close to the best coverage.
/// Coverage is substring-based, so a typo like "reinstal" still covers "reinstall"; a best
/// coverage of zero means nothing matches and the vector leg only found noise.
fn drop_weak_term_matches(matches: &mut Vec<MemoryMatch>, query: &str) {
    let terms = query_terms(query);
    if terms.is_empty() {
        return;
    }
    let coverage: Vec<f64> = matches
        .iter()
        .map(|m| term_coverage(&terms, &m.memory))
        .collect();
    let best = coverage.iter().copied().fold(0.0, f64::max);
    if best == 0.0 {
        matches.clear();
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
        .map_err(db_err)?;
    let rows = sqlx::query_as::<_, MemoryDbRow>(&format!(
        "{MEMORY_SELECT} ORDER BY updated_at DESC LIMIT ? OFFSET ?"
    ))
    .bind(limit as i64)
    .bind(offset as i64)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    let memories = rows.into_iter().map(row_from_db).collect();
    Ok((memories, total))
}

/// Lightweight title catalog for stable wiki-link disambiguation. Bodies, tags
/// and file lists are fetched by ID only for selected targets.
pub async fn link_catalog(pool: &SqlitePool) -> Result<Vec<MemoryRow>, AxError> {
    let rows: Vec<(String, String, String, i64, i64, i64, String)> = sqlx::query_as(
        "SELECT id, kind, title, created_at, updated_at, enabled, COALESCE(tier, 't2') FROM memories ORDER BY created_at, id"
    ).fetch_all(pool).await.map_err(db_err)?;
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
        .map_err(db_err)?;
    Ok(row.map(row_from_db))
}

pub async fn set_enabled(pool: &SqlitePool, id: &str, enabled: bool) -> Result<bool, AxError> {
    let result = sqlx::query("UPDATE memories SET enabled = ? WHERE id = ?")
        .bind(if enabled { 1 } else { 0 })
        .bind(id)
        .execute(pool)
        .await
        .map_err(db_err)?;
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
    .map_err(db_err)?;
    if result.rows_affected() > 0 {
        if let Some(row) = get(pool, id).await? {
            crate::brain::register_cues(pool, &row, &[]).await?;
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
    let (query_embedding, model) = crate::embed::embed_labeled(text);
    let rows = sqlx::query_as::<_, (String, Vec<u8>)>(
        "SELECT id, embedding FROM memories
         WHERE embedding IS NOT NULL AND enabled = 1
           AND COALESCE(tier, 't2') != 't3'
           AND embedding_model = ?",
    )
    .bind(model)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;

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
    .map_err(db_err)?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_query_drops_stop_words_and_short_tokens() {
        let q = fts_query_from_text("the fix of db in x").unwrap();
        assert_eq!(q, "\"fix\"");
        assert!(fts_query_from_text("a of in").is_none());
    }

    #[test]
    fn decay_halves_after_half_life() {
        let now = 1_000_000_000_000_i64;
        let ninety_days_ago = now - (90 * 86_400_000);
        let decayed = effective_confidence(1.0, ninety_days_ago, now);
        assert!((decayed - 0.5).abs() < 0.01, "got {decayed}");
    }
}
