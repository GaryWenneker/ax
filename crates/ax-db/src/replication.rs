//! Content-addressed, project-scoped graph and history replication.
//! Incoming databases are data, never schema or executable migrations.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use std::{collections::{BTreeMap, BTreeSet}, io::{Read, Write}, path::{Path, PathBuf}};

pub const DATA_PATH: &str = "state/datasets.json";
pub const PREFIX: &str = "ax-replica/";
pub const CHUNK_BYTES: usize = 512 * 1024;
pub const MAX_DATA_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const TABLES: &[&str] = &[
    "nodes", "edges", "files", "file_contents", "unresolved_refs", "node_communities", "project_metadata",
    "agent_sessions", "agent_turns", "agent_tool_calls", "agent_model_calls", "agent_optimizations", "agent_context_cache",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileManifest { pub digest: String, pub bytes: u64, pub chunks: Vec<String> }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Dataset {
    pub origin: String, pub graph: FileManifest, pub logs: BTreeMap<String, FileManifest>,
    pub nodes: u64, pub edges: u64, pub files: u64, pub source_bytes: u64, pub agent_sessions: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub origin: String, pub nodes: i64, pub edges: i64, pub files: i64, pub source_bytes: i64,
    pub log_files: i64, pub log_bytes: i64, pub agent_sessions: i64, pub updated_at: i64,
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn valid_hash(value: &str) -> bool { value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
pub fn is_replica(value: &str) -> bool { value.starts_with(PREFIX) }
fn safe_dir(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(m) => ensure!(m.is_dir() && !m.file_type().is_symlink(), "unsafe replication directory"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => { std::fs::create_dir(path)?; },
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
fn directory(root: &Path, kind: &str) -> Result<PathBuf> {
    ensure!(matches!(kind, "blobs" | "datasets" | "replicated-logs"), "unknown data directory");
    let ax = root.join(".ax"); safe_dir(&ax)?;
    let server = ax.join("server"); safe_dir(&server)?;
    let path = server.join(kind); safe_dir(&path)?; Ok(path)
}
fn regular(path: &Path, limit: u64) -> Result<std::fs::Metadata> {
    let m = std::fs::symlink_metadata(path)?;
    ensure!(m.is_file() && !m.file_type().is_symlink() && m.len() <= limit, "unsafe or oversized replicated file"); Ok(m)
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() { regular(path, MAX_DATA_BYTES)?; }
    let mut f = tempfile::NamedTempFile::new_in(path.parent().context("missing parent")?)?;
    f.write_all(bytes)?; f.as_file().sync_all()?; f.persist(path).map_err(|e| e.error)?; Ok(())
}
pub fn read_chunk(root: &Path, hash: &str) -> Result<Vec<u8>> {
    ensure!(valid_hash(hash), "invalid chunk hash");
    let path = directory(root, "blobs")?.join(hash); regular(&path, CHUNK_BYTES as u64)?;
    let bytes = std::fs::read(path)?;
    ensure!(!bytes.is_empty() && blake3::hash(&bytes).to_hex().as_str() == hash, "chunk checksum mismatch"); Ok(bytes)
}
pub fn store_chunk(root: &Path, hash: &str, bytes: &[u8]) -> Result<()> {
    ensure!(valid_hash(hash) && !bytes.is_empty() && bytes.len() <= CHUNK_BYTES, "invalid chunk size or hash");
    ensure!(blake3::hash(bytes).to_hex().as_str() == hash, "chunk checksum mismatch");
    let dir = directory(root, "blobs")?; let path = dir.join(hash);
    if path.exists() && read_chunk(root, hash).is_ok() { return Ok(()); }
    let mut size = 0u64;
    for item in std::fs::read_dir(&dir)? { let item = item?; let m = item.file_type()?;
        if m.is_file() { size = size.checked_add(item.metadata()?.len()).context("cache size overflow")?; }
    }
    ensure!(size.saturating_add(bytes.len() as u64) <= MAX_DATA_BYTES, "replication cache capacity reached");
    atomic(&path, bytes)
}
fn file_manifest(root: &Path, path: &Path) -> Result<FileManifest> {
    regular(path, MAX_DATA_BYTES)?; let mut input = std::fs::File::open(path)?;
    let mut buffer = vec![0; CHUNK_BYTES]; let mut digest = blake3::Hasher::new(); let mut chunks = Vec::new(); let mut bytes = 0u64;
    loop {
        let mut used = 0;
        while used < buffer.len() { let count = input.read(&mut buffer[used..])?; if count == 0 { break; } used += count; }
        if used == 0 { break; }
        bytes = bytes.checked_add(used as u64).context("file size overflow")?;
        ensure!(bytes <= MAX_DATA_BYTES, "replicated file too large");
        let part = &buffer[..used]; let hash = blake3::hash(part).to_hex().to_string();
        store_chunk(root, &hash, part)?; digest.update(part); chunks.push(hash);
        // Capture the first EOF. An actively appended log must not produce two short chunks.
        if used < buffer.len() { break; }
    }
    Ok(FileManifest { digest: digest.finalize().to_hex().to_string(), bytes, chunks })
}
fn validate_file(item: &FileManifest) -> Result<()> {
    ensure!(valid_hash(&item.digest) && item.bytes <= MAX_DATA_BYTES, "invalid file manifest");
    ensure!(item.chunks.len() as u64 == item.bytes.div_ceil(CHUNK_BYTES as u64) && item.chunks.iter().all(|c| valid_hash(c)), "invalid chunk manifest"); Ok(())
}
pub fn valid_log(name: &str) -> bool {
    if name == "mcp-verbose.log" { return true; }
    let Some(rest) = name.strip_prefix("mcp-verbose-") else { return false; };
    let Some(rest) = rest.strip_suffix(".log") else { return false; };
    let (date, segment) = rest.split_once('.').map_or((rest, None), |(d, h)| (d, Some(h)));
    date.len() == 10 && date.as_bytes()[4] == b'-' && date.as_bytes()[7] == b'-'
        && date.bytes().enumerate().all(|(i,b)| i == 4 || i == 7 || b.is_ascii_digit())
        && segment.is_none_or(valid_hash)
}
pub fn validate(items: &[Dataset]) -> Result<()> {
    ensure!(items.len() <= 64, "too many dataset origins"); let mut origins = BTreeSet::new();
    let mut total = 0u64;
    for item in items {
        ensure!(valid_id(&item.origin) && origins.insert(&item.origin), "invalid or duplicate origin");
        validate_file(&item.graph)?; ensure!(item.graph.bytes >= 4096, "missing graph database");
        ensure!(item.logs.len() <= 3660, "too many log files"); total = total.checked_add(item.graph.bytes).context("data size overflow")?;
        for (name, log) in &item.logs { ensure!(valid_log(name), "invalid log name"); validate_file(log)?; total = total.checked_add(log.bytes).context("data size overflow")?; }
    }
    ensure!(total <= MAX_DATA_BYTES, "project datasets exceed replication capacity"); Ok(())
}
pub fn hashes(items: &[Dataset]) -> BTreeSet<String> {
    items.iter().flat_map(|d| std::iter::once(&d.graph).chain(d.logs.values())).flat_map(|f| f.chunks.iter().cloned()).collect()
}
fn assemble(root: &Path, manifest: &FileManifest, path: &Path) -> Result<()> {
    validate_file(manifest)?; let mut output = tempfile::NamedTempFile::new_in(path.parent().context("missing parent")?)?;
    let mut hash = blake3::Hasher::new(); let mut size = 0u64;
    for (i, part) in manifest.chunks.iter().enumerate() {
        let bytes = read_chunk(root, part)?;
        let expected = if i + 1 == manifest.chunks.len() { manifest.bytes - size } else { CHUNK_BYTES as u64 };
        ensure!(bytes.len() as u64 == expected, "invalid chunk length");
        size += bytes.len() as u64; hash.update(&bytes); output.write_all(&bytes)?;
    }
    ensure!(size == manifest.bytes && hash.finalize().to_hex().as_str() == manifest.digest, "file checksum mismatch");
    if path.exists() { regular(path, MAX_DATA_BYTES)?; }
    output.as_file().sync_all()?; output.persist(path).map_err(|e| e.error)?; Ok(())
}
pub fn pin(root: &Path, item: &Dataset) -> Result<()> {
    validate(std::slice::from_ref(item))?;
    atomic(&directory(root, "datasets")?.join(format!("{}.json", item.origin)), &serde_json::to_vec(item)?)
}
pub fn prune_chunks(root: &Path) -> Result<()> {
    let mut referenced = BTreeSet::new();
    for entry in std::fs::read_dir(directory(root, "datasets")?)? {
        let entry = entry?; let path = entry.path(); if path.extension().and_then(|s| s.to_str()) != Some("json") { continue; }
        regular(&path, 16 * 1024 * 1024)?;
        let item: Dataset = serde_json::from_slice(&std::fs::read(path)?)?; validate(std::slice::from_ref(&item))?;
        referenced.extend(hashes(&[item]));
    }
    for entry in std::fs::read_dir(directory(root, "blobs")?)? {
        let entry = entry?; let name = entry.file_name().to_string_lossy().to_string();
        if !valid_hash(&name) || referenced.contains(&name) || !entry.file_type()?.is_file() { continue; }
        if entry.metadata()?.modified()?.elapsed().unwrap_or_default().as_secs() > 24 * 3600 { std::fs::remove_file(entry.path())?; }
    }
    Ok(())
}
async fn columns(pool: &SqlitePool, table: &str) -> Result<Vec<String>> {
    ensure!(TABLES.contains(&table), "unregistered table");
    Ok(sqlx::query(&format!("PRAGMA table_info(\"{table}\")")).fetch_all(pool).await?.into_iter().map(|r| r.get("name")).collect())
}
fn native_filter(table: &str) -> &'static str {
    match table {
        "nodes" => "id NOT GLOB 'ax-replica/*'",
        "edges" => "source NOT GLOB 'ax-replica/*' AND target NOT GLOB 'ax-replica/*'",
        "files" | "file_contents" => "path NOT GLOB 'ax-replica/*'",
        "unresolved_refs" => "from_node_id NOT GLOB 'ax-replica/*'",
        "node_communities" => "node_id NOT GLOB 'ax-replica/*'",
        "project_metadata" | "agent_context_cache" => "key NOT GLOB 'ax-replica/*'",
        _ => "id NOT GLOB 'ax-replica/*'",
    }
}
async fn numbers(pool: &SqlitePool) -> Result<(i64,i64,i64,i64,i64)> {
    Ok(sqlx::query_as("SELECT (SELECT COUNT(*) FROM nodes), (SELECT COUNT(*) FROM edges), (SELECT COUNT(*) FROM files), (SELECT COALESCE(SUM(byte_len),0) FROM file_contents), (SELECT COUNT(*) FROM agent_sessions)").fetch_one(pool).await?)
}
pub async fn export(root: &Path, origin: &str, _pool: &SqlitePool) -> Result<Dataset> {
    ensure!(valid_id(origin), "invalid native origin");
    let root_owned = root.to_path_buf(); tokio::task::spawn_blocking(move || prune_chunks(&root_owned)).await??;
    let dir = directory(root, "datasets")?; let temporary = tempfile::tempdir_in(dir)?; let path = temporary.path().join("graph.db");
    let database = crate::Database::open(&path).await?; let pool = database.pool();
    let mut names = BTreeMap::new(); for table in TABLES { names.insert(*table, columns(pool, table).await?); }
    let mut conn = pool.acquire().await?;
    sqlx::query("ATTACH DATABASE ? AS native").bind(root.join(".ax/ax.db").to_string_lossy().as_ref()).execute(&mut *conn).await?;
    let mut tx = conn.begin().await?;
    for table in TABLES {
        let cols = names[table].iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(",");
        sqlx::query(&format!("INSERT OR REPLACE INTO main.\"{table}\" ({cols}) SELECT {cols} FROM native.\"{table}\" WHERE {} ORDER BY rowid", native_filter(table))).execute(&mut *tx).await?;
    }
    sqlx::query("UPDATE schema_versions SET applied_at=0").execute(&mut *tx).await?;
    tx.commit().await?; sqlx::query("DETACH DATABASE native").execute(&mut *conn).await?;
    sqlx::query("VACUUM").execute(&mut *conn).await?;
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&mut *conn).await?; drop(conn);
    let (nodes,edges,files,source_bytes,agent_sessions) = numbers(pool).await?;
    pool.close().await;
    let root_owned = root.to_path_buf(); let origin = origin.to_owned();
    tokio::task::spawn_blocking(move || {
        let graph = file_manifest(&root_owned, &path)?; let mut logs = BTreeMap::new();
        for entry in std::fs::read_dir(root_owned.join(".ax"))? {
            let entry = entry?; let name = entry.file_name().to_string_lossy().to_string();
            if valid_log(&name) && (name == "mcp-verbose.log" || name.len() == 26) && entry.file_type()?.is_file() {
                logs.insert(name, file_manifest(&root_owned, &entry.path())?);
            }
        }
        let item = Dataset { origin, graph, logs, nodes:nodes as u64, edges:edges as u64, files:files as u64, source_bytes:source_bytes as u64, agent_sessions:agent_sessions as u64 };
        pin(&root_owned, &item)?; drop(temporary); Ok(item)
    }).await?
}

struct Prepared { item: Dataset, graph: PathBuf, logs: tempfile::TempDir }
async fn prepare(root: &Path, pool: &SqlitePool, items: &[Dataset]) -> Result<Vec<Prepared>> {
    validate(items)?; let mut expected = BTreeMap::new();
    for table in TABLES { expected.insert(*table, columns(pool, table).await?); }
    let mut result = Vec::new();
    for item in items {
        let dir = directory(root, "datasets")?.join(&item.origin); safe_dir(&dir)?;
        let graph = dir.join(format!("{}.db", item.graph.digest));
        let root_owned = root.to_path_buf(); let file = item.graph.clone(); let dest = graph.clone();
        tokio::task::spawn_blocking(move || assemble(&root_owned, &file, &dest)).await??;
        let options = crate::connect_options(&graph, false).read_only(true);
        let incoming = SqlitePoolOptions::new().max_connections(1).connect_with(options).await?;
        sqlx::query("PRAGMA trusted_schema=OFF").execute(&incoming).await?;
        let integrity: Vec<String> = sqlx::query_scalar("PRAGMA quick_check").fetch_all(&incoming).await?;
        ensure!(integrity == ["ok"], "received graph integrity check failed");
        for table in TABLES {
            let kind: Option<(String,i64)> = sqlx::query_as("SELECT type,rootpage FROM sqlite_master WHERE name=?").bind(table).fetch_optional(&incoming).await?;
            ensure!(kind.is_some_and(|(kind,page)| kind == "table" && page > 0) && columns(&incoming, table).await? == expected[table], "received graph schema mismatch: {table}");
        }
        let (nodes,edges,files,source_bytes,agent_sessions) = numbers(&incoming).await?;
        ensure!(nodes as u64 == item.nodes && edges as u64 == item.edges && files as u64 == item.files && source_bytes as u64 == item.source_bytes && agent_sessions as u64 == item.agent_sessions, "received graph counts mismatch");
        ensure!(sqlx::query("PRAGMA foreign_key_check").fetch_all(&incoming).await?.is_empty(), "received graph has broken references");
        let nested: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE id GLOB 'ax-replica/*'").fetch_one(&incoming).await?;
        ensure!(nested == 0, "received graph contains imported namespaces"); incoming.close().await;
        let log_dir = directory(root, "replicated-logs")?.join(&item.origin); safe_dir(&log_dir)?;
        let logs = tempfile::tempdir_in(&log_dir)?; let root_owned = root.to_path_buf(); let log_map = item.logs.clone(); let stage = logs.path().to_path_buf();
        tokio::task::spawn_blocking(move || -> Result<()> { for (name,file) in log_map { assemble(&root_owned, &file, &stage.join(name))?; } Ok(()) }).await??;
        result.push(Prepared {item:item.clone(),graph,logs});
    }
    Ok(result)
}
pub async fn verify(root: &Path, pool: &SqlitePool, items: &[Dataset]) -> Result<()> { prepare(root, pool, items).await?; Ok(()) }

fn identity(table: &str) -> &'static str { match table { "files"|"file_contents" => "path", "project_metadata"|"agent_context_cache" => "key", "unresolved_refs" => "from_node_id", "edges" => "source", "node_communities" => "node_id", _ => "id" } }
fn mapped(table: &str, column: &str, prefix: &str, community_offset: i64) -> String {
    let name = format!("\"{column}\"");
    if column == "community_id" { return format!("{name}+{community_offset}"); }
    let rewrite = match table {
        "nodes" => matches!(column, "id"|"file_path"), "edges" => matches!(column, "source"|"target"),
        "files"|"file_contents" => column == "path", "unresolved_refs" => matches!(column, "from_node_id"|"file_path"),
        "node_communities" => column == "node_id", "project_metadata"|"agent_context_cache" => column == "key",
        _ => matches!(column, "id"|"session_id"|"turn_id"|"tool_call_id"),
    };
    if rewrite { format!("CASE WHEN {name} IS NULL THEN NULL ELSE '{prefix}'||{name} END") } else { name }
}
pub async fn project(root: &Path, local_node: &str, pool: &SqlitePool, items: &[Dataset]) -> Result<()> {
    validate(items)?; let foreign: Vec<_> = items.iter().filter(|d| d.origin != local_node).cloned().collect();
    let prepared = prepare(root,pool,&foreign).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS ax_replica_datasets (origin TEXT PRIMARY KEY,graph_digest TEXT NOT NULL,manifest_digest TEXT NOT NULL,nodes INTEGER NOT NULL,edges INTEGER NOT NULL,files INTEGER NOT NULL,source_bytes INTEGER NOT NULL,log_files INTEGER NOT NULL,log_bytes INTEGER NOT NULL,agent_sessions INTEGER NOT NULL,updated_at INTEGER NOT NULL)").execute(pool).await?;
    let mut names = BTreeMap::new(); for table in TABLES { names.insert(*table,columns(pool,table).await?); }
    for prepared in prepared {
        let item = &prepared.item; let serialized = serde_json::to_vec(item)?; let digest = blake3::hash(&serialized).to_hex().to_string();
        let old: Option<(String,String)> = sqlx::query_as("SELECT graph_digest,manifest_digest FROM ax_replica_datasets WHERE origin=?").bind(&item.origin).fetch_optional(pool).await?;
        if old.as_ref().is_some_and(|(_,d)| d == &digest) { continue; }
        let prefix = format!("{PREFIX}{}/", item.origin); let mut conn = pool.acquire().await?;
        sqlx::query("ATTACH DATABASE ? AS replicated").bind(prepared.graph.to_string_lossy().as_ref()).execute(&mut *conn).await?;
        let mut tx = conn.begin().await?;
        if old.as_ref().is_none_or(|(graph,_)| graph != &item.graph.digest) {
            for table in ["edges","unresolved_refs","node_communities","nodes","files","file_contents","project_metadata","agent_optimizations","agent_tool_calls","agent_model_calls","agent_turns","agent_sessions","agent_context_cache"] {
                sqlx::query(&format!("DELETE FROM \"{table}\" WHERE \"{}\" GLOB ?",identity(table))).bind(format!("{prefix}*")).execute(&mut *tx).await?;
            }
            let community_offset: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(community_id),0)+1 FROM node_communities").fetch_one(&mut *tx).await?;
            for table in TABLES {
                let cols: Vec<_> = names[table].iter().filter(|c| !(matches!(*table,"edges"|"unresolved_refs") && c.as_str() == "id")).collect();
                let into = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(",");
                let select = cols.iter().map(|c| mapped(table,c,&prefix,community_offset)).collect::<Vec<_>>().join(",");
                sqlx::query(&format!("INSERT INTO main.\"{table}\" ({into}) SELECT {select} FROM replicated.\"{table}\" ORDER BY rowid")).execute(&mut *tx).await?;
            }
            // Candidate IDs are JSON arrays and need the same namespace as resolved edges.
            let rows: Vec<(i64,Option<String>)> = sqlx::query_as("SELECT id,candidates FROM unresolved_refs WHERE from_node_id GLOB ?").bind(format!("{prefix}*")).fetch_all(&mut *tx).await?;
            for (id, candidates) in rows { if let Some(raw) = candidates {
                if let Ok(ids) = serde_json::from_str::<Vec<String>>(&raw) {
                    let ids: Vec<_> = ids.iter().map(|id| format!("{prefix}{id}")).collect();
                    sqlx::query("UPDATE unresolved_refs SET candidates=? WHERE id=?").bind(serde_json::to_string(&ids)?).bind(id).execute(&mut *tx).await?;
                }
            } }
        }
        let root_owned = root.to_path_buf(); let origin = item.origin.clone(); let stage = prepared.logs.path().to_path_buf(); let logs = item.logs.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let dir = directory(&root_owned,"replicated-logs")?.join(origin);
            for name in logs.keys() { let destination = dir.join(name); if destination.exists() { regular(&destination,MAX_DATA_BYTES)?; }
                std::fs::rename(stage.join(name),destination)?;
            } Ok(())
        }).await??;
        sqlx::query("INSERT INTO ax_replica_datasets VALUES (?,?,?,?,?,?,?,?,?,?,strftime('%s','now')) ON CONFLICT(origin) DO UPDATE SET graph_digest=excluded.graph_digest,manifest_digest=excluded.manifest_digest,nodes=excluded.nodes,edges=excluded.edges,files=excluded.files,source_bytes=excluded.source_bytes,log_files=excluded.log_files,log_bytes=excluded.log_bytes,agent_sessions=excluded.agent_sessions,updated_at=excluded.updated_at")
            .bind(&item.origin).bind(&item.graph.digest).bind(&digest).bind(item.nodes as i64).bind(item.edges as i64).bind(item.files as i64).bind(item.source_bytes as i64).bind(item.logs.len() as i64).bind(item.logs.values().map(|f| f.bytes as i64).sum::<i64>()).bind(item.agent_sessions as i64).execute(&mut *tx).await?;
        tx.commit().await?; sqlx::query("DETACH DATABASE replicated").execute(&mut *conn).await?; drop(conn);
        pin(root,item)?;
        // The live projection and pinned chunks retain the data. Old assembled databases are disposable.
        for entry in std::fs::read_dir(prepared.graph.parent().context("missing dataset parent")?)? { let entry=entry?;
            if entry.path()!=prepared.graph && entry.path().extension().and_then(|s|s.to_str())==Some("db") && entry.file_type()?.is_file() {
                // A concurrent reader may keep an old assembled file open on Windows; retry next sync.
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    Ok(())
}
pub async fn counts(pool: &SqlitePool) -> Result<Vec<Counts>> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='ax_replica_datasets')").fetch_one(pool).await?;
    if !exists { return Ok(vec![]); }
    Ok(sqlx::query("SELECT * FROM ax_replica_datasets ORDER BY origin").fetch_all(pool).await?.into_iter().map(|r| Counts {
        origin:r.get("origin"),nodes:r.get("nodes"),edges:r.get("edges"),files:r.get("files"),source_bytes:r.get("source_bytes"),log_files:r.get("log_files"),log_bytes:r.get("log_bytes"),agent_sessions:r.get("agent_sessions"),updated_at:r.get("updated_at"),
    }).collect())
}

/// Preserve received history when a native logger rotates, truncates or removes a file.
pub fn retain_logs(root: &Path, old: &Dataset, new: &mut Dataset) -> Result<()> {
    for (name, file) in &old.logs {
        match new.logs.get(name) {
            None => { new.logs.insert(name.clone(),file.clone()); },
            Some(current) if current == file => {},
            Some(current) => {
                let mut before = Vec::new(); for hash in &file.chunks { before.extend(read_chunk(root,hash)?); }
                let mut now = Vec::new(); for hash in &current.chunks { now.extend(read_chunk(root,hash)?); if now.len() >= before.len() { break; } }
                if !now.starts_with(&before) {
                    let stem = name.strip_suffix(".log").context("invalid log name")?;
                    let segment = format!("{stem}.{}.log",file.digest);
                    ensure!(valid_log(&segment), "invalid retained log segment"); new.logs.insert(segment,file.clone());
                }
            }
        }
    }
    Ok(())
}

use sqlx::Acquire;

#[cfg(test)]
mod tests {
    use super::*;
    async fn db(root: &Path) -> crate::Database { std::fs::create_dir_all(root.join(".ax")).unwrap(); crate::Database::open(&root.join(".ax/ax.db")).await.unwrap() }
    async fn seed(pool: &SqlitePool, source: &str) {
        sqlx::query("INSERT INTO nodes(id,kind,name,qualified_name,file_path,language,start_line,end_line,start_column,end_column,updated_at) VALUES ('same','function','hello','hello','src/main.rs','rust',1,1,0,10,1)").execute(pool).await.unwrap();
        sqlx::query("INSERT INTO files VALUES ('src/main.rs','hash','rust',1,1,1,1,NULL)").execute(pool).await.unwrap();
        sqlx::query("INSERT INTO file_contents VALUES ('src/main.rs','hash',?,?,1)").bind(source).bind(source.len() as i64).execute(pool).await.unwrap();
        sqlx::query("INSERT INTO edges(source,target,kind) VALUES ('same','same','calls')").execute(pool).await.unwrap();
        sqlx::query("INSERT INTO unresolved_refs(from_node_id,reference_name,reference_kind,line,col,candidates,file_path) VALUES ('same','missing','calls',1,0,'[\"same\"]','src/main.rs')").execute(pool).await.unwrap();
        sqlx::query("INSERT INTO node_communities VALUES ('same',1,'Core',1)").execute(pool).await.unwrap();
        sqlx::query("INSERT INTO agent_sessions VALUES ('session','project',1,1,NULL,NULL,'')").execute(pool).await.unwrap();
        sqlx::query("INSERT INTO agent_turns(id,session_id,sequence) VALUES ('turn','session',1)").execute(pool).await.unwrap();
    }
    fn transfer(source: &Path, target: &Path, items: &[Dataset]) { for hash in hashes(items) { store_chunk(target,&hash,&read_chunk(source,&hash).unwrap()).unwrap(); } }
    #[tokio::test]
    async fn complete_graph_is_queryable_collision_safe_and_survives_reindex() {
        let a = tempfile::tempdir().unwrap(); let b = tempfile::tempdir().unwrap(); let da=db(a.path()).await; let db=db(b.path()).await;
        seed(da.pool(),"fn hello() {}\n").await; seed(db.pool(),"local\n").await;
        std::fs::write(a.path().join(".ax/mcp-verbose-2026-10-12.log"),"2026-10-12T00:00:00Z hello\n").unwrap();
        let item=export(a.path(),"a_1",da.pool()).await.unwrap(); let repeated=export(a.path(),"a_1",da.pool()).await.unwrap(); assert_eq!(item,repeated);
        transfer(a.path(),b.path(),std::slice::from_ref(&item)); project(b.path(),"b",db.pool(),std::slice::from_ref(&item)).await.unwrap();
        project(b.path(),"b",db.pool(),std::slice::from_ref(&item)).await.unwrap();
        let text:String=sqlx::query_scalar("SELECT content FROM file_contents WHERE path='ax-replica/a_1/src/main.rs'").fetch_one(db.pool()).await.unwrap(); assert_eq!(text,"fn hello() {}\n");
        let candidates:String=sqlx::query_scalar("SELECT candidates FROM unresolved_refs WHERE from_node_id='ax-replica/a_1/same'").fetch_one(db.pool()).await.unwrap(); assert_eq!(candidates,"[\"ax-replica/a_1/same\"]");
        let query=crate::queries::QueryBuilder::new(db.pool().clone()); query.clear_all().await.unwrap();
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM nodes").fetch_one(db.pool()).await.unwrap(); assert_eq!(count,1);
        query.clear_file("ax-replica/a_1/src/main.rs").await.unwrap();
        let mirror=export(b.path(),"b",db.pool()).await.unwrap(); assert_eq!(mirror.nodes,0); assert_eq!(mirror.agent_sessions,0); assert!(mirror.logs.is_empty());
        assert_eq!(std::fs::read_to_string(b.path().join(".ax/server/replicated-logs/a_1/mcp-verbose-2026-10-12.log")).unwrap(),"2026-10-12T00:00:00Z hello\n");
        sqlx::query("DELETE FROM nodes").execute(da.pool()).await.unwrap();
        let empty=export(a.path(),"a_1",da.pool()).await.unwrap(); transfer(a.path(),b.path(),std::slice::from_ref(&empty)); project(b.path(),"b",db.pool(),&[empty]).await.unwrap();
        assert_eq!(counts(db.pool()).await.unwrap()[0].nodes,0);
    }
    #[tokio::test]
    async fn large_source_uses_bounded_chunks_and_corruption_never_projects() {
        let a=tempfile::tempdir().unwrap(); let b=tempfile::tempdir().unwrap(); let da=db(a.path()).await; let db=db(b.path()).await;
        seed(da.pool(),&"x".repeat(9*1024*1024)).await;
        let item=export(a.path(),"large",da.pool()).await.unwrap(); assert!(item.graph.bytes>8*1024*1024); assert!(item.graph.chunks.len()>16);
        transfer(a.path(),b.path(),std::slice::from_ref(&item));
        let hash=&item.graph.chunks[0]; std::fs::write(directory(b.path(),"blobs").unwrap().join(hash),b"bad").unwrap();
        assert!(project(b.path(),"b",db.pool(),std::slice::from_ref(&item)).await.is_err());
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM nodes").fetch_one(db.pool()).await.unwrap(); assert_eq!(count,0);
        store_chunk(b.path(),hash,&read_chunk(a.path(),hash).unwrap()).unwrap(); project(b.path(),"b",db.pool(),&[item]).await.unwrap();
    }
    #[tokio::test]
    async fn rotated_and_truncated_logs_keep_received_history() {
        let a=tempfile::tempdir().unwrap(); let da=db(a.path()).await;
        let log=a.path().join(".ax/mcp-verbose-2026-10-12.log"); std::fs::write(&log,"first\n").unwrap();
        let old=export(a.path(),"a",da.pool()).await.unwrap(); std::fs::write(&log,"second\n").unwrap();
        let mut new=export(a.path(),"a",da.pool()).await.unwrap(); retain_logs(a.path(),&old,&mut new).unwrap(); assert_eq!(new.logs.len(),2);
        std::fs::remove_file(log).unwrap(); let mut removed=export(a.path(),"a",da.pool()).await.unwrap(); retain_logs(a.path(),&new,&mut removed).unwrap(); assert_eq!(removed.logs.len(),2);
        let mut invalid=removed; invalid.origin="../../escape".into(); assert!(validate(&[invalid]).is_err());
    }
}
