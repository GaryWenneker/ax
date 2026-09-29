//! WebDAV view of rules, skills, and memories, for opening ax as an Obsidian vault.

mod folders;
mod fs;
pub mod mount;
pub mod pages;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{Method, StatusCode, Uri};
use axum::response::IntoResponse;
use axum::routing::any;
use axum::Router;
use dav_server::davpath::DavPath;
use dav_server::fakels::FakeLs;
use dav_server::DavHandler;

use crate::workspace_state::WebHub;
use pages::Target;

pub const MAX_PUT_BYTES: usize = 5 * 1024 * 1024;

/// `/ax` is what the OS names the mounted volume; `/dav` stays for existing vaults.
const PREFIXES: [&str; 2] = ["/dav", "/ax"];

fn prefix_of(raw: &str) -> &'static str {
    if raw == "/ax" || raw.starts_with("/ax/") {
        "/ax"
    } else {
        "/dav"
    }
}

/// Routes for `/dav` and `/ax`. Must not sit behind the CORS layer: browsers may not reach it cross-origin.
pub fn router(hub: WebHub) -> Router {
    PREFIXES.iter().fold(Router::new(), |r, prefix| {
        r.merge(prefix_router(hub.clone(), prefix))
    })
}

fn prefix_router(hub: WebHub, prefix: &'static str) -> Router {
    let readonly = hub.readonly;
    let dav = DavHandler::builder()
        .strip_prefix(prefix)
        .locksystem(FakeLs::new())
        .filesystem(Box::new(fs::DavVault::new(hub)))
        .build_handler();
    let handle = move |req: Request| {
        let dav = dav.clone();
        async move {
            if let Some(status) = refuse(&req, readonly) {
                return status.into_response();
            }
            dav.handle(req).await.map(Body::new).into_response()
        }
    };
    Router::new()
        .route(prefix, any(handle.clone()))
        .route(&format!("{prefix}/"), any(handle.clone()))
        .route(&format!("{prefix}/{{*rest}}"), any(handle))
}

fn is_read(method: &Method) -> bool {
    matches!(method.as_str(), "GET" | "HEAD" | "OPTIONS" | "PROPFIND")
}

/// Vault-relative, percent-decoded path of a request path. `None` if it does not parse.
fn vault_path(raw: &str) -> Option<String> {
    let mut path = DavPath::new(raw).ok()?;
    path.set_prefix(prefix_of(raw)).ok()?;
    path.as_rel_ospath().to_str().map(str::to_string)
}

/// The vault root and the area folders. `dav-server` deletes a folder's
/// children before the folder, so these must be refused before it sees the request.
fn is_protected(raw: &str) -> bool {
    vault_path(raw).is_some_and(|p| {
        matches!(
            pages::classify(&p),
            Target::Root | Target::GlobalDir | Target::AreaDir(_) | Target::FoldersDir
        ) || matches!(pages::classify(&p), Target::Folder(_, rest) if rest.is_empty())
    })
}

/// `folders/...` exposes real directories, so only the local browser may reach it.
fn is_folder(raw: &str) -> bool {
    vault_path(raw)
        .is_some_and(|p| matches!(pages::classify(&p), Target::FoldersDir | Target::Folder(..)))
}

fn is_global_page(raw: &str) -> bool {
    vault_path(raw)
        .is_some_and(|p| matches!(pages::classify(&p), Target::Page(area, _) if area.is_global()))
}

fn refuse(req: &Request, readonly: bool) -> Option<StatusCode> {
    if vault_path(req.uri().path()).is_none() {
        return Some(StatusCode::BAD_REQUEST);
    }
    let destination = req
        .headers()
        .get("destination")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<Uri>().ok())
        .map(|u| u.path().to_string());
    let touches_folder =
        is_folder(req.uri().path()) || destination.as_deref().is_some_and(is_folder);
    if touches_folder && !mount::allowed(req.headers(), false) {
        return Some(StatusCode::FORBIDDEN);
    }
    let method = req.method();
    if is_read(method) {
        return None;
    }
    if readonly {
        return Some(StatusCode::FORBIDDEN);
    }
    if !matches!(method.as_str(), "DELETE" | "MOVE" | "COPY") {
        return None;
    }
    let source = req.uri().path();
    let protected = is_protected(source)
        || destination.as_deref().is_some_and(is_protected)
        || (method.as_str() == "COPY" && is_global_page(source));
    protected.then_some(StatusCode::FORBIDDEN)
}
