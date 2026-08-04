//! CRUD + FTS recall for the memory vault.

use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

use crate::types::{MemoryMatch, MemoryRow, RememberInput};

/// Confidence halves every 90 days unless the memory is updated.
const DECAY_HALF_LIFE_DAYS: f64 = 90.0;

fn db_err(e: impl std::fmt::Display) -> AxError {
    AxError::Database(DatabaseError::new(e.to_string()))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
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
);

fn row_from_db(
    (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, enabled): MemoryDbRow,
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
    }
}

const MEMORY_SELECT: &str = r#"SELECT id, kind, title, body, tags, files, confidence, source, created_at, updated_at, enabled
           FROM memories"#;

/// Time-decayed confidence: recently touched memories rank higher.
pub fn effective_confidence(confidence: f64, updated_at_ms: i64, now_ms: i64) -> f64 {
    let age_days = ((now_ms - updated_at_ms).max(0) as f64) / 86_400_000.0;
    confidence * 0.5_f64.powf(age_days / DECAY_HALF_LIFE_DAYS)
}

pub async fn remember(pool: &SqlitePool, input: RememberInput) -> Result<MemoryRow, AxError> {
    let now = now_ms();
    let title = if input.title.trim().is_empty() {
        // First line of the body doubles as the title.
        input.body.lines().next().unwrap_or("").chars().take(120).collect()
    } else {
        input.title.trim().to_string()
    };
    let row = MemoryRow {
        id: uuid::Uuid::new_v4().to_string(),
        kind: input.kind.unwrap_or_else(|| "note".into()),
        title,
        body: input.body,
        tags: input.tags,
        files: input.files,
        confidence: 1.0,
        source: input.source.unwrap_or_else(|| "manual".into()),
        enabled: true,
        created_at: now,
        updated_at: now,
    };
    let embedding = crate::embed::embedding_to_blob(&crate::embed::embed_text(&format!(
        "{} {} {}",
        row.title,
        row.body,
        row.tags.join(" ")
    )));
    sqlx::query(
        r#"INSERT INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, embedding, enabled)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)"#,
    )
    .bind(&row.id)
    .bind(&row.kind)
    .bind(&row.title)
    .bind(&row.body)
    .bind(serde_json::to_string(&row.tags).unwrap_or_else(|_| "[]".into()))
    .bind(serde_json::to_string(&row.files).unwrap_or_else(|_| "[]".into()))
    .bind(row.confidence)
    .bind(&row.source)
    .bind(row.created_at)
    .bind(row.updated_at)
    .bind(embedding)
    .execute(pool)
    .await
    .map_err(db_err)?;
    Ok(row)
}

