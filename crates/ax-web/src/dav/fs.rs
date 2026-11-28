//! SQLite-backed `DavFileSystem`: pages map to policy/memory rows, everything else to `dav_files`.

use std::collections::BTreeMap;
use std::io::SeekFrom;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bytes::{Buf, Bytes};
use dav_server::davpath::DavPath;
use dav_server::fs::{
    DavDirEntry, DavFile, DavFileSystem, DavMetaData, FsError, FsFuture, FsResult, FsStream,
    OpenOptions, ReadDirMeta,
};
use futures_util::FutureExt;
use sqlx::SqlitePool;

use ax_policy::global_level::Kind;

use super::folders::{self, Found};
use super::pages::{self, Area, Target, DRAFTS_PAGE, FOLDERS_DIR, GLOBAL_DIR};
use super::MAX_PUT_BYTES;
use crate::global_policy::{self, GlobalItem};
use crate::vault_folders::{self, VaultFolder};
use crate::workspace_state::WebHub;

#[derive(Debug, Clone)]
struct Meta {
    len: u64,
    modified: SystemTime,
    dir: bool,
}

impl Meta {
    fn dir(modified_ms: i64) -> Self {
        Self {
            len: 0,
            modified: at_ms(modified_ms),
            dir: true,
        }
    }

    fn file(len: usize, modified_ms: i64) -> Self {
        Self {
            len: len as u64,
            modified: at_ms(modified_ms),
            dir: false,
        }
    }
}

impl DavMetaData for Meta {
    fn len(&self) -> u64 {
        self.len
    }
    fn modified(&self) -> FsResult<SystemTime> {
        Ok(self.modified)
    }
    fn is_dir(&self) -> bool {
        self.dir
    }
    // Vault entries are never symlinks; dav-server 0.12 made this required.
    fn is_symlink(&self) -> bool {
        false
    }
}

fn at_ms(ms: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms.max(0) as u64)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

struct Entry {
    name: String,
    meta: Meta,
}

impl DavDirEntry for Entry {
    fn name(&self) -> Vec<u8> {
        self.name.as_bytes().to_vec()
    }
    fn metadata(&self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let meta = self.meta.clone();
        async move { Ok(Box::new(meta) as Box<dyn DavMetaData>) }.boxed()
    }
}

/// A page's current bytes plus metadata.
struct Content {
    bytes: Vec<u8>,
    modified_ms: i64,
}

/// A file in a vault folder: where it is and its metadata, never its contents.
struct DiskNode {
    path: std::path::PathBuf,
    len: u64,
    modified_ms: i64,
}

enum Node {
    Dir(i64),
    File(Content),
    Disk(DiskNode),
}

impl Node {
    fn meta(&self) -> Meta {
        match self {
            Node::Dir(ms) => Meta::dir(*ms),
            Node::File(c) => Meta::file(c.bytes.len(), c.modified_ms),
            Node::Disk(d) => Meta {
                len: d.len,
                modified: at_ms(d.modified_ms),
                dir: false,
            },
        }
    }
}

fn db_err<E: std::fmt::Display>(e: E) -> FsError {
    tracing::warn!("dav: {e}");
    FsError::GeneralFailure
}

fn rel(path: &DavPath) -> FsResult<String> {
    path.as_rel_ospath()
        .to_str()
        .map(str::to_string)
        .ok_or(FsError::Forbidden)
}

// ── dav_files ──

struct StoredRow {
    path: String,
    is_dir: bool,
    content: Vec<u8>,
    updated_at: i64,
}

type RawRow = (String, i64, Vec<u8>, i64);

impl From<RawRow> for StoredRow {
    fn from((path, is_dir, content, updated_at): RawRow) -> Self {
        StoredRow {
            path,
            is_dir: is_dir != 0,
            content,
            updated_at,
        }
    }
}

async fn stored_get(pool: &SqlitePool, path: &str) -> FsResult<Option<StoredRow>> {
    let row: Option<RawRow> =
        sqlx::query_as("SELECT path, is_dir, content, updated_at FROM dav_files WHERE path = ?")
            .bind(path)
            .fetch_optional(pool)
            .await
            .map_err(db_err)?;
    Ok(row.map(StoredRow::from))
}

async fn stored_put(
    pool: &SqlitePool,
    path: &str,
    is_dir: bool,
    content: &[u8],
    draft_error: Option<&str>,
) -> FsResult<()> {
    sqlx::query(
        "INSERT INTO dav_files (path, is_dir, content, draft_error, updated_at) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(path) DO UPDATE SET is_dir = excluded.is_dir, content = excluded.content,
           draft_error = excluded.draft_error, updated_at = excluded.updated_at",
    )
    .bind(path)
    .bind(is_dir as i64)
    .bind(content)
    .bind(draft_error)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(db_err)?;
    Ok(())
}

/// Skill directories GET can open even when the database row was pruned.
fn disk_skill_names(root: &std::path::Path) -> Vec<String> {
    let mut names = Vec::new();
    for layer in ax_policy::policy_layers(root) {
        let Ok(entries) = std::fs::read_dir(layer.dir.join("skills")) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry.path().join("SKILL.md").is_file() {
                continue;
            }
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else {
                continue;
            };
            if name.is_empty() || names.iter().any(|existing| existing == name) {
                continue;
            }
            names.push(name.to_string());
        }
    }
    names
}

fn like_prefix(dir: &str) -> String {
    let escaped = dir
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("{escaped}/%")
}

