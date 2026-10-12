//! Project-scoped working context, durable conversations and structured logs.
//! Only rows with explicit project ownership are exported from the global usage store.
use sha2::{Digest, Sha256};
use sqlx::{Acquire, Row, SqlitePool};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
const TABLES: &[&str] = &[
    "mcp_call_log",
    "mcp_session_index",
    "mcp_context_cache",
    "mcp_reuse_cache",
    "mcp_working_context",
    "ax_durable_scope",
    "ax_durable_conversation",
    "ax_durable_entry",
    "ax_durable_document",
    "ax_durable_task",
    "ax_durable_hook",
    "agent_usage_event",
    "agent_session_log",
];
fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}
pub struct Export {
    pub path: PathBuf,
    pub rows: u64,
    _directory: tempfile::TempDir,
}
fn hashed(origin: &str, value: &str) -> String {
    hex::encode(Sha256::digest(format!("{origin}\0{value}").as_bytes()))
}
fn mapped(origin: &str, kind: &str, value: &str) -> String {
    let digest = hashed(origin, value);
    match kind {
        "cache" => format!("cc_replica_{}", &digest[..48]),
        "conversation" => format!("dcp_replica_{}", &digest[..48]),
        _ => format!("replica_{}", &digest[..48]),
    }
}
async fn ledger(pool: &SqlitePool) -> Result<(), String> {
    sqlx::query("CREATE TABLE IF NOT EXISTS ax_replica_history_ids(origin TEXT NOT NULL,table_name TEXT NOT NULL,remote_id INTEGER NOT NULL,local_id INTEGER NOT NULL,PRIMARY KEY(origin,table_name,remote_id),UNIQUE(table_name,local_id))").execute(pool).await.map_err(err)?;
    Ok(())
}
async fn columns(pool: &SqlitePool, table: &str) -> Result<Vec<String>, String> {
    if !TABLES.contains(&table) {
        return Err("Unknown history table".into());
    }
    Ok(sqlx::query(&format!("PRAGMA table_info(\"{table}\")"))
        .fetch_all(pool)
        .await
        .map_err(err)?
        .iter()
        .map(|r| r.get("name"))
        .collect())
}
fn conversations() -> String {
    "SELECT conversation FROM native.ax_durable_scope WHERE project=?1 AND session NOT GLOB 'replica_*'".into()
}
fn filter(table: &str) -> String {
    let owned = conversations();
    let sessions="SELECT session FROM native.ax_durable_scope WHERE project=?1 AND session NOT GLOB 'replica_*' UNION SELECT substr(scope,1,length(scope)-length(?1)-1) FROM native.mcp_working_context WHERE substr(scope,-length(?1))=?1 AND scope NOT GLOB 'replica_*'";
    let core=match table {
        "mcp_call_log"|"agent_usage_event"=>"project=?1".into(),
        "mcp_context_cache"=>"project=?1 AND id NOT GLOB 'cc_replica_*'".into(),
        "mcp_working_context"=>"substr(scope,-length(?1))=?1 AND scope NOT GLOB 'replica_*'".into(),
        "mcp_reuse_cache"=>"substr(conversation,-length(?1))=?1 AND conversation NOT GLOB 'replica_*'".into(),
        "ax_durable_scope"=>"project=?1 AND session NOT GLOB 'replica_*'".into(),
        "ax_durable_conversation"=>format!("id IN ({owned})"),
        "ax_durable_entry"|"ax_durable_document"|"ax_durable_task"|"ax_durable_hook"=>format!("conversation_id IN ({owned})"),
        "mcp_session_index"=>format!("session_id IN ({sessions}) OR cache_id IN (SELECT id FROM native.mcp_context_cache WHERE project=?1 AND id NOT GLOB 'cc_replica_*')"),
        "agent_session_log"=>"(agent,session_id) IN (SELECT agent,session_id FROM native.agent_usage_event WHERE project=?1)".into(),
        _=>"0".into(),
    };
    if numeric(table) {
        format!("({core}) AND NOT EXISTS (SELECT 1 FROM native.ax_replica_history_ids l WHERE l.table_name='{table}' AND l.local_id=native.\"{table}\".id)")
    } else {
        core
    }
}
fn numeric(table: &str) -> bool {
    matches!(
        table,
        "mcp_call_log"
            | "mcp_session_index"
            | "ax_durable_entry"
            | "agent_usage_event"
            | "agent_session_log"
    )
}
fn portable(table: &str, column: &str) -> String {
    if column == "project" {
        return "'project'".into();
    }
    if (table == "mcp_working_context" && column == "scope")
        || (table == "mcp_reuse_cache" && column == "conversation")
    {
        return format!("substr(\"{column}\",1,length(\"{column}\")-length(?1)-1)");
    }
    if table == "ax_durable_conversation" && column == "parent_id" {
        return format!(
            "CASE WHEN parent_id IN ({}) THEN parent_id ELSE NULL END",
            conversations()
        );
    }
    format!("\"{column}\"")
}
pub async fn export(root: &Path) -> Result<Export, String> {
    let native = crate::store::open_pool().await.map_err(err)?;
    export_with(root, &native, &crate::store::usage_db_path()).await
}
async fn export_with(
    root: &Path,
    native: &SqlitePool,
    native_path: &Path,
) -> Result<Export, String> {
    ledger(native).await?;
    let directory = tempfile::tempdir_in(root.join(".ax/server/datasets")).map_err(err)?;
    let path = directory.path().join("history.db");
    let output = crate::store::open_pool_at(&path).await.map_err(err)?;
    let project = crate::context_cache::project_scope(root);
    let mut names = BTreeMap::new();
    for table in TABLES {
        names.insert(*table, columns(&output, table).await?);
    }
    let mut conn = output.acquire().await.map_err(err)?;
    sqlx::query("ATTACH DATABASE ? AS native")
        .bind(native_path.to_string_lossy().as_ref())
        .execute(&mut *conn)
        .await
        .map_err(err)?;
    let mut tx = conn.begin().await.map_err(err)?;
    for table in TABLES {
        let into = names[table]
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(",");
        let select = names[table]
            .iter()
            .map(|c| portable(table, c))
            .collect::<Vec<_>>()
            .join(",");
        sqlx::query(&format!("INSERT INTO main.\"{table}\" ({into}) SELECT {select} FROM native.\"{table}\" WHERE {} ORDER BY rowid",filter(table))).bind(&project).execute(&mut *tx).await.map_err(err)?;
    }
    tx.commit().await.map_err(err)?;
    sqlx::query("DETACH DATABASE native")
        .execute(&mut *conn)
        .await
        .map_err(err)?;
    sqlx::query("VACUUM")
        .execute(&mut *conn)
        .await
        .map_err(err)?;
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut *conn)
        .await
        .map_err(err)?;
    drop(conn);
    let rows = row_count(&output).await?;
    output.close().await;
    Ok(Export {
        path,
        rows,
        _directory: directory,
    })
}
async fn row_count(pool: &SqlitePool) -> Result<u64, String> {
    let mut total = 0u64;
    for table in TABLES {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM \"{table}\""))
            .fetch_one(pool)
            .await
            .map_err(err)?;
        total = total
            .checked_add(count as u64)
            .ok_or("History count overflow")?;
    }
    Ok(total)
}
pub async fn verify(path: &Path, expected_rows: u64) -> Result<(), String> {
    let expected = crate::store::open_pool().await.map_err(err)?;
    verify_with(path, expected_rows, &expected).await
}
async fn verify_with(path: &Path, expected_rows: u64, expected: &SqlitePool) -> Result<(), String> {
    let incoming = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(path)
                .read_only(true),
        )
        .await
        .map_err(err)?;
    sqlx::query("PRAGMA trusted_schema=OFF")
        .execute(&incoming)
        .await
        .map_err(err)?;
    let checks: Vec<String> = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_all(&incoming)
        .await
        .map_err(err)?;
    if checks != ["ok"] {
        return Err("Received conversation history is corrupt".into());
    }
    // A fresh local schema defines the entire accepted data contract.
    for table in TABLES {
        let kind: Option<(String, i64)> =
            sqlx::query_as("SELECT type,rootpage FROM sqlite_master WHERE name=?")
                .bind(table)
                .fetch_optional(&incoming)
                .await
                .map_err(err)?;
        if !kind.is_some_and(|(kind, page)| kind == "table" && page > 0)
            || ax_db::replication::table_contract(&incoming, table)
                .await
                .map_err(err)?
                != ax_db::replication::table_contract(expected, table)
                    .await
                    .map_err(err)?
        {
            return Err(format!("Received history schema mismatch: {table}"));
        }
        if columns(&incoming, table)
            .await?
            .iter()
            .any(|c| c == "project")
        {
            let foreign: i64 = sqlx::query_scalar(&format!(
                "SELECT COUNT(*) FROM \"{table}\" WHERE project IS NULL OR project!='project'"
            ))
            .fetch_one(&incoming)
            .await
            .map_err(err)?;
            if foreign != 0 {
                return Err("History contains unrelated project scopes".into());
            }
        }
    }
    if row_count(&incoming).await? != expected_rows || expected_rows > 1_000_000 {
        return Err("History row count mismatch or capacity exceeded".into());
    }
    incoming.close().await;
    Ok(())
}
fn string_kind(table: &str, column: &str) -> Option<&'static str> {
    if column == "cache_id" || (table == "mcp_context_cache" && column == "id") {
        Some("cache")
    } else if column == "conversation_id"
        || (table == "ax_durable_conversation" && matches!(column, "id" | "parent_id"))
        || (table == "ax_durable_scope" && column == "conversation")
    {
        Some("conversation")
    } else if column == "session_id"
        || (table == "ax_durable_scope" && column == "session")
        || (table == "ax_durable_task" && column == "id")
        || (table == "mcp_reuse_cache" && column == "reuse_key")
    {
        Some("session")
    } else {
        None
    }
}
pub async fn project(root: &Path, origin: &str, path: &Path, rows: u64) -> Result<(), String> {
    let pool = crate::store::open_pool().await.map_err(err)?;
    project_with(root, origin, path, rows, &pool).await
}
async fn project_with(
    root: &Path,
    origin: &str,
    path: &Path,
    rows: u64,
    pool: &SqlitePool,
) -> Result<(), String> {
    if !ax_db::replication::valid_id(origin) {
        return Err("Invalid history origin".into());
    }
    verify_with(path, rows, pool).await?;
    ledger(pool).await?;
    let mut names = BTreeMap::new();
    for table in TABLES {
        names.insert(*table, columns(pool, table).await?);
    }
    let mut conn = pool.acquire().await.map_err(err)?;
    sqlx::query("ATTACH DATABASE ? AS received_history")
        .bind(path.to_string_lossy().as_ref())
        .execute(&mut *conn)
        .await
        .map_err(err)?;
    sqlx::query("CREATE TEMP TABLE IF NOT EXISTS ax_replica_strings(kind TEXT NOT NULL,remote TEXT NOT NULL,local TEXT NOT NULL,PRIMARY KEY(kind,remote))").execute(&mut *conn).await.map_err(err)?;
    let mut tx = conn.begin().await.map_err(err)?;
    sqlx::query("DELETE FROM ax_replica_strings")
        .execute(&mut *tx)
        .await
        .map_err(err)?;
    for table in TABLES {
        for column in &names[table] {
            let kind = string_kind(table, column).or_else(|| {
                ((table == &"mcp_working_context" && column == "scope")
                    || (table == &"mcp_reuse_cache" && column == "conversation"))
                    .then_some("session")
            });
            if let Some(kind) = kind {
                let values:Vec<String>=sqlx::query_scalar(&format!("SELECT DISTINCT \"{column}\" FROM received_history.\"{table}\" WHERE \"{column}\" IS NOT NULL")).fetch_all(&mut *tx).await.map_err(err)?;
                for value in values {
                    sqlx::query("INSERT OR IGNORE INTO ax_replica_strings VALUES (?,?,?)")
                        .bind(kind)
                        .bind(&value)
                        .bind(mapped(origin, kind, &value))
                        .execute(&mut *tx)
                        .await
                        .map_err(err)?;
                }
            }
        }
        if numeric(table) {
            let ids: Vec<i64> = sqlx::query_scalar(&format!(
                "SELECT id FROM received_history.\"{table}\" ORDER BY id"
            ))
            .fetch_all(&mut *tx)
            .await
            .map_err(err)?;
            let mut next: i64 = sqlx::query_scalar(&format!(
                "SELECT COALESCE(MAX(id),0)+1 FROM main.\"{table}\""
            ))
            .fetch_one(&mut *tx)
            .await
            .map_err(err)?;
            let reserved: Option<i64> = sqlx::query_scalar(
                "SELECT MAX(local_id)+1 FROM ax_replica_history_ids WHERE table_name=?",
            )
            .bind(table)
            .fetch_one(&mut *tx)
            .await
            .map_err(err)?;
            next = next.max(reserved.unwrap_or(1));
            for id in ids {
                let existing:Option<i64>=sqlx::query_scalar("SELECT local_id FROM ax_replica_history_ids WHERE origin=? AND table_name=? AND remote_id=?").bind(origin).bind(table).bind(id).fetch_optional(&mut *tx).await.map_err(err)?;
                if existing.is_none() {
                    sqlx::query("INSERT INTO ax_replica_history_ids VALUES (?,?,?,?)")
                        .bind(origin)
                        .bind(table)
                        .bind(id)
                        .bind(next)
                        .execute(&mut *tx)
                        .await
                        .map_err(err)?;
                    next += 1;
                }
            }
        }
    }
    let project = crate::context_cache::project_scope(root);
    let quote = |v: &str| v.replace('\'', "''");
    for table in TABLES {
        let into = names[table]
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(",");
        let select=names[table].iter().map(|c| {
            let column=format!("r.\"{c}\"");
            if c=="project" {return format!("'{}'",quote(&project));}
            if (table==&"mcp_working_context" && c=="scope") || (table==&"mcp_reuse_cache" && c=="conversation") {return format!("(SELECT local FROM ax_replica_strings WHERE kind='session' AND remote={column})||char(31)||'{}'",quote(&project));}
            if c=="index_fingerprint" {return format!("'replica:{}'",quote(origin));}
            if let Some(kind)=string_kind(table,c) {return format!("CASE WHEN {column} IS NULL THEN NULL ELSE (SELECT local FROM ax_replica_strings WHERE kind='{kind}' AND remote={column}) END");}
            if (numeric(table) && c=="id") || (table==&"ax_durable_entry" && c=="head") || (table==&"ax_durable_conversation" && c=="fork_entry") {
                let source=if c=="id" {*table} else {"ax_durable_entry"};return format!("(SELECT local_id FROM ax_replica_history_ids WHERE origin='{}' AND table_name='{source}' AND remote_id={column})",quote(origin));
            }
            column
        }).collect::<Vec<_>>().join(",");
        sqlx::query(&format!("INSERT OR REPLACE INTO main.\"{table}\" ({into}) SELECT {select} FROM received_history.\"{table}\" r ORDER BY rowid")).execute(&mut *tx).await.map_err(err)?;
    }
    tx.commit().await.map_err(err)?;
    sqlx::query("DETACH DATABASE received_history")
        .execute(&mut *conn)
        .await
        .map_err(err)?;
    Ok(())
}