/// Sanitize free text into an FTS5 OR-query of quoted tokens.
/// Returns `None` when there is nothing searchable.
pub fn fts_query_from_text(text: &str) -> Option<String> {
    const STOP_WORDS: &[&str] = &[
        "the", "a", "an", "and", "or", "of", "in", "on", "to", "for", "with", "is", "are",
        "de", "het", "een", "en", "van", "voor", "met", "dat", "die", "je", "ik",
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
pub async fn recall(pool: &SqlitePool, query: &str, limit: usize) -> Result<Vec<MemoryMatch>, AxError> {
    use std::collections::HashMap;

    let overfetch = (limit * 4).max(20);

    // Lexical leg: FTS5 with BM25 ranking.
    let mut fts_ranked: Vec<String> = Vec::new();
    if let Some(fts_query) = fts_query_from_text(query) {
        let ids = sqlx::query_scalar::<_, String>(
            r#"SELECT m.id
               FROM memories_fts
               JOIN memories m ON m.rowid = memories_fts.rowid
               WHERE memories_fts MATCH ? AND m.enabled = 1
               ORDER BY bm25(memories_fts)
               LIMIT ?"#,
        )
        .bind(&fts_query)
        .bind(overfetch as i64)
        .fetch_all(pool)
        .await
        .map_err(db_err)?;
        fts_ranked = ids;
    }

    // Vector leg: cosine similarity over stored embeddings.
    let query_embedding = crate::embed::embed_text(query);
    let embedding_rows = sqlx::query_as::<_, (String, Vec<u8>)>(
        "SELECT id, embedding FROM memories WHERE embedding IS NOT NULL AND enabled = 1",
    )
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

    if fts_ranked.is_empty() && vector_scored.is_empty() {
        return Ok(Vec::new());
    }

    // Weighted Reciprocal Rank Fusion (vector / FTS). Graph proximity can be
    // layered later via WEIGHT_GRAPH when entity links are available.
    let mut fused: HashMap<String, f64> = HashMap::new();
    for (rank, id) in fts_ranked.iter().enumerate() {
        *fused.entry(id.clone()).or_default() +=
            crate::embed::WEIGHT_FTS * crate::embed::rrf_score(rank);
    }
    for (rank, (id, _)) in vector_scored.iter().enumerate() {
        *fused.entry(id.clone()).or_default() +=
            crate::embed::WEIGHT_VECTOR * crate::embed::rrf_score(rank);
    }
    let _ = crate::embed::WEIGHT_GRAPH; // reserved for entity-linked boost

    let ids: Vec<String> = fused.keys().cloned().collect();
    let now = now_ms();
    let mut matches = Vec::with_capacity(ids.len());
    for id in &ids {
        if let Some(memory) = get(pool, id).await? {
            let decayed = effective_confidence(memory.confidence, memory.updated_at, now);
            let score = fused.get(id).copied().unwrap_or(0.0) * decayed.max(0.05) * 100.0;
            matches.push(MemoryMatch { memory, score });
        }
    }
    matches.sort_by(|a, b| b.score.total_cmp(&a.score));
    matches.truncate(limit);
    Ok(matches)
}

pub async fn list(pool: &SqlitePool, limit: usize, offset: usize) -> Result<(Vec<MemoryRow>, i64), AxError> {
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories")
        .fetch_one(pool)
        .await
        .map_err(db_err)?;
    let rows = sqlx::query_as::<_, MemoryDbRow>(
        &format!("{MEMORY_SELECT} ORDER BY updated_at DESC LIMIT ? OFFSET ?"),
    )
    .bind(limit as i64)
    .bind(offset as i64)
    .fetch_all(pool)
    .await
    .map_err(db_err)?;
    let memories = rows.into_iter().map(row_from_db).collect();
    Ok((memories, total))
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
    let embedding = crate::embed::embedding_to_blob(&crate::embed::embed_text(&format!(
        "{} {} {}",
        input.title,
        input.body,
        input.tags.join(" ")
    )));
    let result = sqlx::query(
        r#"UPDATE memories
           SET title = ?, body = ?, kind = COALESCE(NULLIF(?, ''), kind),
               tags = ?, files = ?, confidence = 1.0, updated_at = ?, embedding = ?
           WHERE id = ?"#,
    )
    .bind(input.title.trim())
    .bind(&input.body)
    .bind(input.kind.unwrap_or_default())
    .bind(serde_json::to_string(&input.tags).unwrap_or_else(|_| "[]".into()))
    .bind(serde_json::to_string(&input.files).unwrap_or_else(|_| "[]".into()))
    .bind(now_ms())
    .bind(embedding)
    .bind(id)
    .execute(pool)
    .await
    .map_err(db_err)?;
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
    let query_embedding = crate::embed::embed_text(text);
    let rows = sqlx::query_as::<_, (String, Vec<u8>)>(
        "SELECT id, embedding FROM memories WHERE embedding IS NOT NULL AND enabled = 1",
    )
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
            out.push(MemoryMatch { memory, score: sim as f64 });
        }
    }
    Ok(out)
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<bool, AxError> {
    let result = sqlx::query("DELETE FROM memories WHERE id = ?")
        .bind(id)
        .execute(pool)
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
