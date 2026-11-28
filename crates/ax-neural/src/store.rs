use crate::graph::{InputEdge, Suggestion};
use crate::learning::{promotion_allowed, secret_shaped, FeedbackRow};
use crate::predict::Observation;
use crate::runtime::Backend;
use serde_json::{json, Value};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

pub async fn insert_feedback(pool: &SqlitePool, row: &FeedbackRow) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO neural_feedback (
            id, project_id, execution_id, task_category, context_ids, skill_ids, rule_ids,
            outcome, context_attribution, retry_count, input_tokens, output_tokens, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(&row.project_id)
    .bind(&row.execution_id)
    .bind(&row.task_category)
    .bind(&row.context_ids)
    .bind(&row.skill_ids)
    .bind(&row.rule_ids)
    .bind(&row.outcome)
    .bind(&row.context_attribution)
    .bind(row.retry_count)
    .bind(row.input_tokens)
    .bind(row.output_tokens)
    .bind(now())
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|error| error.to_string())
}

pub async fn feedback_count(pool: &SqlitePool, project_id: &str) -> Result<i64, String> {
    sqlx::query("SELECT COUNT(*) AS n FROM neural_feedback WHERE project_id = ?")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .map(|row| row.get("n"))
        .map_err(|error| error.to_string())
}

pub async fn outcome_counts(
    pool: &SqlitePool,
    project_id: &str,
) -> Result<Vec<(String, u64)>, String> {
    let rows = sqlx::query(
        "SELECT outcome, COUNT(*) AS n FROM neural_feedback WHERE project_id = ? GROUP BY outcome",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("outcome"),
                row.get::<i64, _>("n") as u64,
            )
        })
        .collect())
}

pub async fn observations(pool: &SqlitePool, project_id: &str) -> Result<Vec<Observation>, String> {
    let rows = sqlx::query(
        "SELECT task_category, skill_ids, rule_ids, context_ids FROM neural_feedback WHERE project_id = ?",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .map(|row| Observation {
            task_category: row.get("task_category"),
            skills: string_list(row.get("skill_ids")),
            rules: string_list(row.get("rule_ids")),
            context_ids: string_list(row.get("context_ids")),
        })
        .collect())
}

pub async fn insert_embedding(
    pool: &SqlitePool,
    entity_kind: &str,
    entity_id: &str,
    content_hash: &str,
    model_id: &str,
    embedding: &[f32],
    admitted: bool,
) -> Result<(), String> {
    if model_id == ax_memory::embed::ACTIVE_MODEL {
        return Err("hash-v1 stays on memories.embedding".into());
    }
    if !admitted {
        return Err("model is not admitted by the bake-off".into());
    }
    let blob = ax_memory::embed::embedding_to_blob(embedding);
    sqlx::query(
        "INSERT INTO neural_embedding (
            entity_kind, entity_id, content_hash, model_id, dim, embedding, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(entity_kind, entity_id, model_id) DO UPDATE SET
            content_hash = excluded.content_hash,
            dim = excluded.dim,
            embedding = excluded.embedding",
    )
    .bind(entity_kind)
    .bind(entity_id)
    .bind(content_hash)
    .bind(model_id)
    .bind(embedding.len() as i64)
    .bind(blob)
    .bind(now())
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|error| error.to_string())
}

pub async fn embedding_count(pool: &SqlitePool) -> Result<i64, String> {
    sqlx::query("SELECT COUNT(*) AS n FROM neural_embedding")
        .fetch_one(pool)
        .await
        .map(|row| row.get("n"))
        .map_err(|error| error.to_string())
}

pub async fn search_semantic(pool: &SqlitePool) -> Result<serde_json::Value, String> {
    search_semantic_query(pool, None, None).await
}

/// Semantic search over `neural_embedding` only. Never reads `memories.embedding`.
///
/// Stored vectors are ranked only when the lexical challenger is admitted and
/// the runtime backend is a semantic model. The built-in `cpu-blake3` backend
/// hashes the whole text, so it never ranks; the reply stays on the hash-v1
/// fallback and says why.
pub async fn search_semantic_query(
    pool: &SqlitePool,
    query: Option<&str>,
    model_id: Option<&str>,
) -> Result<serde_json::Value, String> {
    let count = embedding_count(pool).await?;
    let admitted = crate::bakeoff::challenger_admitted();
    let fallback = |reason: &str| {
        serde_json::json!({
            "hits": [],
            "fallback": "hash-v1",
            "storedVectors": count,
            "scannedMemoryColumn": false,
            "challengerAdmitted": admitted,
            "model": ax_memory::embed::ACTIVE_MODEL,
            "reason": reason
        })
    };
    if !admitted {
        return Ok(serde_json::json!({
            "hits": [],
            "fallback": "hash-v1",
            "storedVectors": count,
            "scannedMemoryColumn": false,
            "challengerAdmitted": false,
            "model": ax_memory::embed::ACTIVE_MODEL,
        }));
    }
    let Some(prompt) = query.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(serde_json::json!({
            "hits": [],
            "fallback": "hash-v1",
            "storedVectors": count,
            "scannedMemoryColumn": false,
            "challengerAdmitted": true,
            "model": ax_memory::embed::ACTIVE_MODEL,
        }));
    };
    let Some(backend) = crate::runtime::active() else {
        return Ok(fallback("runtime backend unavailable"));
    };
    if !backend.semantic() {
        return Ok(fallback(
            "no semantic embedding backend admitted; the runtime backend is a text hash",
        ));
    }
    let prompt = prompt.to_string();
    let query_vec = tokio::task::spawn_blocking(move || backend.embed(&prompt))
        .await
        .map_err(|error| format!("neural task failed: {error}"))??;
    let hits = rank_stored_vectors(pool, &query_vec, model_id).await?;
    Ok(serde_json::json!({
        "hits": hits,
        "fallback": "hash-v1",
        "storedVectors": count,
        "scannedMemoryColumn": false,
        "challengerAdmitted": true,
        "model": crate::runtime::semantic_search_model(true),
    }))
}