/// `path` is `dir` itself or lies below it.
fn is_within(path: &str, dir: &str) -> bool {
    path == dir
        || path
            .strip_prefix(dir)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Delete `path` and, when it is a folder, everything below it.
async fn stored_delete_tree(pool: &SqlitePool, path: &str) -> FsResult<u64> {
    let res = sqlx::query("DELETE FROM dav_files WHERE path = ? OR path LIKE ? ESCAPE '\\'")
        .bind(path)
        .bind(like_prefix(path))
        .execute(pool)
        .await
        .map_err(db_err)?;
    Ok(res.rows_affected())
}

/// The most memories the vault shows as pages (newest first).
const MEMORY_PAGE_LIMIT: i64 = 100_000;

/// `key -> updated_at` from a two-column query. A failed query reads as no
/// times (pages then show time 0), as a single lookup does.
async fn updated_by_key(
    pool: &SqlitePool,
    sql: &'static str,
) -> FsResult<std::collections::HashMap<String, i64>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(sql)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    Ok(rows.into_iter().collect())
}

/// A `dav_files` row without its contents.
struct StoredMeta {
    path: String,
    is_dir: bool,
    len: u64,
    draft_error: Option<String>,
    updated_at: i64,
}

type RawMeta = (String, i64, i64, Option<String>, i64);

impl From<RawMeta> for StoredMeta {
    fn from((path, is_dir, len, draft_error, updated_at): RawMeta) -> Self {
        StoredMeta {
            path,
            is_dir: is_dir != 0,
            len: len.max(0) as u64,
            draft_error,
            updated_at,
        }
    }
}

/// `SELECT <metadata columns> FROM dav_files` followed by `$rest`, as one `&'static str`.
macro_rules! select_meta {
    ($rest:literal) => {
        concat!(
            "SELECT path, is_dir, length(content), draft_error, updated_at FROM dav_files",
            $rest
        )
    };
}

/// One row's metadata, without loading its contents.
async fn stored_meta(pool: &SqlitePool, path: &str) -> FsResult<Option<StoredMeta>> {
    let row: Option<RawMeta> = sqlx::query_as(select_meta!(" WHERE path = ?"))
        .bind(path)
        .fetch_optional(pool)
        .await
        .map_err(db_err)?;
    Ok(row.map(StoredMeta::from))
}

/// Saved rows below `dir` (every saved row for the root), without contents.
async fn stored_below(pool: &SqlitePool, dir: &str) -> FsResult<Vec<StoredMeta>> {
    let rows: Vec<RawMeta> = if dir.is_empty() {
        sqlx::query_as(select_meta!(" WHERE draft_error IS NULL"))
            .fetch_all(pool)
            .await
    } else {
        sqlx::query_as(select_meta!(
            " WHERE draft_error IS NULL AND path LIKE ? ESCAPE '\\'"
        ))
        .bind(like_prefix(dir))
        .fetch_all(pool)
        .await
    }
    .map_err(db_err)?;
    Ok(rows.into_iter().map(StoredMeta::from).collect())
}

/// When some saved row lies below `dir`, the time of one of them: `dir` is then an implied folder.
async fn stored_dir_updated(pool: &SqlitePool, dir: &str) -> FsResult<Option<i64>> {
    sqlx::query_scalar(
        "SELECT updated_at FROM dav_files
         WHERE draft_error IS NULL AND path LIKE ? ESCAPE '\\' LIMIT 1",
    )
    .bind(like_prefix(dir))
    .fetch_optional(pool)
    .await
    .map_err(db_err)
}

async fn drafts(pool: &SqlitePool) -> FsResult<Vec<StoredMeta>> {
    let rows: Vec<RawMeta> =
        sqlx::query_as(select_meta!(" WHERE draft_error IS NOT NULL ORDER BY path"))
            .fetch_all(pool)
            .await
            .map_err(db_err)?;
    Ok(rows
        .into_iter()
        .map(StoredMeta::from)
        .map(|row| StoredMeta {
            is_dir: false,
            ..row
        })
        .collect())
}

/// Direct children of `dir` implied by stored rows (a nested file implies its folders).
fn stored_children(dir: &str, rows: &[StoredMeta]) -> BTreeMap<String, Meta> {
    let prefix = if dir.is_empty() {
        String::new()
    } else {
        format!("{dir}/")
    };
    let mut out = BTreeMap::new();
    for row in rows {
        let Some(rest) = row.path.strip_prefix(&prefix) else {
            continue;
        };
        match rest.split_once('/') {
            Some((name, _)) => {
                out.entry(name.to_string())
                    .or_insert(Meta::dir(row.updated_at));
            }
            None if row.is_dir => {
                out.insert(rest.to_string(), Meta::dir(row.updated_at));
            }
            None => {
                out.insert(
                    rest.to_string(),
                    Meta {
                        len: row.len,
                        modified: at_ms(row.updated_at),
                        dir: false,
                    },
                );
            }
        }
    }
    out
}

// ── The vault ──

#[derive(Clone)]
pub struct DavVault {
    hub: WebHub,
}

impl DavVault {
    pub fn new(hub: WebHub) -> Self {
        Self { hub }
    }

    fn writable(&self) -> FsResult<()> {
        if self.hub.readonly {
            Err(FsError::Forbidden)
        } else {
            Ok(())
        }
    }

    async fn project_root(&self) -> std::path::PathBuf {
        self.hub
            .read()
            .await
            .policy
            .store
            .project_root()
            .to_path_buf()
    }

    async fn folder(&self, name: &str) -> FsResult<VaultFolder> {
        vault_folders::find(&self.project_root().await, name).ok_or(FsError::NotFound)
    }

    async fn folder_changed(&self, folder: &VaultFolder) {
        if folder.index {
            vault_folders::schedule_resync(
                self.hub.clone(),
                self.project_root().await,
                folder.name.clone(),
            );
        }
    }