/// Keep accepted history available to new replicas after native cache/context
/// cleanup. Current values replace matching keys; omission cannot erase history.
pub async fn retain(
    root: &Path,
    old: &ax_db::replication::Dataset,
    new: &mut ax_db::replication::Dataset,
) -> Result<(), String> {
    let (Some(before), Some(after)) = (&old.history, &new.history) else {
        return Err("Missing retained project history".into());
    };
    if before.digest == after.digest {
        return Ok(());
    }
    let directory = tempfile::tempdir_in(root.join(".ax/server/datasets")).map_err(err)?;
    let old_path = directory.path().join("before.db");
    let new_path = directory.path().join("after.db");
    let output_path = directory.path().join("merged.db");
    let input_root = root.to_owned();
    let old_file = before.clone();
    let new_file = after.clone();
    let first = old_path.clone();
    let last = new_path.clone();
    tokio::task::spawn_blocking(move || {
        ax_db::replication::assemble(&input_root, &old_file, &first)?;
        ax_db::replication::assemble(&input_root, &new_file, &last)
    })
    .await
    .map_err(err)?
    .map_err(err)?;
    verify(&old_path, old.history_rows).await?;
    verify(&new_path, new.history_rows).await?;
    let output = crate::store::open_pool_at(&output_path)
        .await
        .map_err(err)?;
    let mut names = BTreeMap::new();
    for table in TABLES {
        names.insert(*table, columns(&output, table).await?);
    }
    let mut conn = output.acquire().await.map_err(err)?;
    sqlx::query("ATTACH DATABASE ? AS previous")
        .bind(old_path.to_string_lossy().as_ref())
        .execute(&mut *conn)
        .await
        .map_err(err)?;
    sqlx::query("ATTACH DATABASE ? AS current")
        .bind(new_path.to_string_lossy().as_ref())
        .execute(&mut *conn)
        .await
        .map_err(err)?;
    let mut tx = conn.begin().await.map_err(err)?;
    for source in ["previous", "current"] {
        for table in TABLES {
            let fields = names[table]
                .iter()
                .map(|n| format!("\"{n}\""))
                .collect::<Vec<_>>()
                .join(",");
            sqlx::query(&format!("INSERT OR REPLACE INTO main.\"{table}\" ({fields}) SELECT {fields} FROM {source}.\"{table}\" ORDER BY rowid")).execute(&mut *tx).await.map_err(err)?;
        }
    }
    tx.commit().await.map_err(err)?;
    sqlx::query("DETACH DATABASE previous")
        .execute(&mut *conn)
        .await
        .map_err(err)?;
    sqlx::query("DETACH DATABASE current")
        .execute(&mut *conn)
        .await
        .map_err(err)?;
    sqlx::query("VACUUM")
        .execute(&mut *conn)
        .await
        .map_err(err)?;
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut *conn)
        .await
        .map_err(err)?;
    drop(conn);
    let rows = row_count(&output).await?;
    if rows > 1_000_000 {
        return Err("Retained history capacity reached; previous history is preserved".into());
    }
    output.close().await;
    let input_root = root.to_owned();
    let path = output_path.clone();
    let manifest =
        tokio::task::spawn_blocking(move || ax_db::replication::file_manifest(&input_root, &path))
            .await
            .map_err(err)?
            .map_err(err)?;
    new.history = Some(manifest);
    new.history_rows = rows;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn complete_project_history_is_scoped_persistent_queryable_and_never_echoed() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(source.path().join(".ax/server/datasets")).unwrap();
        std::fs::create_dir_all(target.path().join(".ax/server/datasets")).unwrap();
        let path = source.path().join("usage.db");
        let native = crate::store::open_pool_at(&path).await.unwrap();
        let destination = target.path().join("usage.db");
        let received = crate::store::open_pool_at(&destination).await.unwrap();
        let scope = crate::project_scope(source.path());
        sqlx::query("INSERT INTO mcp_call_log(tool,project,response_chars,response_tokens_est,ok,created_at) VALUES ('ax_node',?,10,2,1,1),('secret','/unrelated',10,2,1,1)").bind(&scope).execute(&native).await.unwrap();
        sqlx::query("INSERT INTO mcp_context_cache VALUES ('cc_native','ax_node','source body',3,1,9999999999999,?)").bind(&scope).execute(&native).await.unwrap();
        sqlx::query("INSERT INTO mcp_working_context(scope,objective,content_hash,updated_at) VALUES (?,'Remember our decision','hash',1)").bind(format!("session\u{1f}{scope}")).execute(&native).await.unwrap();
        sqlx::query("INSERT INTO ax_durable_scope VALUES (?,'session','conversation')")
            .bind(&scope)
            .execute(&native)
            .await
            .unwrap();
        sqlx::query("INSERT INTO ax_durable_conversation VALUES ('conversation',NULL,NULL,1)")
            .execute(&native)
            .await
            .unwrap();
        sqlx::query("INSERT INTO ax_durable_entry VALUES (8,'conversation','message','Existing transcript',NULL,1),(9,'conversation','summary','Summary',8,2)").execute(&native).await.unwrap();
        sqlx::query("INSERT INTO ax_durable_document VALUES ('conversation','spec','Our project specification',1)").execute(&native).await.unwrap();
        sqlx::query("INSERT INTO ax_durable_task VALUES ('task','conversation','implementation','running','test','checkpoint',NULL,1)").execute(&native).await.unwrap();
        sqlx::query("INSERT INTO mcp_session_index(session_id,tool,summary,original_tokens,cache_id,created_at) VALUES ('session','ax_node','Cached source',3,'cc_native',1)").execute(&native).await.unwrap();
        let exported = export_with(source.path(), &native, &path).await.unwrap();
        assert_eq!(exported.rows, 10);
        verify_with(&exported.path, exported.rows, &received)
            .await
            .unwrap();
        for _ in 0..2 {
            project_with(
                target.path(),
                "a_1",
                &exported.path,
                exported.rows,
                &received,
            )
            .await
            .unwrap();
        }
        let calls: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mcp_call_log")
            .fetch_one(&received)
            .await
            .unwrap();
        assert_eq!(calls, 1);
        let session = mapped("a_1", "session", "session");
        let key = format!("{session}\u{1f}{}", crate::project_scope(target.path()));
        let objective: String =
            sqlx::query_scalar("SELECT objective FROM mcp_working_context WHERE scope=?")
                .bind(&key)
                .fetch_one(&received)
                .await
                .unwrap();
        assert_eq!(objective, "Remember our decision");
        let conversation: String = sqlx::query_scalar(
            "SELECT conversation FROM ax_durable_scope WHERE session=? AND project=?",
        )
        .bind(&session)
        .bind(crate::project_scope(target.path()))
        .fetch_one(&received)
        .await
        .unwrap();
        let body: String =
            sqlx::query_scalar("SELECT body FROM ax_durable_document WHERE conversation_id=?")
                .bind(&conversation)
                .fetch_one(&received)
                .await
                .unwrap();
        assert_eq!(body, "Our project specification");
        let linked:i64=sqlx::query_scalar("SELECT COUNT(*) FROM ax_durable_entry a JOIN ax_durable_entry b ON a.head=b.id WHERE a.body='Summary' AND b.body='Existing transcript'").fetch_one(&received).await.unwrap();
        assert_eq!(linked, 1);
        crate::working_context::apply_with(
            &received,
            target.path(),
            "new-local-chat",
            &serde_json::json!({"action":"add","facts":["New local fact"]}),
            "fresh",
            true,
        )
        .await
        .unwrap();
        let retained: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM mcp_working_context WHERE scope=?")
                .bind(&key)
                .fetch_one(&received)
                .await
                .unwrap();
        assert_eq!(retained, 1);
        // Remove the new native note before checking that received history is not fed back.
        sqlx::query("DELETE FROM mcp_working_context WHERE scope NOT GLOB 'replica_*'")
            .execute(&received)
            .await
            .unwrap();
        let echo = export_with(target.path(), &received, &destination)
            .await
            .unwrap();
        assert_eq!(echo.rows, 0);
        received.close().await;
        let reopened = crate::store::open_pool_at(&destination).await.unwrap();
        let tasks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ax_durable_task")
            .fetch_one(&reopened)
            .await
            .unwrap();
        assert_eq!(tasks, 1);
        assert!(verify_with(&exported.path, exported.rows + 1, &reopened)
            .await
            .is_err());
    }
}