/// Top 10 stored vectors by cosine similarity to `query_vec`, skipping rows of
/// another width or (when given) another model id.
async fn rank_stored_vectors(
    pool: &SqlitePool,
    query_vec: &[f32],
    model_id: Option<&str>,
) -> Result<Vec<serde_json::Value>, String> {
    let query_dim = query_vec.len() as i64;
    let expected_bytes = ax_memory::embed::embedding_blob_byte_len(query_vec.len());
    let mut ranked: TopHits<(String, String, String)> = TopHits::new(SEMANTIC_HITS);
    let mut rows = if let Some(model_id) = model_id.filter(|id| !id.is_empty()) {
        sqlx::query(
            "SELECT entity_kind, entity_id, model_id, embedding FROM neural_embedding
             WHERE model_id = ? AND dim = ?",
        )
        .bind(model_id)
        .bind(query_dim)
        .fetch(pool)
    } else {
        sqlx::query(
            "SELECT entity_kind, entity_id, model_id, embedding FROM neural_embedding
             WHERE dim = ?",
        )
        .bind(query_dim)
        .fetch(pool)
    };
    use futures_util::TryStreamExt;
    while let Some(row) = rows.try_next().await.map_err(|e| e.to_string())? {
        let blob: Vec<u8> = row.get("embedding");
        if blob.len() != expected_bytes {
            continue;
        }
        let Some(vector) = ax_memory::embed::blob_to_embedding(&blob) else {
            continue;
        };
        if vector.len() != query_vec.len() {
            continue;
        }
        let score = ax_memory::embed::cosine(query_vec, &vector);
        if !ranked.admits(score) {
            continue;
        }
        ranked.push(
            score,
            (
                row.get::<String, _>("entity_kind"),
                row.get::<String, _>("entity_id"),
                row.get::<String, _>("model_id"),
            ),
        );
    }
    Ok(ranked
        .into_sorted()
        .into_iter()
        .map(|(score, (entity_kind, entity_id, model_id))| {
            serde_json::json!({
                "entityKind": entity_kind,
                "entityId": entity_id,
                "modelId": model_id,
                "score": score
            })
        })
        .collect())
}

/// How many stored vectors a semantic search returns.
const SEMANTIC_HITS: usize = 10;

/// The best `k` items by score, held in a bounded min-heap. Equal scores keep
/// arrival order, so the result matches a stable descending sort truncated to `k`.
struct TopHits<T> {
    k: usize,
    seq: u64,
    heap: std::collections::BinaryHeap<std::cmp::Reverse<Ranked<T>>>,
}

struct Ranked<T> {
    score: f32,
    seq: u64,
    item: T,
}

impl<T> Ranked<T> {
    /// Greater means better: a higher score, then an earlier arrival.
    fn rank_cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.score
            .total_cmp(&other.score)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

impl<T> PartialEq for Ranked<T> {
    fn eq(&self, other: &Self) -> bool {
        self.rank_cmp(other).is_eq()
    }
}
impl<T> Eq for Ranked<T> {}
impl<T> PartialOrd for Ranked<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for Ranked<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.rank_cmp(other)
    }
}

impl<T> TopHits<T> {
    fn new(k: usize) -> Self {
        Self {
            k,
            seq: 0,
            heap: std::collections::BinaryHeap::with_capacity(k + 1),
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.heap.len()
    }

    /// True when an item with `score` would be kept. A later arrival never
    /// beats an equal score already held, so ties are not admitted once full.
    fn admits(&self, score: f32) -> bool {
        if self.heap.len() < self.k {
            return self.k > 0;
        }
        self.heap
            .peek()
            .is_some_and(|worst| score.total_cmp(&worst.0.score).is_gt())
    }

    fn push(&mut self, score: f32, item: T) {
        let seq = self.seq;
        self.seq += 1;
        if !self.admits(score) {
            return;
        }
        self.heap
            .push(std::cmp::Reverse(Ranked { score, seq, item }));
        if self.heap.len() > self.k {
            self.heap.pop();
        }
    }

    /// Best first.
    fn into_sorted(self) -> Vec<(f32, T)> {
        let mut kept: Vec<Ranked<T>> = self.heap.into_iter().map(|entry| entry.0).collect();
        kept.sort_by(|a, b| b.rank_cmp(a));
        kept.into_iter()
            .map(|entry| (entry.score, entry.item))
            .collect()
    }
}

/// Replaces the project's earlier suggestion rows in one transaction.
pub async fn replace_suggestions(
    pool: &SqlitePool,
    project_id: &str,
    suggestions: &[Suggestion],
) -> Result<usize, String> {
    if suggestions
        .iter()
        .any(|item| item.provenance != "predicted")
    {
        return Err("graph suggestions must use provenance predicted".into());
    }
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM neural_graph_suggestion WHERE project_id = ?")
        .bind(project_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    for item in suggestions {
        sqlx::query(
            "INSERT INTO neural_graph_suggestion (
                id, project_id, source, target, score, reason, provenance, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(project_id)
        .bind(&item.source)
        .bind(&item.target)
        .bind(item.score)
        .bind(&item.reason)
        .bind(&item.provenance)
        .bind(now())
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(suggestions.len())
}

/// Graph edges for suggestions. Skips `contains` (file membership) and replica rows.
/// The bool is true when more than `limit` edges exist.
pub async fn project_edges(
    pool: &SqlitePool,
    limit: usize,
) -> Result<(Vec<InputEdge>, bool), String> {
    let rows = sqlx::query(
        "SELECT source, target FROM edges
         WHERE kind != 'contains'
           AND source NOT GLOB 'ax-replica/*'
           AND target NOT GLOB 'ax-replica/*'
         ORDER BY id
         LIMIT ?",
    )
    .bind(limit as i64 + 1)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let more = rows.len() > limit;
    let edges = rows
        .into_iter()
        .take(limit)
        .map(|row| InputEdge {
            source: row.get("source"),
            target: row.get("target"),
        })
        .collect();
    Ok((edges, more))
}

/// Context ids of the newest successful feedback rows, for co-occurrence.
pub async fn success_context_sets(
    pool: &SqlitePool,
    project_id: &str,
    limit: usize,
) -> Result<Vec<Vec<String>>, String> {
    let rows = sqlx::query(
        "SELECT context_ids FROM neural_feedback
         WHERE project_id = ? AND outcome = 'success'
         ORDER BY created_at DESC, id DESC
         LIMIT ?",
    )
    .bind(project_id)
    .bind(limit as i64)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .map(|row| string_list(row.get("context_ids")))
        .collect())
}

/// One registry row. `promoted` starts at 0; only `promote` sets it.
pub struct ModelRecord<'a> {
    pub model_id: &'a str,
    pub version: &'a str,
    pub model_type: &'a str,
    pub dataset_version: &'a str,
    pub checksum: &'a str,
    pub platforms: &'a str,
    pub metrics_json: &'a str,
}

pub async fn register(pool: &SqlitePool, model: &ModelRecord<'_>) -> Result<(), String> {
    if model.model_id == ax_memory::embed::ACTIVE_MODEL {
        return Err("hash-v1 is not a neural registry model".into());
    }
    sqlx::query(
        "INSERT INTO neural_model (
            model_id, version, model_type, dataset_version, checksum, platforms, metrics_json, promoted, created_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?)",
    )
    .bind(model.model_id)
    .bind(model.version)
    .bind(model.model_type)
    .bind(model.dataset_version)
    .bind(model.checksum)
    .bind(model.platforms)
    .bind(model.metrics_json)
    .bind(now())
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|error| error.to_string())
}

