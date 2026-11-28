//! Extra directories shown in the vault drive under `folders/`, optionally imported into memory.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use ax_memory::FolderSyncReport;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::workspace_state::WebHub;

pub const FOLDERS_DIR: &str = "folders";
const CONFIG_FILE: &str = "vault-folders.json";
const MAX_FOLDERS: usize = 64;
const WRITE_RESYNC_DELAY: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultFolder {
    pub name: String,
    pub path: PathBuf,
    #[serde(default)]
    pub index: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct LastSync {
    at_ms: i64,
    #[serde(flatten)]
    report: FolderSyncReport,
}

type FolderKey = (PathBuf, String);

static LAST_SYNC: Mutex<BTreeMap<FolderKey, LastSync>> = Mutex::new(BTreeMap::new());
static PENDING: Mutex<Option<HashSet<FolderKey>>> = Mutex::new(None);

fn config_path(project_root: &Path) -> PathBuf {
    project_root.join(".ax").join(CONFIG_FILE)
}

/// The configured folders. A missing or unreadable file is an empty list.
pub fn load(project_root: &Path) -> Vec<VaultFolder> {
    std::fs::read(config_path(project_root))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn find(project_root: &Path, name: &str) -> Option<VaultFolder> {
    load(project_root).into_iter().find(|f| f.name == name)
}

/// Check names and paths; paths come back canonical.
pub fn validate(list: Vec<VaultFolder>) -> Result<Vec<VaultFolder>, String> {
    if list.len() > MAX_FOLDERS {
        return Err(format!("at most {MAX_FOLDERS} vault folders"));
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for mut folder in list {
        folder.name = folder.name.trim().to_string();
        if !crate::dav::mount::valid_name(&folder.name) {
            return Err(format!(
                "\"{}\": a name is 1-32 letters, digits, spaces, '-' or '_'",
                folder.name
            ));
        }
        if !seen.insert(folder.name.to_lowercase()) {
            return Err(format!("\"{}\" is used twice", folder.name));
        }
        if !folder.path.is_absolute() {
            return Err(format!(
                "{}: the path must be absolute",
                folder.path.display()
            ));
        }
        let canonical = folder
            .path
            .canonicalize()
            .map_err(|_| format!("{}: the folder does not exist", folder.path.display()))?;
        if !canonical.is_dir() {
            return Err(format!("{}: not a folder", folder.path.display()));
        }
        folder.path = canonical;
        out.push(folder);
    }
    Ok(out)
}

/// Serializes read-modify-write of the folder list (PUT and DELETE).
static CONFIG_WRITE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn save(project_root: &Path, list: &[VaultFolder]) -> std::io::Result<()> {
    let path = config_path(project_root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(list).map_err(std::io::Error::other)?;
    ax_utils::atomic_write(&path, text.as_bytes())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

async fn project_root(hub: &WebHub) -> PathBuf {
    hub.read().await.policy.store.project_root().to_path_buf()
}

/// The served project's root and graph pool, read under one guard so they
/// always belong to the same project.
async fn project(hub: &WebHub) -> (PathBuf, sqlx::SqlitePool) {
    let ws = hub.read().await;
    (
        ws.policy.store.project_root().to_path_buf(),
        ws.graph_pool.clone(),
    )
}

/// Import one folder into memory and remember the result for the Settings list.
async fn sync_into(
    root: &Path,
    pool: &sqlx::SqlitePool,
    folder: &VaultFolder,
) -> Result<FolderSyncReport, String> {
    let report = ax_memory::sync_folder(pool, &folder.name, &folder.path)
        .await
        .map_err(|e| e.to_string())?;
    if let Ok(mut map) = LAST_SYNC.lock() {
        map.insert(
            (root.to_path_buf(), folder.name.clone()),
            LastSync {
                at_ms: now_ms(),
                report,
            },
        );
    }
    Ok(report)
}

/// Sync every folder with indexing on. Used when `ax web` starts.
pub async fn sync_indexed(hub: &WebHub) {
    let (root, pool) = project(hub).await;
    for folder in load(&root).into_iter().filter(|f| f.index) {
        if let Err(e) = sync_into(&root, &pool, &folder).await {
            tracing::warn!("vault folder {} sync failed: {e}", folder.name);
        }
    }
}

/// After a write through the drive: re-sync the folder once writes settle.
pub fn schedule_resync(hub: WebHub, root: PathBuf, name: String) {
    let key = (root.clone(), name.clone());
    {
        let Ok(mut pending) = PENDING.lock() else {
            return;
        };
        if !pending.get_or_insert_with(HashSet::new).insert(key.clone()) {
            return;
        }
    }
    tokio::spawn(async move {
        tokio::time::sleep(WRITE_RESYNC_DELAY).await;
        if let Ok(mut pending) = PENDING.lock() {
            if let Some(set) = pending.as_mut() {
                set.remove(&key);
            }
        }
        // The hub may serve another project by now; its database is not
        // the one this folder belongs to.
        let (current, pool) = project(&hub).await;
        if current != root {
            return;
        }
        if let Some(folder) = find(&root, &name).filter(|f| f.index) {
            if let Err(e) = sync_into(&root, &pool, &folder).await {
                tracing::warn!(folder = %folder.name, "vault folder resync failed: {e}");
            }
        }
    });
}

fn error(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

/// Folder paths and contents are for the local user's own browser only.
fn refuse(hub: &WebHub, headers: &HeaderMap, write: bool) -> Option<Response> {
    if write && hub.readonly {
        return Some(error(StatusCode::FORBIDDEN, "ax web is read-only"));
    }
    (!crate::dav::mount::allowed(headers, false, hub.port)).then(|| {
        error(
            StatusCode::FORBIDDEN,
            "only the local browser on this machine can manage vault folders",
        )
    })
}

async fn list_response(hub: &WebHub) -> Response {
    let root = project_root(hub).await;
    let last = LAST_SYNC.lock().map(|m| m.clone()).unwrap_or_default();
    let folders: Vec<_> = load(&root)
        .into_iter()
        .map(|f| {
            let sync = last.get(&(root.clone(), f.name.clone()));
            json!({ "name": f.name, "path": f.path, "index": f.index, "lastSync": sync })
        })
        .collect();
    Json(json!({ "folders": folders, "readonly": hub.readonly })).into_response()
}

async fn get_folders(State(hub): State<WebHub>, headers: HeaderMap) -> Response {
    if let Some(r) = refuse(&hub, &headers, false) {
        return r;
    }
    list_response(&hub).await
}

async fn put_folders(
    State(hub): State<WebHub>,
    headers: HeaderMap,
    Json(list): Json<Vec<VaultFolder>>,
) -> Response {
    if let Some(r) = refuse(&hub, &headers, true) {
        return r;
    }
    let list = match validate(list) {
        Ok(l) => l,
        Err(e) => return error(StatusCode::BAD_REQUEST, &e),
    };
    let _write = CONFIG_WRITE.lock().await;
    let (root, pool) = project(&hub).await;
    let before = load(&root);
    if let Err(e) = save(&root, &list) {
        return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }
    for old in before.iter().filter(|f| f.index) {
        if !list.iter().any(|f| f.name == old.name && f.index) {
            if let Err(e) = ax_memory::remove_folder_memories(&pool, &old.name).await {
                return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
            }
        }
    }
    let unchanged = |f: &VaultFolder| {
        before
            .iter()
            .any(|o| o.index && o.name == f.name && o.path == f.path)
    };
    for folder in list.iter().filter(|f| f.index && !unchanged(f)) {
        if let Err(e) = sync_into(&root, &pool, folder).await {
            return error(StatusCode::INTERNAL_SERVER_ERROR, &e);
        }
    }
    list_response(&hub).await
}

async fn post_sync(
    State(hub): State<WebHub>,
    headers: HeaderMap,
    UrlPath(name): UrlPath<String>,
) -> Response {
    if let Some(r) = refuse(&hub, &headers, true) {
        return r;
    }
    let (root, pool) = project(&hub).await;
    let Some(folder) = find(&root, &name) else {
        return error(StatusCode::NOT_FOUND, "no such folder");
    };
    if !folder.index {
        return error(
            StatusCode::CONFLICT,
            "indexing is off for this folder; turn it on to sync it",
        );
    }
    match sync_into(&root, &pool, &folder).await {
        Ok(report) => Json(json!({ "report": report })).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

async fn delete_folder(
    State(hub): State<WebHub>,
    headers: HeaderMap,
    UrlPath(name): UrlPath<String>,
) -> Response {
    if let Some(r) = refuse(&hub, &headers, true) {
        return r;
    }
    let _write = CONFIG_WRITE.lock().await;
    let (root, pool) = project(&hub).await;
    let mut list = load(&root);
    let before = list.len();
    list.retain(|f| f.name != name);
    if list.len() == before {
        return error(StatusCode::NOT_FOUND, "no such folder");
    }
    if let Err(e) = save(&root, &list) {
        return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }
    if let Err(e) = ax_memory::remove_folder_memories(&pool, &name).await {
        return error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }
    if let Ok(mut map) = LAST_SYNC.lock() {
        map.remove(&(root, name));
    }
    list_response(&hub).await
}

pub fn router_hub(hub: WebHub) -> Router {
    Router::new()
        .route("/", get(get_folders).put(put_folders))
        .route("/{name}/sync", post(post_sync))
        .route("/{name}", axum::routing::delete(delete_folder))
        .with_state(hub)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn more_than_64_vault_folders_are_rejected() {
        let list = (0..65)
            .map(|i| VaultFolder {
                name: format!("folder{i}"),
                path: PathBuf::from(format!("/tmp/ax-vault-{i}")),
                index: false,
            })
            .collect();
        let err = validate(list).expect_err("65 folders");
        assert!(err.contains("64"), "{err}");
    }
}