    async fn folder_lookup(&self, name: &str, rest: &str) -> FsResult<Node> {
        let folder = self.folder(name).await?;
        let rest = rest.to_string();
        Ok(
            match folders::blocking(move || folders::lookup(&folder.path, &rest)).await? {
                Found::Dir(ms) => Node::Dir(ms),
                Found::File(path, len, modified_ms) => Node::Disk(DiskNode {
                    path,
                    len,
                    modified_ms,
                }),
            },
        )
    }

    async fn pool(&self) -> SqlitePool {
        self.hub.read().await.policy.store.pool().clone()
    }

    async fn memory_pool(&self) -> SqlitePool {
        self.hub.read().await.graph_pool.clone()
    }

    async fn rule_updated(&self, id: &str) -> i64 {
        let pool = self.pool().await;
        sqlx::query_scalar("SELECT updated_at FROM policy_rules WHERE id = ?")
            .bind(id)
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten()
            .unwrap_or(0)
    }

    async fn skill_updated(&self, name: &str) -> i64 {
        let pool = self.pool().await;
        sqlx::query_scalar("SELECT updated_at FROM policy_skills WHERE name = ?")
            .bind(name)
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten()
            .unwrap_or(0)
    }

    async fn policy_pool_and_root(&self) -> (SqlitePool, std::path::PathBuf) {
        let ws = self.hub.read().await;
        (
            ws.policy.store.pool().clone(),
            ws.policy.store.project_root().to_path_buf(),
        )
    }

    async fn memories(&self) -> FsResult<Vec<ax_memory::MemoryRow>> {
        let pool = self.memory_pool().await;
        let (rows, _) = ax_memory::list(&pool, MEMORY_PAGE_LIMIT as usize, 0)
            .await
            .map_err(db_err)?;
        Ok(rows)
    }