/// Makes `version` the one promoted version of `model_id`. The check, the
/// flag change and the audit row commit together or not at all.
pub async fn promote(
    pool: &SqlitePool,
    project_id: &str,
    model_id: &str,
    version: &str,
    explicit: bool,
    beats_baseline: bool,
    regressions_clear: bool,
) -> Result<(), String> {
    if model_id == ax_memory::embed::ACTIVE_MODEL {
        return Err("hash-v1 is not a neural registry model".into());
    }
    promotion_allowed(explicit, beats_baseline, regressions_clear).await?;
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(|error| error.to_string())?;
    let exists: i64 =
        sqlx::query("SELECT COUNT(*) AS n FROM neural_model WHERE model_id = ? AND version = ?")
            .bind(model_id)
            .bind(version)
            .fetch_one(&mut *tx)
            .await
            .map(|row| row.get("n"))
            .map_err(|error| error.to_string())?;
    if exists == 0 {
        return Err("model version is not registered".into());
    }
    activate(&mut tx, project_id, model_id, version, "promote").await?;
    tx.commit().await.map_err(|error| error.to_string())
}

/// Returns `model_id` to the version that was active before the current one,
/// skipping versions that an earlier rollback left. Runs in one transaction.
pub async fn rollback(
    pool: &SqlitePool,
    project_id: &str,
    model_id: &str,
) -> Result<String, String> {
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(|error| error.to_string())?;
    let current = promoted_version(&mut tx, model_id)
        .await?
        .ok_or_else(|| "no promoted model to roll back".to_string())?;
    let stack = activation_stack(&mut tx, model_id).await?;
    let Some(index) = stack.iter().rposition(|version| version == &current) else {
        return Err("no previous promoted version to select".into());
    };
    if index == 0 {
        return Err("no previous promoted version to select".into());
    }
    let version = stack[index - 1].clone();
    activate(&mut tx, project_id, model_id, &version, "rollback").await?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(version)
}

