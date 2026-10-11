//! Opt-in, conflict-safe projection of authenticated replica memories.
//!
//! Project-state transport and selective export remain independent.
//! A replica only writes namespaced memory IDs; native/local records are never
//! replaced or deleted. Policies stay staged for explicit review.

use crate::{server_config::ServerConfig, server_snapshot::Snapshot};
use ax_memory::{embed, types::MemoryRow};
use sqlx::{Row, Sqlite, Transaction};
use std::collections::HashSet;

const MEMORY_PATH: &str = "state/memories.json";

fn stored_to_row(row: &sqlx::sqlite::SqliteRow) -> Result<MemoryRow, String> {
    let read = |name| -> Result<String, String> {
        row.try_get::<String, _>(name).map_err(|e| e.to_string())
    };
    Ok(MemoryRow {
        id: read("id")?,
        kind: read("kind")?,
        title: read("title")?,
        body: read("body")?,
        tags: serde_json::from_str(&read("tags")?).map_err(|e| e.to_string())?,
        files: serde_json::from_str(&read("files")?).map_err(|e| e.to_string())?,
        confidence: row.try_get("confidence").map_err(|e| e.to_string())?,
        source: read("source")?,
        enabled: row.try_get::<i64, _>("enabled").map_err(|e| e.to_string())? != 0,
        created_at: row.try_get("created_at").map_err(|e| e.to_string())?,
        updated_at: row.try_get("updated_at").map_err(|e| e.to_string())?,
    })
}

async fn current_memory(
    tx: &mut Transaction<'_, Sqlite>,
    id: &str,
) -> Result<Option<MemoryRow>, String> {
    let row = sqlx::query(
        "SELECT id, kind, title, body, tags, files, confidence, source, enabled, created_at, updated_at FROM memories WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    row.as_ref().map(stored_to_row).transpose()
}

async fn write_memory(
    tx: &mut Transaction<'_, Sqlite>,
    memory: &MemoryRow,
    exists: bool,
) -> Result<(), String> {
    let tags = serde_json::to_string(&memory.tags).map_err(|e| e.to_string())?;
    let files = serde_json::to_string(&memory.files).map_err(|e| e.to_string())?;
    let embedding = embed::embedding_to_blob(&embed::embed_text(
        &format!("{} {} {}", memory.title, memory.body, memory.tags.join(" ")),
    ));
    let statement = if exists {
        "UPDATE memories SET kind=?, title=?, body=?, tags=?, files=?, confidence=?, source=?, created_at=?, updated_at=?, embedding=?, enabled=? WHERE id=?"
    } else {
        "INSERT INTO memories (kind,title,body,tags,files,confidence,source,created_at,updated_at,embedding,enabled,id) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)"
    };
    sqlx::query(statement)
        .bind(&memory.kind)
        .bind(&memory.title)
        .bind(&memory.body)
        .bind(tags)
        .bind(files)
        .bind(memory.confidence)
        .bind(&memory.source)
        .bind(memory.created_at)
        .bind(memory.updated_at)
        .bind(embedding)
        .bind(i64::from(memory.enabled))
        .bind(&memory.id)
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Default)]
pub struct ApplyResult {
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub removed: usize,
    pub conflicts: usize,
}