    /// The memory a page stem names. A stem depends on every title, so this
    /// reads the titles of the rows [`Self::memories`] lists but loads only
    /// the matching memory in full.
    async fn memory_for_stem(&self, stem: &str) -> FsResult<Option<ax_memory::MemoryRow>> {
        let pool = self.memory_pool().await;
        let keys: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT id, title, created_at FROM memories ORDER BY updated_at DESC LIMIT ?",
        )
        .bind(MEMORY_PAGE_LIMIT)
        .fetch_all(&pool)
        .await
        .map_err(db_err)?;
        let entries: Vec<(&str, &str, i64)> = keys
            .iter()
            .map(|(id, title, created)| (id.as_str(), title.as_str(), *created))
            .collect();
        let Some(id) = ax_policy::links::unique_stems(&entries)
            .into_iter()
            .find(|(_, s)| s == stem)
            .map(|(id, _)| id)
        else {
            return Ok(None);
        };
        ax_memory::get(&pool, &id).await.map_err(db_err)
    }

    async fn global_item(&self, area: Area, stem: &str) -> FsResult<Option<GlobalItem>> {
        global_policy::leader(global_kind(area), stem)
            .await
            .map_err(db_err)
    }

    /// `app.json` as served: the stored file with defaults merged in (see [`pages::app_json`]).
    async fn app_json(&self) -> FsResult<Node> {
        Ok(
            match stored_get(&self.pool().await, pages::APP_JSON).await? {
                Some(row) if row.is_dir => Node::Dir(row.updated_at),
                Some(row) => Node::File(Content {
                    bytes: pages::app_json(Some(&row.content)),
                    modified_ms: row.updated_at,
                }),
                None => Node::File(Content {
                    bytes: pages::app_json(None),
                    modified_ms: 0,
                }),
            },
        )
    }

    /// The saved page (ignoring drafts).
    async fn saved_page(&self, area: Area, stem: &str) -> FsResult<Option<Content>> {
        if area.is_global() {
            return Ok(self
                .global_item(area, stem)
                .await?
                .and_then(|item| global_page(area, &item)));
        }
        let ws = self.hub.read().await;
        let store = &ws.policy.store;
        Ok(match area {
            Area::Rules => match store.get_rule_doc(stem).await.map_err(db_err)? {
                Some(doc) => {
                    drop(ws);
                    Some(Content {
                        bytes: pages::rule_page(&doc).into_bytes(),
                        modified_ms: self.rule_updated(stem).await,
                    })
                }
                None => None,
            },
            Area::Skills => match store.get_skill_doc(stem).await.map_err(db_err)? {
                Some(doc) => {
                    drop(ws);
                    Some(Content {
                        bytes: pages::skill_page(&doc).into_bytes(),
                        modified_ms: self.skill_updated(stem).await,
                    })
                }
                None => None,
            },
            Area::Memories => {
                drop(ws);
                self.memory_for_stem(stem).await?.map(|m| Content {
                    bytes: pages::memory_page(&m).into_bytes(),
                    modified_ms: m.updated_at,
                })
            }
            Area::GlobalRules | Area::GlobalSkills => None,
        })
    }

    async fn drafts_page(&self) -> FsResult<Content> {
        let rows = drafts(&self.pool().await).await?;
        let modified_ms = rows.iter().map(|r| r.updated_at).max().unwrap_or(0);
        let list: Vec<(String, String)> = rows
            .into_iter()
            .map(|r| (r.path, r.draft_error.unwrap_or_default()))
            .collect();
        Ok(Content {
            bytes: pages::drafts_page(&list).into_bytes(),
            modified_ms,
        })
    }

    async fn lookup(&self, target: &Target) -> FsResult<Node> {
        match target {
            Target::Root | Target::GlobalDir | Target::AreaDir(_) | Target::FoldersDir => {
                Ok(Node::Dir(0))
            }
            Target::Folder(name, rest) => self.folder_lookup(name, rest).await,
            Target::Junk => Err(FsError::NotFound),
            Target::Stored(path) if path == pages::APP_JSON => self.app_json().await,
            Target::Drafts => Ok(Node::File(self.drafts_page().await?)),
            Target::Page(area, stem) => {
                let path = pages::page_path(*area, stem);
                if let Some(row) = stored_get(&self.pool().await, &path).await? {
                    return Ok(Node::File(Content {
                        bytes: row.content,
                        modified_ms: row.updated_at,
                    }));
                }
                self.saved_page(*area, stem)
                    .await?
                    .map(Node::File)
                    .ok_or(FsError::NotFound)
            }
            Target::Stored(path) => {
                let pool = self.pool().await;
                if let Some(row) = stored_get(&pool, path).await? {
                    return Ok(if row.is_dir {
                        Node::Dir(row.updated_at)
                    } else {
                        Node::File(Content {
                            bytes: row.content,
                            modified_ms: row.updated_at,
                        })
                    });
                }
                stored_dir_updated(&pool, path)
                    .await?
                    .map(Node::Dir)
                    .ok_or(FsError::NotFound)
            }
        }
    }

    async fn area_pages(&self, area: Area) -> FsResult<BTreeMap<String, Meta>> {
        let mut out = BTreeMap::new();
        let stems: Vec<String> = match area {
            Area::Rules => {
                // One list query and one timestamp query build every page,
                // the same pages `saved_page` serves for an indexed rule.
                let (pool, root) = self.policy_pool_and_root().await;
                let rows = ax_policy::index::list_rules(&pool).await.map_err(db_err)?;
                let updated =
                    updated_by_key(&pool, "SELECT id, updated_at FROM policy_rules").await?;
                for row in &rows {
                    let len = pages::rule_page(&ax_policy::rule_row_to_doc(row, &root)).len();
                    let ms = updated.get(&row.id).copied().unwrap_or(0);
                    out.insert(format!("{}.md", row.id), Meta::file(len, ms));
                }
                Vec::new()
            }
            Area::Skills => {
                let (pool, root) = self.policy_pool_and_root().await;
                let rows = ax_policy::index::list_skills(&pool).await.map_err(db_err)?;
                let updated =
                    updated_by_key(&pool, "SELECT name, updated_at FROM policy_skills").await?;
                for row in &rows {
                    let len = pages::skill_page(&ax_policy::skill_row_to_doc(row, &root)).len();
                    let ms = updated.get(&row.name).copied().unwrap_or(0);
                    out.insert(format!("{}.md", row.name), Meta::file(len, ms));
                }
                // Skill folders on disk without a database row are read one by one.
                disk_skill_names(&root)
                    .into_iter()
                    .filter(|name| !rows.iter().any(|r| &r.name == name))
                    .collect()
            }
            Area::Memories => {
                let rows = self.memories().await?;
                let by_id: std::collections::HashMap<&str, &ax_memory::MemoryRow> =
                    rows.iter().map(|m| (m.id.as_str(), m)).collect();
                for (id, stem) in pages::memory_stems(&rows) {
                    if let Some(m) = by_id.get(id.as_str()) {
                        let len = pages::memory_page(m).len();
                        out.insert(format!("{stem}.md"), Meta::file(len, m.updated_at));
                    }
                }
                Vec::new()
            }
            Area::GlobalRules | Area::GlobalSkills => {
                let items = global_policy::leaders(global_kind(area))
                    .await
                    .map_err(db_err)?;
                for item in items.iter().filter(|i| !i.name.contains('/')) {
                    if let Some(c) = global_page(area, item) {
                        out.insert(
                            format!("{}.md", item.name),
                            Meta::file(c.bytes.len(), c.modified_ms),
                        );
                    }
                }
                Vec::new()
            }
        };
        for stem in stems {
            if let Some(c) = self.saved_page(area, &stem).await? {
                out.insert(
                    format!("{stem}.md"),
                    Meta::file(c.bytes.len(), c.modified_ms),
                );
            }
        }
        let prefix = format!("{}/", area.dir());
        for row in drafts(&self.pool().await).await? {
            if let Some(name) = row.path.strip_prefix(&prefix) {
                out.insert(
                    name.to_string(),
                    Meta {
                        len: row.len,
                        modified: at_ms(row.updated_at),
                        dir: false,
                    },
                );
            }
        }
        Ok(out)
    }

    async fn children(&self, target: &Target) -> FsResult<BTreeMap<String, Meta>> {
        let pool = self.pool().await;
        match target {
            Target::Root => {
                let mut out = stored_children("", &stored_below(&pool, "").await?);
                for area in Area::TOP {
                    out.insert(area.dir().to_string(), Meta::dir(0));
                }
                out.insert(GLOBAL_DIR.to_string(), Meta::dir(0));
                out.insert(FOLDERS_DIR.to_string(), Meta::dir(0));
                let drafts = self.drafts_page().await?;
                out.insert(
                    DRAFTS_PAGE.into(),
                    Meta::file(drafts.bytes.len(), drafts.modified_ms),
                );
                Ok(out)
            }
            Target::GlobalDir => {
                let mut out = stored_children(GLOBAL_DIR, &stored_below(&pool, GLOBAL_DIR).await?);
                for area in Area::GLOBAL {
                    let name = area.dir().rsplit('/').next().unwrap_or_default();
                    out.insert(name.to_string(), Meta::dir(0));
                }
                Ok(out)
            }
            Target::AreaDir(area) => {
                let mut out = stored_children(area.dir(), &stored_below(&pool, area.dir()).await?);
                out.extend(self.area_pages(*area).await?);
                Ok(out)
            }
            Target::Stored(path) => match self.lookup(target).await? {
                Node::Dir(_) => {
                    let mut out = stored_children(path, &stored_below(&pool, path).await?);
                    if let Some((dir, name)) = pages::APP_JSON.rsplit_once('/') {
                        if path == dir {
                            if let Node::File(c) = self.app_json().await? {
                                out.insert(
                                    name.to_string(),
                                    Meta::file(c.bytes.len(), c.modified_ms),
                                );
                            }
                        }
                    }
                    Ok(out)
                }
                Node::File(_) | Node::Disk(_) => Err(FsError::Forbidden),
            },
            Target::FoldersDir => Ok(vault_folders::load(&self.project_root().await)
                .into_iter()
                .map(|f| (f.name, Meta::dir(0)))
                .collect()),
            Target::Folder(name, rest) => {
                let folder = self.folder(name).await?;
                let rest = rest.to_string();
                Ok(
                    folders::blocking(move || folders::children(&folder.path, &rest))
                        .await?
                        .into_iter()
                        .map(|(n, (dir, len, ms))| {
                            (
                                n,
                                if dir {
                                    Meta::dir(ms)
                                } else {
                                    Meta::file(len as usize, ms)
                                },
                            )
                        })
                        .collect(),
                )
            }
            _ => Err(FsError::Forbidden),
        }
    }

    /// Save a page to its row. `Err` is the reason it stays a draft.
    async fn save_page(&self, area: Area, stem: &str, text: &str) -> Result<(), String> {
        let ws = self.hub.read().await;
        let store = &ws.policy.store;
        match area {
            Area::Rules => {
                let existing = store.get_rule_doc(stem).await.ok().flatten();
                let (fm, body) = pages::rule_from_page(stem, text, existing.as_ref())?;
                store
                    .save_rule(fm, body)
                    .await
                    .map(|_| ())
                    .map_err(|v| v.error)
            }
            Area::Skills => {
                let existing = store.get_skill_doc(stem).await.ok().flatten();
                let (fm, body) = pages::skill_from_page(stem, text, existing.as_ref())?;
                store
                    .save_skill(fm, body)
                    .await
                    .map(|_| ())
                    .map_err(|v| v.error)
            }
            Area::Memories => {
                drop(ws);
                self.save_memory(stem, text).await
            }
            Area::GlobalRules | Area::GlobalSkills => {
                let root = store.project_root().to_path_buf();
                drop(ws);
                self.save_global(&root, area, stem, text).await
            }
        }
    }

    /// Updates the leading row of an existing item; a new item is filed under this project.
    async fn save_global(
        &self,
        root: &std::path::Path,
        area: Area,
        stem: &str,
        text: &str,
    ) -> Result<(), String> {
        let existing = global_policy::leader(global_kind(area), stem).await?;
        let project_id = existing.as_ref().map(|i| i.project_id);
        if area == Area::GlobalRules {
            let doc = existing.as_ref().and_then(GlobalItem::rule_doc);
            let (fm, body) = pages::rule_from_page(stem, text, doc.as_ref())?;
            global_policy::save_rule(root, project_id, fm, body).await
        } else {
            let doc = existing.as_ref().and_then(GlobalItem::skill_doc);
            let (fm, body) = pages::skill_from_page(stem, text, doc.as_ref())?;
            global_policy::save_skill(root, project_id, fm, body).await
        }
    }

    async fn save_memory(&self, stem: &str, text: &str) -> Result<(), String> {
        let draft = pages::memory_from_page(stem, text)?;
        let pool = self.memory_pool().await;
        let existing = match &draft.id {
            Some(id) => Some(
                ax_memory::get(&pool, id)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("unknown memory id {id}"))?,
            ),
            None => self
                .memory_for_stem(stem)
                .await
                .map_err(|e| e.to_string())?,
        };
        let input = draft.into_input(existing.as_ref());
        match existing {
            Some(m) => ax_memory::update(&pool, &m.id, input).await.map(|_| ()),
            None => ax_memory::remember(&pool, input).await.map(|_| ()),
        }
        .map_err(|e| e.to_string())
    }

    async fn commit(&self, path: &str, bytes: Vec<u8>) -> FsResult<()> {
        self.writable()?;
        let pool = self.pool().await;
        match pages::classify(path) {
            Target::Junk => Ok(()),
            Target::Root
            | Target::GlobalDir
            | Target::AreaDir(_)
            | Target::Drafts
            | Target::FoldersDir => Err(FsError::Forbidden),
            Target::Folder(name, rest) => {
                let folder = self.folder(&name).await?;
                if rest.is_empty() {
                    return Err(FsError::Forbidden);
                }
                let (root, rest) = (folder.path.clone(), rest.clone());
                folders::blocking(move || folders::write(&root, &rest, &bytes)).await?;
                self.folder_changed(&folder).await;
                Ok(())
            }
            Target::Stored(p) => stored_put(&pool, &p, false, &bytes, None).await,
            Target::Page(area, stem) => {
                let page = pages::page_path(area, &stem);
                let outcome = match std::str::from_utf8(&bytes) {
                    Ok(text) => self.save_page(area, &stem, text).await,
                    Err(_) => Err("not UTF-8 text".into()),
                };
                match outcome {
                    Ok(()) => stored_delete_tree(&pool, &page).await.map(|_| ()),
                    Err(reason) => stored_put(&pool, &page, false, &bytes, Some(&reason)).await,
                }
            }
        }
    }

    /// Every project uses a global item, so the vault never deletes or renames one.
    async fn refuse_saved_global(&self, area: Area, stem: &str) -> FsResult<()> {
        if area.is_global() && self.global_item(area, stem).await?.is_some() {
            return Err(FsError::Forbidden);
        }
        Ok(())
    }

    async fn delete_saved(&self, area: Area, stem: &str) -> FsResult<bool> {
        self.refuse_saved_global(area, stem).await?;
        let ws = self.hub.read().await;
        match area {
            Area::Rules => ws.policy.store.delete_rule(stem).await.map_err(db_err),
            Area::Skills => ws.policy.store.delete_skill(stem).await.map_err(db_err),
            Area::Memories => {
                drop(ws);
                match self.memory_for_stem(stem).await? {
                    Some(m) => ax_memory::delete(&self.memory_pool().await, &m.id)
                        .await
                        .map_err(db_err),
                    None => Ok(false),
                }
            }
            Area::GlobalRules | Area::GlobalSkills => Ok(false),
        }
    }

    async fn remove(&self, path: &str) -> FsResult<()> {
        self.writable()?;
        let pool = self.pool().await;
        match pages::classify(path) {
            Target::Junk => Ok(()),
            Target::Root
            | Target::GlobalDir
            | Target::AreaDir(_)
            | Target::Drafts
            | Target::FoldersDir => Err(FsError::Forbidden),
            Target::Folder(name, rest) => {
                let folder = self.folder(&name).await?;
                if rest.is_empty() {
                    return Err(FsError::Forbidden);
                }
                let (root, rest) = (folder.path.clone(), rest.clone());
                folders::blocking(move || folders::remove(&root, &rest)).await?;
                self.folder_changed(&folder).await;
                Ok(())
            }
            Target::Stored(p) => match stored_delete_tree(&pool, &p).await? {
                0 => Err(FsError::NotFound),
                _ => Ok(()),
            },
            Target::Page(area, stem) => {
                let saved = self.delete_saved(area, &stem).await?;
                let drafted = stored_delete_tree(&pool, &pages::page_path(area, &stem)).await? > 0;
                if drafted || saved {
                    Ok(())
                } else {
                    Err(FsError::NotFound)
                }
            }
        }
    }

    async fn mkdir(&self, path: &str) -> FsResult<()> {
        self.writable()?;
        match pages::classify(path) {
            Target::Root | Target::GlobalDir | Target::AreaDir(_) | Target::FoldersDir => {
                Err(FsError::Exists)
            }
            Target::Folder(name, rest) => {
                let folder = self.folder(&name).await?;
                if rest.is_empty() {
                    return Err(FsError::Exists);
                }
                folders::blocking(move || folders::mkdir(&folder.path, &rest)).await
            }
            Target::Stored(p) => {
                let pool = self.pool().await;
                if stored_get(&pool, &p).await?.is_some() {
                    return Err(FsError::Exists);
                }
                stored_put(&pool, &p, true, &[], None).await
            }
            _ => Err(FsError::Forbidden),
        }
    }

    async fn rename_saved(&self, area: Area, from: &str, to: &str) -> FsResult<bool> {
        self.refuse_saved_global(area, from).await?;
        let ws = self.hub.read().await;
        let store = &ws.policy.store;
        match area {
            Area::Rules => {
                let Some(doc) = store.get_rule_doc(from).await.map_err(db_err)? else {
                    return Ok(false);
                };
                let mut fm = doc.frontmatter;
                fm.id = to.to_string();
                store
                    .rename_rule(from, fm, doc.body)
                    .await
                    .map_err(|_| FsError::Forbidden)?;
                Ok(true)
            }
            Area::Skills => {
                let Some(doc) = store.get_skill_doc(from).await.map_err(db_err)? else {
                    return Ok(false);
                };
                if store.get_skill_doc(to).await.map_err(db_err)?.is_some() {
                    return Err(FsError::Exists);
                }
                let mut fm = doc.frontmatter;
                fm.name = to.to_string();
                store
                    .save_skill(fm, doc.body)
                    .await
                    .map_err(|_| FsError::Forbidden)?;
                store.delete_skill(from).await.map_err(db_err)?;
                Ok(true)
            }
            Area::Memories => {
                drop(ws);
                let Some(m) = self.memory_for_stem(from).await? else {
                    return Ok(false);
                };
                let input = ax_memory::RememberInput {
                    title: to.to_string(),
                    body: m.body.clone(),
                    kind: Some(m.kind.clone()),
                    tags: m.tags.clone(),
                    files: m.files.clone(),
                    source: None,
                };
                ax_memory::update(&self.memory_pool().await, &m.id, input)
                    .await
                    .map_err(db_err)?;
                Ok(true)
            }
            Area::GlobalRules | Area::GlobalSkills => Ok(false),
        }
    }

    async fn rename_page(&self, area: Area, from: &str, to: &str) -> FsResult<()> {
        let pool = self.pool().await;
        let draft = stored_get(&pool, &pages::page_path(area, from)).await?;
        let renamed = self.rename_saved(area, from, to).await?;
        let Some(draft) = draft else {
            return if renamed {
                Ok(())
            } else {
                Err(FsError::NotFound)
            };
        };
        stored_delete_tree(&pool, &draft.path).await?;
        self.commit(&pages::page_path(area, to), draft.content)
            .await
    }

    async fn move_path(&self, from: &str, to: &str) -> FsResult<()> {
        self.writable()?;
        match (pages::classify(from), pages::classify(to)) {
            (Target::Page(a, f), Target::Page(b, t)) if a == b => self.rename_page(a, &f, &t).await,
            (Target::Stored(f), Target::Stored(t)) => self.rename_stored(&f, &t).await,
            (Target::Folder(a, f), Target::Folder(b, t))
                if a == b && !f.is_empty() && !t.is_empty() =>
            {
                let folder = self.folder(&a).await?;
                let root = folder.path.clone();
                folders::blocking(move || folders::rename(&root, &f, &t)).await?;
                self.folder_changed(&folder).await;
                Ok(())
            }
            (Target::Stored(f), Target::Page(..)) => {
                let pool = self.pool().await;
                let row = stored_get(&pool, &f).await?.ok_or(FsError::NotFound)?;
                if row.is_dir {
                    return Err(FsError::Forbidden);
                }
                self.commit(to, row.content).await?;
                stored_delete_tree(&pool, &f).await.map(|_| ())
            }
            _ => Err(FsError::Forbidden),
        }
    }

    /// Move `from` and everything below it to `to` in one transaction, replacing
    /// whatever was at `to`. A destination inside the source is refused.
    async fn rename_stored(&self, from: &str, to: &str) -> FsResult<()> {
        if is_within(to, from) {
            return Err(FsError::Forbidden);
        }
        let pool = self.pool().await;
        let mut tx = pool.begin().await.map_err(db_err)?;
        sqlx::query("DELETE FROM dav_files WHERE path = ? OR path LIKE ? ESCAPE '\\'")
            .bind(to)
            .bind(like_prefix(to))
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        let moved = sqlx::query(
            "UPDATE dav_files SET path = ? || substr(path, ?), updated_at = ?
             WHERE path = ? OR path LIKE ? ESCAPE '\\'",
        )
        .bind(to)
        .bind(from.chars().count() as i64 + 1)
        .bind(now_ms())
        .bind(from)
        .bind(like_prefix(from))
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
        if moved.rows_affected() == 0 {
            return Err(FsError::NotFound);
        }
        tx.commit().await.map_err(db_err)
    }
}