async fn activate(
    tx: &mut sqlx::SqliteConnection,
    project_id: &str,
    model_id: &str,
    version: &str,
    action: &str,
) -> Result<(), String> {
    sqlx::query("UPDATE neural_model SET promoted = 0 WHERE model_id = ?")
        .bind(model_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query("UPDATE neural_model SET promoted = 1 WHERE model_id = ? AND version = ?")
        .bind(model_id)
        .bind(version)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(
        "INSERT INTO neural_audit (project_id, action, detail, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(project_id)
    .bind(action)
    .bind(format!("{model_id}@{version}"))
    .bind(now())
    .execute(&mut *tx)
    .await
    .map(|_| ())
    .map_err(|error| error.to_string())
}

async fn promoted_version(
    conn: &mut sqlx::SqliteConnection,
    model_id: &str,
) -> Result<Option<String>, String> {
    sqlx::query("SELECT version FROM neural_model WHERE model_id = ? AND promoted = 1 LIMIT 1")
        .bind(model_id)
        .fetch_optional(conn)
        .await
        .map(|row| row.map(|row| row.get("version")))
        .map_err(|error| error.to_string())
}

/// The versions that were active for this model, oldest first, rebuilt from
/// every project's promote and rollback audit rows (the promoted flag belongs
/// to the model, not to a project). A promote pushes its version; a rollback
/// pops the version it left, so a rolled-back version is never selected again.
async fn activation_stack(
    conn: &mut sqlx::SqliteConnection,
    model_id: &str,
) -> Result<Vec<String>, String> {
    let prefix = format!("{model_id}@");
    let rows = sqlx::query(
        "SELECT action, detail FROM neural_audit
         WHERE action IN ('promote', 'rollback') AND substr(detail, 1, ?) = ?
         ORDER BY id ASC",
    )
    .bind(prefix.chars().count() as i64)
    .bind(&prefix)
    .fetch_all(conn)
    .await
    .map_err(|error| error.to_string())?;
    let mut stack: Vec<String> = Vec::new();
    for row in rows {
        let action: String = row.get("action");
        let detail: String = row.get("detail");
        let Some(version) = detail.strip_prefix(&prefix) else {
            continue;
        };
        if action == "rollback" {
            stack.pop();
        }
        if stack.last().map(String::as_str) != Some(version) {
            stack.push(version.to_string());
        }
    }
    Ok(stack)
}

pub async fn promoted_checksum(
    pool: &SqlitePool,
    model_id: &str,
) -> Result<Option<String>, String> {
    let row = sqlx::query(
        "SELECT checksum FROM neural_model WHERE model_id = ? AND promoted = 1 LIMIT 1",
    )
    .bind(model_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(row.map(|row| row.get("checksum")))
}

pub async fn list_models(pool: &SqlitePool) -> Result<Vec<SqliteRow>, String> {
    sqlx::query(
        "SELECT model_id, version, checksum, platforms, promoted FROM neural_model ORDER BY created_at ASC, rowid ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())
}

/// Per-project tables whose rows are keyed by `project_id`.
const PROJECT_TABLES: [&str; 3] = ["neural_feedback", "neural_graph_suggestion", "neural_audit"];

/// Moves rows stored under an older spelling of a project's key to `canonical`,
/// in one transaction. Rank totals under both keys are added together. Returns
/// how many legacy rows were moved; with nothing to move it only reads.
pub async fn adopt_project_keys(
    pool: &SqlitePool,
    canonical: &str,
    legacy: &[String],
) -> Result<u64, String> {
    let legacy: Vec<&str> = legacy
        .iter()
        .map(String::as_str)
        .filter(|key| *key != canonical && !key.is_empty())
        .collect();
    if legacy.is_empty() {
        return Ok(0);
    }
    let marks = vec!["?"; legacy.len()].join(", ");
    let probe = PROJECT_TABLES
        .iter()
        .chain(std::iter::once(&"neural_rank_total"))
        .map(|table| format!("SELECT 1 FROM {table} WHERE project_id IN ({marks})"))
        .collect::<Vec<_>>()
        .join(" UNION ALL ");
    let mut query = sqlx::query(&probe);
    for _ in 0..=PROJECT_TABLES.len() {
        for key in &legacy {
            query = query.bind(*key);
        }
    }
    let found = query
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?;
    if found.is_none() {
        return Ok(0);
    }
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    let mut moved = 0;
    for key in &legacy {
        for table in PROJECT_TABLES {
            moved += sqlx::query(&format!(
                "UPDATE {table} SET project_id = ? WHERE project_id = ?"
            ))
            .bind(canonical)
            .bind(key)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?
            .rows_affected();
        }
        sqlx::query(
            "INSERT INTO neural_rank_total (
                project_id, preflights, duplicate_blocks, duplicate_tokens, rank_micros, updated_at
            )
            SELECT ?, preflights, duplicate_blocks, duplicate_tokens, rank_micros, updated_at
            FROM neural_rank_total WHERE project_id = ?
            ON CONFLICT(project_id) DO UPDATE SET
                preflights = preflights + excluded.preflights,
                duplicate_blocks = duplicate_blocks + excluded.duplicate_blocks,
                duplicate_tokens = duplicate_tokens + excluded.duplicate_tokens,
                rank_micros = rank_micros + excluded.rank_micros,
                updated_at = MAX(updated_at, excluded.updated_at)",
        )
        .bind(canonical)
        .bind(key)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        moved += sqlx::query("DELETE FROM neural_rank_total WHERE project_id = ?")
            .bind(key)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?
            .rows_affected();
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(moved)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankTotals {
    pub preflights: i64,
    pub duplicate_blocks: i64,
    pub duplicate_tokens: i64,
    pub rank_micros: i64,
}

/// Adds one preflight to the project's totals.
pub async fn record_rank(
    pool: &SqlitePool,
    project_id: &str,
    duplicate_blocks: i64,
    duplicate_tokens: i64,
    rank_micros: i64,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO neural_rank_total (
            project_id, preflights, duplicate_blocks, duplicate_tokens, rank_micros, updated_at
        ) VALUES (?, 1, ?, ?, ?, ?)
        ON CONFLICT(project_id) DO UPDATE SET
            preflights = preflights + 1,
            duplicate_blocks = duplicate_blocks + excluded.duplicate_blocks,
            duplicate_tokens = duplicate_tokens + excluded.duplicate_tokens,
            rank_micros = rank_micros + excluded.rank_micros,
            updated_at = excluded.updated_at",
    )
    .bind(project_id)
    .bind(duplicate_blocks)
    .bind(duplicate_tokens)
    .bind(rank_micros)
    .bind(now())
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|error| error.to_string())
}

pub async fn rank_totals(
    pool: &SqlitePool,
    project_id: &str,
) -> Result<Option<RankTotals>, String> {
    let row = sqlx::query(
        "SELECT preflights, duplicate_blocks, duplicate_tokens, rank_micros
         FROM neural_rank_total WHERE project_id = ?",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(row.map(|row| RankTotals {
        preflights: row.get("preflights"),
        duplicate_blocks: row.get("duplicate_blocks"),
        duplicate_tokens: row.get("duplicate_tokens"),
        rank_micros: row.get("rank_micros"),
    }))
}

pub async fn record_evaluation(
    pool: &SqlitePool,
    project_id: &str,
    report: &Value,
) -> Result<(), String> {
    audit(pool, project_id, "evaluate", &report.to_string()).await
}

pub async fn last_evaluation(pool: &SqlitePool, project_id: &str) -> Result<Option<Value>, String> {
    let row = sqlx::query(
        "SELECT detail FROM neural_audit
         WHERE project_id = ? AND action = 'evaluate'
         ORDER BY id DESC LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    match row {
        None => Ok(None),
        Some(row) => serde_json::from_str(&row.get::<String, _>("detail"))
            .map(Some)
            .map_err(|error| error.to_string()),
    }
}

pub async fn promoted_models(pool: &SqlitePool) -> Result<Vec<Value>, String> {
    let rows = sqlx::query(
        "SELECT model_id, version, checksum FROM neural_model WHERE promoted = 1 ORDER BY model_id",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .map(|row| {
            json!({
                "modelId": row.get::<String, _>("model_id"),
                "version": row.get::<String, _>("version"),
                "checksum": row.get::<String, _>("checksum"),
            })
        })
        .collect())
}

/// Feedback rows for an offline trainer. Leaves out the project path, execution ids, and
/// any row that looks like it holds a secret, even if it was stored before the check.
pub async fn training_export(pool: &SqlitePool, project_id: &str) -> Result<Value, String> {
    let rows = sqlx::query(
        "SELECT task_category, context_ids, skill_ids, rule_ids, outcome, context_attribution,
                retry_count, input_tokens, output_tokens
         FROM neural_feedback WHERE project_id = ?
         ORDER BY created_at ASC, id ASC",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let mut exported = Vec::new();
    let mut skipped = 0;
    for row in rows {
        let task: String = row.get("task_category");
        let context: String = row.get("context_ids");
        let skills: String = row.get("skill_ids");
        let rules: String = row.get("rule_ids");
        if secret_shaped(&format!("{task} {context} {skills} {rules}")) {
            skipped += 1;
            continue;
        }
        exported.push(json!({
            "taskCategory": task,
            "contextIds": string_list(context),
            "skillIds": string_list(skills),
            "ruleIds": string_list(rules),
            "outcome": row.get::<String, _>("outcome"),
            "contextAttribution": row.get::<String, _>("context_attribution"),
            "retryCount": row.get::<i64, _>("retry_count"),
            "inputTokens": row.get::<i64, _>("input_tokens"),
            "outputTokens": row.get::<i64, _>("output_tokens"),
        }));
    }
    Ok(json!({
        "format": "ax-neural-feedback-v1",
        "exported": exported.len(),
        "skippedSecretShaped": skipped,
        "projectPathIncluded": false,
        "sourceIncluded": false,
        "rows": exported
    }))
}

async fn audit(
    pool: &SqlitePool,
    project_id: &str,
    action: &str,
    detail: &str,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO neural_audit (project_id, action, detail, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(project_id)
    .bind(action)
    .bind(detail)
    .bind(now())
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|error| error.to_string())
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn string_list(text: String) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(&text).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{suggest, InputEdge};
    use crate::learning::{validate_feedback, FeedbackInput};

    async fn db() -> (tempfile::TempDir, ax_db::Database) {
        let dir = tempfile::tempdir().unwrap();
        let database = ax_db::Database::open(&dir.path().join("ax.db"))
            .await
            .expect("temp database");
        (dir, database)
    }

    #[test]
    fn top_hits_keeps_at_most_k_and_matches_a_full_stable_sort() {
        let scores: Vec<f32> = (0..500u32)
            .map(|i| ((i.wrapping_mul(2_654_435_761) >> 7) % 37) as f32 / 7.0)
            .collect();
        let mut top = TopHits::new(10);
        for (index, score) in scores.iter().enumerate() {
            top.push(*score, index);
            assert!(top.len() <= 10);
        }
        let mut expected: Vec<(f32, usize)> = scores.iter().copied().zip(0..).collect();
        expected.sort_by(|a, b| b.0.total_cmp(&a.0));
        expected.truncate(10);
        assert_eq!(top.into_sorted(), expected);
        assert!(TopHits::<usize>::new(10).into_sorted().is_empty());
    }

    #[tokio::test]
    async fn hash_v1_is_rejected_and_an_unadmitted_model_is_not_stored() {
        let (_dir, database) = db().await;
        let error = insert_embedding(
            database.pool(),
            "memory",
            "m1",
            "hash",
            "hash-v1",
            &[0.0; 4],
            true,
        )
        .await
        .unwrap_err();
        assert!(error.contains("hash-v1"));
        let error = insert_embedding(
            database.pool(),
            "memory",
            "m1",
            "hash",
            "candidate",
            &[0.1, 0.2],
            false,
        )
        .await
        .unwrap_err();
        assert!(error.contains("not admitted"));
        assert_eq!(embedding_count(database.pool()).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn feedback_is_append_only_and_suggestions_do_not_touch_edges() {
        let (_dir, database) = db().await;
        let before: i64 = sqlx::query("SELECT COUNT(*) AS n FROM edges")
            .fetch_one(database.pool())
            .await
            .unwrap()
            .get("n");
        let row = validate_feedback(&FeedbackInput {
            project_id: "p".into(),
            execution_id: "e".into(),
            task_category: "memory".into(),
            context_ids: vec!["file.rs".into()],
            skill_ids: vec!["recall".into()],
            rule_ids: vec![],
            outcome: "success".into(),
            retry_count: 1,
            input_tokens: 3,
            output_tokens: 1,
            source_text: String::new(),
        })
        .unwrap();
        insert_feedback(database.pool(), &row).await.unwrap();
        assert_eq!(feedback_count(database.pool(), "p").await.unwrap(), 1);
        let suggestions = suggest(&[
            InputEdge {
                source: "a".into(),
                target: "hub".into(),
            },
            InputEdge {
                source: "b".into(),
                target: "hub".into(),
            },
        ]);
        replace_suggestions(database.pool(), "p", &suggestions)
            .await
            .unwrap();
        let edges: i64 = sqlx::query("SELECT COUNT(*) AS n FROM edges")
            .fetch_one(database.pool())
            .await
            .unwrap()
            .get("n");
        assert_eq!(edges, before);
        let stored: i64 = sqlx::query("SELECT COUNT(*) AS n FROM neural_graph_suggestion")
            .fetch_one(database.pool())
            .await
            .unwrap()
            .get("n");
        assert!(stored >= 1);
    }

    #[tokio::test]
    async fn promote_requires_the_flag_and_rollback_selects_the_previous_checksum() {
        let (_dir, database) = db().await;
        for (version, checksum) in [("1", "aaa"), ("2", "bbb")] {
            let model = ModelRecord {
                model_id: "candidate",
                version,
                model_type: "embedding",
                dataset_version: "ds",
                checksum,
                platforms: "macos-arm64",
                metrics_json: "{}",
            };
            register(database.pool(), &model).await.unwrap();
        }
        assert!(
            promote(database.pool(), "p", "candidate", "2", false, true, true)
                .await
                .is_err()
        );
        // Both versions must be promoted before rollback can select the previous promoted one.
        promote(database.pool(), "p", "candidate", "1", true, false, false)
            .await
            .unwrap();
        promote(database.pool(), "p", "candidate", "2", true, false, false)
            .await
            .unwrap();
        assert_eq!(
            promoted_checksum(database.pool(), "candidate")
                .await
                .unwrap(),
            Some("bbb".into())
        );
        let version = rollback(database.pool(), "p", "candidate").await.unwrap();
        assert_eq!(version, "1");
        assert_eq!(
            promoted_checksum(database.pool(), "candidate")
                .await
                .unwrap(),
            Some("aaa".into())
        );
    }

    #[tokio::test]
    async fn rollback_selects_previous_promoted_not_merely_previous_registered() {
        let (_dir, database) = db().await;
        for (version, checksum) in [("1", "aaa"), ("2", "bbb"), ("3", "ccc")] {
            let model = ModelRecord {
                model_id: "roll",
                version,
                model_type: "ranker",
                dataset_version: "ds",
                checksum,
                platforms: "macos-arm64",
                metrics_json: "{}",
            };
            register(database.pool(), &model).await.unwrap();
        }
        promote(database.pool(), "p", "roll", "1", true, false, false)
            .await
            .unwrap();
        // v2 is registered but never promoted.
        promote(database.pool(), "p", "roll", "3", true, false, false)
            .await
            .unwrap();
        let version = rollback(database.pool(), "p", "roll").await.unwrap();
        assert_eq!(version, "1");
        assert_eq!(
            promoted_checksum(database.pool(), "roll").await.unwrap(),
            Some("aaa".into())
        );
    }

    #[tokio::test]
    async fn rank_totals_accumulate_per_project() {
        let (_dir, database) = db().await;
        record_rank(database.pool(), "p", 1, 40, 250).await.unwrap();
        record_rank(database.pool(), "p", 0, 0, 150).await.unwrap();
        record_rank(database.pool(), "q", 2, 90, 10).await.unwrap();
        assert_eq!(
            rank_totals(database.pool(), "p").await.unwrap(),
            Some(RankTotals {
                preflights: 2,
                duplicate_blocks: 1,
                duplicate_tokens: 40,
                rank_micros: 400,
            })
        );
        assert_eq!(
            rank_totals(database.pool(), "q").await.unwrap(),
            Some(RankTotals {
                preflights: 1,
                duplicate_blocks: 2,
                duplicate_tokens: 90,
                rank_micros: 10,
            })
        );
        assert_eq!(rank_totals(database.pool(), "missing").await.unwrap(), None);
    }

    async fn suggestion_count(database: &ax_db::Database, project: &str) -> i64 {
        sqlx::query("SELECT COUNT(*) AS n FROM neural_graph_suggestion WHERE project_id = ?")
            .bind(project)
            .fetch_one(database.pool())
            .await
            .unwrap()
            .get("n")
    }

    #[tokio::test]
    async fn rerun_replaces_previous_suggestions() {
        let (_dir, database) = db().await;
        let first = suggest(&[
            InputEdge {
                source: "a".into(),
                target: "hub".into(),
            },
            InputEdge {
                source: "b".into(),
                target: "hub".into(),
            },
            InputEdge {
                source: "c".into(),
                target: "hub".into(),
            },
        ]);
        assert_eq!(first.len(), 3);
        replace_suggestions(database.pool(), "p", &first)
            .await
            .unwrap();
        replace_suggestions(database.pool(), "q", &first)
            .await
            .unwrap();
        replace_suggestions(database.pool(), "p", &first[..1])
            .await
            .unwrap();
        assert_eq!(suggestion_count(&database, "p").await, 1);
        assert_eq!(suggestion_count(&database, "q").await, 3);
    }

    #[tokio::test]
    async fn evaluation_is_recorded_and_read_back() {
        let (_dir, database) = db().await;
        assert_eq!(last_evaluation(database.pool(), "p").await.unwrap(), None);
        record_evaluation(
            database.pool(),
            "p",
            &serde_json::json!({ "precision": 0.5 }),
        )
        .await
        .unwrap();
        record_evaluation(
            database.pool(),
            "p",
            &serde_json::json!({ "precision": 1.0 }),
        )
        .await
        .unwrap();
        let last = last_evaluation(database.pool(), "p")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(last["precision"], 1.0);
        assert_eq!(last_evaluation(database.pool(), "q").await.unwrap(), None);
    }

    #[tokio::test]
    async fn training_export_skips_secret_rows_and_project_paths() {
        let (_dir, database) = db().await;
        let project = "/Users/me/client-project";
        let clean = validate_feedback(&FeedbackInput {
            project_id: project.into(),
            execution_id: "exec-private-77".into(),
            task_category: "memory".into(),
            context_ids: vec!["memory:m1".into()],
            skill_ids: vec!["recall".into()],
            rule_ids: vec![],
            outcome: "success".into(),
            retry_count: 0,
            input_tokens: 5,
            output_tokens: 2,
            source_text: String::new(),
        })
        .unwrap();
        insert_feedback(database.pool(), &clean).await.unwrap();
        let mut older = clean.clone();
        older.id = "row-written-before-the-check".into();
        older.context_ids = r#"["password=hunter2"]"#.into();
        insert_feedback(database.pool(), &older).await.unwrap();
        let mut other = clean.clone();
        other.id = "other-project".into();
        other.project_id = "/Users/me/other".into();
        insert_feedback(database.pool(), &other).await.unwrap();

        let export = training_export(database.pool(), project).await.unwrap();
        assert_eq!(export["exported"], 1);
        assert_eq!(export["skippedSecretShaped"], 1);
        assert_eq!(export["projectPathIncluded"], false);
        assert_eq!(export["rows"][0]["contextIds"][0], "memory:m1");
        assert_eq!(export["rows"][0]["contextAttribution"], "unverified");
        let text = export.to_string();
        assert!(!text.contains("client-project"), "{text}");
        assert!(!text.contains("hunter2"), "{text}");
        assert!(!text.contains("exec-private-77"), "{text}");
    }

    #[tokio::test]
    async fn an_admitted_vector_is_stored_and_counted_while_search_stays_on_hash_v1() {
        let (_dir, database) = db().await;
        let pool = database.pool();
        insert_embedding(pool, "memory", "m1", "hash", "candidate", &[0.1, 0.2], true)
            .await
            .unwrap();
        let value = search_semantic(pool).await.unwrap();
        assert_eq!(value["storedVectors"], 1);
        assert_eq!(value["fallback"], "hash-v1");
        // No query → no neural hits; memory column is never scanned.
        assert_eq!(value["hits"], json!([]));
        assert_eq!(value["scannedMemoryColumn"], false);
    }

    fn unit(values: &[f32]) -> Vec<f32> {
        let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
        values.iter().map(|v| v / norm).collect()
    }

    #[tokio::test]
    async fn ranking_skips_wrong_dim_and_filters_by_model_id() {
        let (_dir, database) = db().await;
        let pool = database.pool();
        let query = unit(&[1.0, 0.0, 0.0, 0.0]);
        let near = unit(&[0.9, 0.1, 0.0, 0.0]);
        let far = unit(&[0.0, 0.0, 1.0, 0.0]);
        let wrong_dim = ax_memory::embed::embed_text("legacy hash width");
        insert_embedding(pool, "memory", "m-near", "a", "semantic", &near, true)
            .await
            .unwrap();
        insert_embedding(pool, "memory", "m-far", "b", "semantic", &far, true)
            .await
            .unwrap();
        insert_embedding(pool, "memory", "m-wide", "c", "legacy", &wrong_dim, true)
            .await
            .unwrap();
        insert_embedding(pool, "memory", "m-other", "d", "other", &near, true)
            .await
            .unwrap();

        let ids = |hits: &[Value]| -> Vec<String> {
            hits.iter()
                .map(|h| h["entityId"].as_str().unwrap().to_string())
                .collect()
        };
        let unfiltered = rank_stored_vectors(pool, &query, None).await.unwrap();
        let unfiltered_ids = ids(&unfiltered);
        assert!(!unfiltered_ids.contains(&"m-wide".to_string()));
        assert_eq!(unfiltered_ids.last().map(String::as_str), Some("m-far"));
        assert!(unfiltered[0]["score"].as_f64().unwrap() > 0.99);

        let filtered = rank_stored_vectors(pool, &query, Some("semantic"))
            .await
            .unwrap();
        assert_eq!(ids(&filtered), vec!["m-near", "m-far"]);
    }

    // Runs only where the runtime backend exists, so a default build lists it as not run.
    #[cfg(feature = "neural-runtime")]
    #[tokio::test]
    async fn a_hash_backend_never_ranks_semantic_search_even_when_the_lexical_challenger_is_admitted(
    ) {
        let (_dir, database) = db().await;
        let pool = database.pool();
        let backend = crate::runtime::active().expect("runtime");
        let vector = backend.embed("parser errors").unwrap();
        insert_embedding(pool, "memory", "m-parser", "hash", "cpu", &vector, true)
            .await
            .unwrap();
        let value = search_semantic_query(pool, Some("parser errors"), None)
            .await
            .unwrap();
        assert_eq!(value["hits"], json!([]), "{value}");
        assert_eq!(value["fallback"], "hash-v1");
        assert_eq!(value["model"], ax_memory::embed::ACTIVE_MODEL);
        assert_eq!(value["storedVectors"], 1);
        assert!(value["reason"].as_str().unwrap_or("").contains("semantic"));
    }

    fn wall_clock_seconds() -> i64 {
        let since_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap();
        since_epoch.as_secs() as i64
    }

    #[tokio::test]
    async fn an_audit_row_records_the_time_of_its_action() {
        let (_dir, database) = db().await;
        let pool = database.pool();
        let model = ModelRecord {
            model_id: "candidate",
            version: "1",
            model_type: "embedding",
            dataset_version: "ds",
            checksum: "abc",
            platforms: "macos-arm64",
            metrics_json: "{}",
        };
        register(pool, &model).await.unwrap();
        let before = wall_clock_seconds();
        promote(pool, "p", "candidate", "1", true, true, true)
            .await
            .unwrap();
        let after = wall_clock_seconds();
        let at: i64 = sqlx::query("SELECT created_at FROM neural_audit WHERE action = 'promote'")
            .fetch_one(pool)
            .await
            .unwrap()
            .get(0);
        assert!(
            (before..=after).contains(&at),
            "{before} <= {at} <= {after}"
        );
    }

    #[tokio::test]
    async fn list_models_breaks_created_at_ties_by_registration_order_whatever_the_plan() {
        let (_dir, database) = db().await;
        let pool = database.pool();
        // An index whose tie order differs from insertion order: a plain
        // `ORDER BY created_at` may walk it and return ties in version order.
        sqlx::query("CREATE INDEX test_model_created ON neural_model (created_at, version DESC)")
            .execute(pool)
            .await
            .unwrap();
        for version in ["1", "2", "10"] {
            let model = ModelRecord {
                model_id: "m",
                version,
                model_type: "embedding",
                dataset_version: "ds",
                checksum: "c",
                platforms: "macos-arm64",
                metrics_json: "{}",
            };
            register(pool, &model).await.unwrap();
        }
        sqlx::query("UPDATE neural_model SET created_at = 1")
            .execute(pool)
            .await
            .unwrap();
        let listed: Vec<String> = list_models(pool)
            .await
            .unwrap()
            .iter()
            .map(|row| row.get("version"))
            .collect();
        assert_eq!(listed, ["1", "2", "10"]);
    }

    #[tokio::test]
    async fn versions_registered_in_one_second_keep_their_registration_order() {
        let (_dir, database) = db().await;
        let pool = database.pool();
        for (version, checksum) in [("1", "c1"), ("2", "c2"), ("10", "c10")] {
            let model = ModelRecord {
                model_id: "m",
                version,
                model_type: "embedding",
                dataset_version: "ds",
                checksum,
                platforms: "macos-arm64",
                metrics_json: "{}",
            };
            register(pool, &model).await.unwrap();
        }
        let listed: Vec<String> = list_models(pool)
            .await
            .unwrap()
            .iter()
            .map(|row| row.get("version"))
            .collect();
        assert_eq!(listed, ["1", "2", "10"]);
        promote(pool, "p", "m", "2", true, true, true)
            .await
            .unwrap();
        promote(pool, "p", "m", "10", true, true, true)
            .await
            .unwrap();
        assert_eq!(rollback(pool, "p", "m").await.unwrap(), "2");
        assert_eq!(
            promoted_checksum(pool, "m").await.unwrap().as_deref(),
            Some("c2")
        );
    }

    fn model(model_id: &'static str, version: &'static str) -> ModelRecord<'static> {
        ModelRecord {
            model_id,
            version,
            model_type: "embedding",
            dataset_version: "ds",
            checksum: version,
            platforms: "macos-arm64",
            metrics_json: "{}",
        }
    }

    #[tokio::test]
    async fn a_suggestion_that_is_not_predicted_is_refused_and_nothing_is_replaced() {
        let (_dir, database) = db().await;
        let pool = database.pool();
        let mut rows = suggest(&[
            InputEdge {
                source: "a".into(),
                target: "hub".into(),
            },
            InputEdge {
                source: "b".into(),
                target: "hub".into(),
            },
        ]);
        replace_suggestions(pool, "p", &rows).await.unwrap();
        assert_eq!(suggestion_count(&database, "p").await, 1);
        rows[0].provenance = "extracted".into();
        let error = replace_suggestions(pool, "p", &rows).await.unwrap_err();
        assert_eq!(error, "graph suggestions must use provenance predicted");
        assert_eq!(suggestion_count(&database, "p").await, 1);
    }

    #[tokio::test]
    async fn hash_v1_cannot_be_registered_as_a_model() {
        let (_dir, database) = db().await;
        let error = register(database.pool(), &model("hash-v1", "1"))
            .await
            .unwrap_err();
        assert_eq!(error, "hash-v1 is not a neural registry model");
        assert!(list_models(database.pool()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn hash_v1_cannot_be_promoted() {
        let (_dir, database) = db().await;
        let error = promote(database.pool(), "p", "hash-v1", "1", true, true, true)
            .await
            .unwrap_err();
        assert_eq!(error, "hash-v1 is not a neural registry model");
    }

    async fn registered(pool: &SqlitePool, versions: &[&'static str]) {
        for version in versions {
            register(pool, &model("m", version)).await.unwrap();
        }
    }

    async fn active(pool: &SqlitePool) -> Option<String> {
        promoted_checksum(pool, "m").await.unwrap()
    }

    async fn audit_rows(pool: &SqlitePool) -> i64 {
        sqlx::query("SELECT COUNT(*) AS n FROM neural_audit")
            .fetch_one(pool)
            .await
            .unwrap()
            .get("n")
    }

    async fn fail_audit_inserts(pool: &SqlitePool) {
        sqlx::query(
            "CREATE TRIGGER fail_audit BEFORE INSERT ON neural_audit
             BEGIN SELECT RAISE(ABORT, 'audit unavailable'); END",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn a_failed_promote_leaves_the_previous_version_active() {
        let (_dir, database) = db().await;
        let pool = database.pool();
        registered(pool, &["1", "2"]).await;
        promote(pool, "p", "m", "1", true, true, true)
            .await
            .unwrap();
        fail_audit_inserts(pool).await;
        let error = promote(pool, "p", "m", "2", true, true, true)
            .await
            .unwrap_err();
        assert!(error.contains("audit unavailable"), "{error}");
        assert_eq!(active(pool).await.as_deref(), Some("1"));
        assert_eq!(audit_rows(pool).await, 1);
    }

    #[tokio::test]
    async fn a_failed_rollback_leaves_the_current_version_active() {
        let (_dir, database) = db().await;
        let pool = database.pool();
        registered(pool, &["1", "2"]).await;
        for version in ["1", "2"] {
            promote(pool, "p", "m", version, true, true, true)
                .await
                .unwrap();
        }
        fail_audit_inserts(pool).await;
        assert!(rollback(pool, "p", "m").await.is_err());
        assert_eq!(active(pool).await.as_deref(), Some("2"));
    }

    #[tokio::test]
    async fn rollback_after_promoting_a_version_again_selects_the_one_in_between() {
        let (_dir, database) = db().await;
        let pool = database.pool();
        registered(pool, &["1", "2"]).await;
        for version in ["1", "2", "1"] {
            promote(pool, "p", "m", version, true, true, true)
                .await
                .unwrap();
        }
        assert_eq!(rollback(pool, "p", "m").await.unwrap(), "2");
        assert_eq!(active(pool).await.as_deref(), Some("2"));
    }

    #[tokio::test]
    async fn rollback_never_returns_to_a_version_that_was_rolled_back() {
        let (_dir, database) = db().await;
        let pool = database.pool();
        registered(pool, &["1", "2", "3"]).await;
        for version in ["1", "2"] {
            promote(pool, "p", "m", version, true, true, true)
                .await
                .unwrap();
        }
        assert_eq!(rollback(pool, "p", "m").await.unwrap(), "1");
        promote(pool, "p", "m", "3", true, true, true)
            .await
            .unwrap();
        assert_eq!(rollback(pool, "p", "m").await.unwrap(), "1");
        assert_eq!(active(pool).await.as_deref(), Some("1"));
    }

    #[tokio::test]
    async fn rollback_uses_the_model_history_from_every_project() {
        // The promoted flag belongs to the model, not to a project.
        let (_dir, database) = db().await;
        let pool = database.pool();
        registered(pool, &["1", "2"]).await;
        promote(pool, "p", "m", "1", true, true, true)
            .await
            .unwrap();
        promote(pool, "q", "m", "2", true, true, true)
            .await
            .unwrap();
        assert_eq!(rollback(pool, "q", "m").await.unwrap(), "1");
    }

    #[tokio::test]
    async fn rollback_history_matches_the_model_id_exactly() {
        let (_dir, database) = db().await;
        let pool = database.pool();
        registered(pool, &["1", "2"]).await;
        register(pool, &model("m_x", "9")).await.unwrap();
        register(pool, &model("m_", "1")).await.unwrap();
        register(pool, &model("m_", "2")).await.unwrap();
        promote(pool, "p", "m_", "1", true, true, true)
            .await
            .unwrap();
        promote(pool, "p", "m_x", "9", true, true, true)
            .await
            .unwrap();
        promote(pool, "p", "m_", "2", true, true, true)
            .await
            .unwrap();
        assert_eq!(rollback(pool, "p", "m_").await.unwrap(), "1");
    }

    #[tokio::test]
    async fn promote_and_rollback_refuse_what_the_registry_does_not_hold() {
        let (_dir, database) = db().await;
        let pool = database.pool();
        let unknown = promote(pool, "p", "m", "1", true, true, true).await;
        assert_eq!(unknown.unwrap_err(), "model version is not registered");
        register(pool, &model("m", "1")).await.unwrap();
        let nothing_promoted = rollback(pool, "p", "m").await;
        assert_eq!(
            nothing_promoted.unwrap_err(),
            "no promoted model to roll back"
        );
        promote(pool, "p", "m", "1", true, true, true)
            .await
            .unwrap();
        let first = rollback(pool, "p", "m").await;
        assert_eq!(first.unwrap_err(), "no previous promoted version to select");
        assert_eq!(
            promoted_checksum(pool, "m").await.unwrap().as_deref(),
            Some("1")
        );
    }
}