/// Project a validated mirror into the local memory vault. The exact last
/// applied version is recorded in a ledger; locally edited records are never
/// overwritten by subsequent automatic syncs. Withdrawal deletes only a
/// record that still matches the last imported version.
pub async fn apply(config: &ServerConfig, snapshot: &Snapshot) -> Result<ApplyResult, String> {
    crate::server_snapshot::validate(snapshot, &config.cluster_id, &snapshot.project_id)?;
    let project = config
        .projects
        .iter()
        .find(|p| p.id == snapshot.project_id)
        .ok_or("project not registered")?;
    let rows: Vec<MemoryRow> = serde_json::from_str(
        snapshot.files.get(MEMORY_PATH).ok_or("snapshot has no memories")?,
    )
    .map_err(|e| e.to_string())?;
    let ax = ax_core::Ax::open(&project.path).await.map_err(|e| e.to_string())?;
    let mut tx = ax.db_pool().begin().await.map_err(|e| e.to_string())?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS ax_replica_memory_ledger (source TEXT NOT NULL, remote_id TEXT NOT NULL, local_id TEXT NOT NULL, last_applied TEXT NOT NULL, PRIMARY KEY(source, remote_id))",
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    let source = format!(
        "ax-replica:{}:{}:{}",
        config.cluster_id, snapshot.project_id, snapshot.primary_node_id
    );
    let mut report = ApplyResult::default();
    let mut remote_ids = HashSet::new();

    for row in &rows {
        remote_ids.insert(row.id.clone());
        let local_id = format!(
            "replica:{}",
            blake3::hash(format!("{}\\0{}", source, row.id).as_bytes()).to_hex()
        );
        let mut local = row.clone();
        local.id = local_id.clone();
        local.source = source.clone();

        let previous: Option<(String, String)> = sqlx::query_as(
            "SELECT local_id, last_applied FROM ax_replica_memory_ledger WHERE source=? AND remote_id=?",
        )
        .bind(&source)
        .bind(&row.id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        let current = current_memory(&mut tx, &local_id).await?;
        if let Some((owned_id, stored)) = &previous {
            if owned_id != &local_id {
                report.conflicts += 1;
                continue;
            }
            let before: MemoryRow = serde_json::from_str(stored)
                .map_err(|e| format!("invalid replica ledger: {e}"))?;
            if current.as_ref().is_none_or(|actual| {
                serde_json::to_value(actual).ok() != serde_json::to_value(&before).ok()
            }) {
                report.conflicts += 1;
                continue;
            }
            if serde_json::to_value(&local).ok() == serde_json::to_value(&before).ok() {
                report.unchanged += 1;
                continue;
            }
            write_memory(&mut tx, &local, true).await?;
            report.updated += 1;
        } else if current.is_some() {
            report.conflicts += 1;
            continue;
        } else {
            write_memory(&mut tx, &local, false).await?;
            report.inserted += 1;
        }

        sqlx::query(
            "INSERT INTO ax_replica_memory_ledger (source,remote_id,local_id,last_applied) VALUES (?,?,?,?) ON CONFLICT (source,remote_id) DO UPDATE SET local_id=excluded.local_id,last_applied=excluded.last_applied",
        )
        .bind(&source)
        .bind(&row.id)
        .bind(&local_id)
        .bind(serde_json::to_string(&local).map_err(|e| e.to_string())?)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }

    let known: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT remote_id, local_id, last_applied FROM ax_replica_memory_ledger WHERE source=?",
    )
    .bind(&source)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    for (remote_id, local_id, previous) in known {
        if remote_ids.contains(&remote_id) {
            continue;
        }
        let previous: MemoryRow = serde_json::from_str(&previous)
            .map_err(|e| format!("invalid replica ledger: {e}"))?;
        let current = current_memory(&mut tx, &local_id).await?;
        if current.as_ref().is_some_and(|actual| {
            serde_json::to_value(actual).ok() == serde_json::to_value(&previous).ok()
        }) {
            sqlx::query("DELETE FROM memories WHERE id=?")
                .bind(&local_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
            report.removed += 1;
        } else {
            report.conflicts += 1;
            continue;
        }
        sqlx::query("DELETE FROM ax_replica_memory_ledger WHERE source=? AND remote_id=?")
            .bind(&source)
            .bind(&remote_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projections_use_deterministic_namespaces() {
        let a = format!("{}\\0{}", "ax-replica:one", "memory");
        let b = format!("{}\\0{}", "ax-replica:two", "memory");
        assert_ne!(blake3::hash(a.as_bytes()), blake3::hash(b.as_bytes()));
    }
}