fn global_kind(area: Area) -> Kind {
    if area == Area::GlobalRules {
        Kind::Rule
    } else {
        Kind::Skill
    }
}

fn global_page(area: Area, item: &GlobalItem) -> Option<Content> {
    let text = if area == Area::GlobalRules {
        pages::rule_page(&item.rule_doc()?)
    } else {
        pages::skill_page(&item.skill_doc()?)
    };
    Some(Content {
        bytes: text.into_bytes(),
        modified_ms: item.changed_ms,
    })
}

// ── File handle ──

#[derive(Debug)]
struct VaultFile {
    #[allow(dead_code)]
    path: String,
    data: Vec<u8>,
    pos: usize,
    modified_ms: i64,
    write: Option<WriteTarget>,
    /// Bytes not yet committed. A file opened for writing starts dirty, so
    /// creating an empty file commits it; a later flush with no new writes does not.
    dirty: bool,
}

struct WriteTarget {
    vault: DavVault,
    path: String,
}

impl std::fmt::Debug for WriteTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WriteTarget")
            .field("path", &self.path)
            .finish()
    }
}

impl VaultFile {
    fn write_slice(&mut self, buf: &[u8]) -> FsResult<()> {
        let end = self.pos + buf.len();
        if end > MAX_PUT_BYTES {
            return Err(FsError::TooLarge);
        }
        if end > self.data.len() {
            self.data.resize(end, 0);
        }
        self.data[self.pos..end].copy_from_slice(buf);
        self.pos = end;
        self.dirty = true;
        Ok(())
    }
}

impl DavFile for VaultFile {
    fn metadata(&mut self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let meta = Meta::file(self.data.len(), self.modified_ms);
        async move { Ok(Box::new(meta) as Box<dyn DavMetaData>) }.boxed()
    }

    fn write_buf(&mut self, mut buf: Box<dyn Buf + Send>) -> FsFuture<'_, ()> {
        let chunk = buf.copy_to_bytes(buf.remaining());
        self.write_bytes(chunk)
    }

    fn write_bytes(&mut self, buf: Bytes) -> FsFuture<'_, ()> {
        let result = if self.write.is_none() {
            Err(FsError::Forbidden)
        } else {
            self.write_slice(&buf)
        };
        async move { result }.boxed()
    }

    fn read_bytes(&mut self, count: usize) -> FsFuture<'_, Bytes> {
        let start = self.pos.min(self.data.len());
        let end = (start + count).min(self.data.len());
        self.pos = end;
        let out = Bytes::copy_from_slice(&self.data[start..end]);
        async move { Ok(out) }.boxed()
    }

    fn seek(&mut self, pos: SeekFrom) -> FsFuture<'_, u64> {
        let next = match pos {
            SeekFrom::Start(n) => n as i64,
            SeekFrom::End(n) => self.data.len() as i64 + n,
            SeekFrom::Current(n) => self.pos as i64 + n,
        };
        let result = if next < 0 {
            Err(FsError::GeneralFailure)
        } else {
            self.pos = next as usize;
            Ok(next as u64)
        };
        async move { result }.boxed()
    }

    fn flush(&mut self) -> FsFuture<'_, ()> {
        async move {
            match &self.write {
                Some(target) if self.dirty => {
                    target.vault.commit(&target.path, self.data.clone()).await?;
                    self.dirty = false;
                    Ok(())
                }
                _ => Ok(()),
            }
        }
        .boxed()
    }
}

/// A read-only handle on a vault folder file that reads from disk as the
/// client asks, so serving a large file never holds all of it in memory.
#[derive(Debug)]
struct DiskFile {
    file: tokio::fs::File,
    len: u64,
    modified_ms: i64,
}

impl DiskFile {
    async fn open(disk: DiskNode) -> FsResult<Box<dyn DavFile>> {
        let file = tokio::fs::File::open(&disk.path)
            .await
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => FsError::NotFound,
                std::io::ErrorKind::PermissionDenied => FsError::Forbidden,
                _ => FsError::GeneralFailure,
            })?;
        Ok(Box::new(DiskFile {
            file,
            len: disk.len,
            modified_ms: disk.modified_ms,
        }) as Box<dyn DavFile>)
    }
}

impl DavFile for DiskFile {
    fn metadata(&mut self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let meta = Meta {
            len: self.len,
            modified: at_ms(self.modified_ms),
            dir: false,
        };
        async move { Ok(Box::new(meta) as Box<dyn DavMetaData>) }.boxed()
    }

    fn write_buf(&mut self, _buf: Box<dyn Buf + Send>) -> FsFuture<'_, ()> {
        async { Err(FsError::Forbidden) }.boxed()
    }

    fn write_bytes(&mut self, _buf: Bytes) -> FsFuture<'_, ()> {
        async { Err(FsError::Forbidden) }.boxed()
    }

    fn read_bytes(&mut self, count: usize) -> FsFuture<'_, Bytes> {
        async move {
            use tokio::io::AsyncReadExt;
            let mut buf = vec![0u8; count];
            let n = self
                .file
                .read(&mut buf)
                .await
                .map_err(|_| FsError::GeneralFailure)?;
            buf.truncate(n);
            Ok(Bytes::from(buf))
        }
        .boxed()
    }

    fn seek(&mut self, pos: SeekFrom) -> FsFuture<'_, u64> {
        async move {
            use tokio::io::AsyncSeekExt;
            self.file
                .seek(pos)
                .await
                .map_err(|_| FsError::GeneralFailure)
        }
        .boxed()
    }

    fn flush(&mut self) -> FsFuture<'_, ()> {
        async { Ok(()) }.boxed()
    }
}

impl DavFileSystem for DavVault {
    fn open<'a>(
        &'a self,
        path: &'a DavPath,
        options: OpenOptions,
    ) -> FsFuture<'a, Box<dyn DavFile>> {
        async move {
            let path = rel(path)?;
            let target = pages::classify(&path);
            let writing = options.write || options.append || options.create || options.create_new;
            let current = match self.lookup(&target).await {
                Ok(Node::Dir(_)) => return Err(FsError::Forbidden),
                Ok(Node::File(c)) => Some(c),
                Ok(Node::Disk(disk)) if !writing => return DiskFile::open(disk).await,
                Ok(Node::Disk(disk)) => Some(Content {
                    bytes: if options.truncate {
                        Vec::new()
                    } else {
                        let path = disk.path;
                        folders::blocking(move || folders::read_limited(&path, MAX_PUT_BYTES))
                            .await?
                    },
                    modified_ms: disk.modified_ms,
                }),
                Err(FsError::NotFound) => None,
                Err(e) => return Err(e),
            };
            if writing {
                self.writable()?;
                if current.is_some() && options.create_new {
                    return Err(FsError::Exists);
                }
                if current.is_none() && !options.create && !options.create_new {
                    return Err(FsError::NotFound);
                }
                if options.size.is_some_and(|n| n as usize > MAX_PUT_BYTES) {
                    return Err(FsError::TooLarge);
                }
            }
            let Some(current) = current.or_else(|| {
                writing.then(|| Content {
                    bytes: Vec::new(),
                    modified_ms: now_ms(),
                })
            }) else {
                return Err(FsError::NotFound);
            };
            let data = if writing && options.truncate {
                Vec::new()
            } else {
                current.bytes
            };
            let pos = if options.append { data.len() } else { 0 };
            let write = writing.then(|| WriteTarget {
                vault: self.clone(),
                path: path.clone(),
            });
            Ok(Box::new(VaultFile {
                path,
                data,
                pos,
                modified_ms: current.modified_ms,
                dirty: write.is_some(),
                write,
            }) as Box<dyn DavFile>)
        }
        .boxed()
    }

    fn read_dir<'a>(
        &'a self,
        path: &'a DavPath,
        _meta: ReadDirMeta,
    ) -> FsFuture<'a, FsStream<Box<dyn DavDirEntry>>> {
        async move {
            let children = self.children(&pages::classify(&rel(path)?)).await?;
            let entries: Vec<FsResult<Box<dyn DavDirEntry>>> = children
                .into_iter()
                .map(|(name, meta)| Ok(Box::new(Entry { name, meta }) as Box<dyn DavDirEntry>))
                .collect();
            Ok(Box::pin(futures_util::stream::iter(entries)) as FsStream<Box<dyn DavDirEntry>>)
        }
        .boxed()
    }

    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        async move {
            let target = pages::classify(&rel(path)?);
            if let Target::Stored(p) = &target {
                if p != pages::APP_JSON {
                    if let Some(row) = stored_meta(&self.pool().await, p).await? {
                        let meta = if row.is_dir {
                            Meta::dir(row.updated_at)
                        } else {
                            Meta {
                                len: row.len,
                                modified: at_ms(row.updated_at),
                                dir: false,
                            }
                        };
                        return Ok(Box::new(meta) as Box<dyn DavMetaData>);
                    }
                }
            }
            let node = self.lookup(&target).await?;
            Ok(Box::new(node.meta()) as Box<dyn DavMetaData>)
        }
        .boxed()
    }

    // No symlinks in the vault, so link metadata is the entry's own metadata
    // (the dav-server 0.11 default, required since 0.12).
    fn symlink_metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        self.metadata(path)
    }

    fn create_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        async move { self.mkdir(&rel(path)?).await }.boxed()
    }

    fn remove_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        async move { self.remove(&rel(path)?).await }.boxed()
    }

    fn remove_file<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        async move { self.remove(&rel(path)?).await }.boxed()
    }

    fn rename<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        async move { self.move_path(&rel(from)?, &rel(to)?).await }.boxed()
    }
}
