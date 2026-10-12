//! Owner-only, workspace-scoped publication export API.
use crate::{local_owner, remote_mcp, workspace_state::WebHub};
use ax_global_db::remote_mcp_publications::{self as catalog, ClientRegistrations, Publication};
use ax_mcp::remote::{
    export::{Client, Transport},
    RemoteConfig,
};
use axum::{
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Extension, Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::path::Path as FilePath;

#[derive(Clone)]
struct OwnerState {
    hub: WebHub,
    pool: Option<SqlitePool>,
}
pub(crate) fn router(hub: WebHub) -> Router {
    router_with_pool(hub, None)
}
fn router_with_pool(hub: WebHub, pool: Option<SqlitePool>) -> Router {
    Router::new()
        .route("/", get(list))
        .route("/import", post(import))
        .route("/local/export", post(local_export))
        .route("/{id}/export", post(export))
        .route("/{id}/connection", post(connection))
        .route("/{id}/check", post(check))
        .route("/{id}", axum::routing::delete(remove))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(middleware::from_fn_with_state(
            OwnerState {
                hub: hub.clone(),
                pool,
            },
            authorize,
        ))
        .with_state(hub)
}
fn response(status: StatusCode, body: Value) -> Response {
    let mut response = (status, Json(body)).into_response();
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}
fn failure(status: StatusCode, error: &str) -> Response {
    response(status, json!({"error":error}))
}
async fn authorize(State(state): State<OwnerState>, mut request: Request, next: Next) -> Response {
    let hub = state.hub;
    if let Err(error) = local_owner::browser_headers(
        request.headers(),
        hub.port,
        hub.readonly,
        request.method() != axum::http::Method::GET,
        remote_mcp::csrf(),
    ) {
        return failure(StatusCode::FORBIDDEN, error);
    }
    if std::env::var("AX_SHARE_TOKEN")
        .ok()
        .is_some_and(|v| !v.is_empty())
    {
        return failure(
            StatusCode::FORBIDDEN,
            "Project publications are unavailable in a share session",
        );
    }
    // Reuse the established local-owner bootstrap for a fresh installation.
    // It never recreates an owner when members already exist (including disabled owners).
    let pool = match state.pool {
        Some(pool) => pool,
        None => match crate::access_api::pool().await {
            Ok(pool) => pool,
            Err(_) => {
                return failure(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Publication database unavailable",
                )
            }
        },
    };
    if let Err(error) = local_owner::require_active_owner(&pool).await {
        return failure(StatusCode::FORBIDDEN, error);
    }
    request.extensions_mut().insert(pool);
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}
/// Public export revision includes client metadata as well as the server snapshot.
fn export_revision(mut p: Publication) -> Publication {
    p.config_fingerprint = catalog::fingerprint(
        format!("{}\0{}", p.config_fingerprint, p.client_registrations).as_bytes(),
    );
    p
}
fn public_value(p: &Publication) -> Value {
    let mut value = serde_json::to_value(p).unwrap_or(Value::Null);
    let clients = p.clients().unwrap_or_default();
    let registration = |id: Option<String>| match id {
        Some(id) => json!({"mode":"pre_registered","clientId":id}),
        None => json!({"mode":"unknown"}),
    };
    value["clientRegistrations"] = json!({"axProxy":registration(clients.ax_proxy),"cursor":registration(clients.cursor),"vscode":registration(clients.vscode)});
    value
}
async fn list(State(hub): State<WebHub>, Extension(pool): Extension<SqlitePool>) -> Response {
    let workspace = hub.read().await;
    let id = match catalog::workspace_id(&workspace.project_root) {
        Ok(id) => id,
        Err(_) => return failure(StatusCode::CONFLICT, "Active project is unavailable"),
    };
    match catalog::list_for_root(&pool, &workspace.project_root).await {
        Ok(publications) => response(
            StatusCode::OK,
            json!({"workspaceId":id,"projectLabel":workspace.project_root.file_name().and_then(|v| v.to_str()).unwrap_or("project"),"csrf":remote_mcp::csrf(),"publications":publications.into_iter().map(export_revision).map(|p| public_value(&p)).collect::<Vec<_>>()}),
        ),
        Err(_) => failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Cannot read project publications",
        ),
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkspaceRequest {
    workspace_id: String,
    config_fingerprint: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalExportRequest {
    workspace_id: String,
    client: crate::local_mcp_export::LocalClient,
    target: crate::local_mcp_export::Target,
}

async fn local_export(State(hub): State<WebHub>, Json(body): Json<LocalExportRequest>) -> Response {
    let workspace = hub.read().await;
    if let Err(error) = matches_workspace(&workspace.project_root, &body.workspace_id) {
        return failure(StatusCode::CONFLICT, error);
    }
    let canonical = match workspace.project_root.canonicalize() {
        Ok(root) => root,
        Err(_) => return failure(StatusCode::CONFLICT, "Active project is unavailable"),
    };
    let root = match canonical.to_str() {
        Some(root) => root,
        None => return failure(StatusCode::BAD_REQUEST, "Project path is not UTF-8"),
    };
    let executable = ax_installer::ax_command::current();
    match crate::local_mcp_export::generate(
        root,
        &executable.path,
        &body.workspace_id,
        body.client,
        body.target,
    ) {
        Ok(value) => response(StatusCode::OK, value),
        Err(error) => failure(StatusCode::BAD_REQUEST, error),
    }
}

async fn connection(
    State(hub): State<WebHub>,
    Extension(pool): Extension<SqlitePool>,
    Path(id): Path<String>,
    Json(body): Json<WorkspaceRequest>,
) -> Response {
    let workspace = hub.read().await;
    if let Err(error) = matches_workspace(&workspace.project_root, &body.workspace_id) {
        return failure(StatusCode::CONFLICT, error);
    }
    let p = match selected(
        &pool,
        &workspace.project_root,
        &id,
        &body.config_fingerprint,
    )
    .await
    {
        Ok(p) => p,
        Err((status, error)) => return failure(status, error),
    };
    let clients = match p.clients() {
        Ok(clients) => clients,
        Err(_) => {
            return failure(
                StatusCode::BAD_REQUEST,
                "Cannot read public client registration",
            )
        }
    };
    let details = json!({"kind":"axRemoteConnection","version":1,"projectMcpUrl":p.endpoint_url,"nativeClientId":clients.ax_proxy});
    response(
        StatusCode::OK,
        json!({"workspaceId":body.workspace_id,"publicationId":p.id,"configFingerprint":p.config_fingerprint,"content":details.to_string(),"containsSecrets":false,"requiresClientId":clients.ax_proxy.is_none()}),
    )
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExportRequest {
    workspace_id: String,
    config_fingerprint: String,
    client: Client,
    transport: Transport,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImportRequest {
    workspace_id: String,
    config: RemoteConfig,
    name: Option<String>,
    clients: Option<ClientRegistrations>,
}
fn matches_workspace(root: &FilePath, requested: &str) -> Result<(), &'static str> {
    if catalog::workspace_id(root).ok().as_deref() != Some(requested) {
        return Err("Workspace changed. Reload project publications.");
    }
    Ok(())
}
async fn selected(
    pool: &SqlitePool,
    root: &FilePath,
    id: &str,
    revision: &str,
) -> Result<Publication, (StatusCode, &'static str)> {
    let p = catalog::list_for_root(pool, root)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Cannot read publications",
            )
        })?
        .into_iter()
        .find(|p| p.id == id)
        .map(export_revision)
        .ok_or((
            StatusCode::NOT_FOUND,
            "Publication is not registered for the active project",
        ))?;
    if p.config_fingerprint != revision {
        return Err((
            StatusCode::CONFLICT,
            "Publication changed. Reload its configuration.",
        ));
    }
    Ok(p)
}
async fn import(
    State(hub): State<WebHub>,
    Extension(pool): Extension<SqlitePool>,
    Json(body): Json<ImportRequest>,
) -> Response {
    // Keep the read lock until registration commits: switching cannot change its root midway.
    let workspace = hub.read().await;
    if let Err(error) = matches_workspace(&workspace.project_root, &body.workspace_id) {
        return failure(StatusCode::CONFLICT, error);
    }
    let snapshot = match ax_mcp::remote::publications::snapshot(
        &body.config,
        body.name.as_deref(),
        body.clients,
        Some(&workspace.project_root),
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => return failure(StatusCode::BAD_REQUEST, &error),
    };
    match catalog::register(&pool, &snapshot, false).await {
        Ok(()) => response(
            StatusCode::OK,
            json!({"workspaceId":body.workspace_id,"registered":true}),
        ),
        Err(error) => failure(StatusCode::CONFLICT, &error.to_string()),
    }
}
async fn export(
    State(hub): State<WebHub>,
    Extension(pool): Extension<SqlitePool>,
    Path(id): Path<String>,
    Json(body): Json<ExportRequest>,
) -> Response {
    let workspace = hub.read().await;
    if let Err(error) = matches_workspace(&workspace.project_root, &body.workspace_id) {
        return failure(StatusCode::CONFLICT, error);
    }
    let p = match selected(
        &pool,
        &workspace.project_root,
        &id,
        &body.config_fingerprint,
    )
    .await
    {
        Ok(p) => p,
        Err((status, error)) => return failure(status, error),
    };
    match ax_mcp::remote::export::generate(&p, &body.workspace_id, body.client, body.transport) {
        Ok(export) => response(
            StatusCode::OK,
            serde_json::to_value(export).unwrap_or(Value::Null),
        ),
        Err(error) => failure(StatusCode::BAD_REQUEST, &error),
    }
}
async fn check(
    State(hub): State<WebHub>,
    Extension(pool): Extension<SqlitePool>,
    Path(id): Path<String>,
    Json(body): Json<WorkspaceRequest>,
) -> Response {
    let workspace = hub.read().await;
    if let Err(error) = matches_workspace(&workspace.project_root, &body.workspace_id) {
        return failure(StatusCode::CONFLICT, error);
    }
    let p = match selected(
        &pool,
        &workspace.project_root,
        &id,
        &body.config_fingerprint,
    )
    .await
    {
        Ok(p) => p,
        Err((status, error)) => return failure(status, error),
    };
    match ax_mcp::remote::probe::check(&p).await {
        Ok(message) => response(
            StatusCode::OK,
            json!({"workspaceId":body.workspace_id,"publicationId":id,"configFingerprint":p.config_fingerprint,"message":message,"state":"signInRequired"}),
        ),
        Err(error) => failure(StatusCode::BAD_GATEWAY, &error),
    }
}
async fn remove(
    State(hub): State<WebHub>,
    Extension(pool): Extension<SqlitePool>,
    Path(id): Path<String>,
    Json(body): Json<WorkspaceRequest>,
) -> Response {
    let workspace = hub.read().await;
    if let Err(error) = matches_workspace(&workspace.project_root, &body.workspace_id) {
        return failure(StatusCode::CONFLICT, error);
    }
    if let Err(r) = selected(
        &pool,
        &workspace.project_root,
        &id,
        &body.config_fingerprint,
    )
    .await
    {
        return failure(r.0, r.1);
    }
    match catalog::remove(&pool, &id, &workspace.project_root).await {
        Ok(true) => response(
            StatusCode::OK,
            json!({"workspaceId":body.workspace_id,"removed":true}),
        ),
        Ok(false) => failure(StatusCode::NOT_FOUND, "Publication is no longer registered"),
        Err(_) => failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Cannot remove local registration",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tower::ServiceExt;
    async fn fixture() -> (
        tempfile::TempDir,
        Router,
        WebHub,
        SqlitePool,
        std::path::PathBuf,
        RemoteConfig,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a/demo");
        let b = temp.path().join("b/demo");
        for root in [&a, &b] {
            std::fs::create_dir_all(root.join(".ax")).unwrap();
            ax_db::Database::open(&root.join(".ax/ax.db"))
                .await
                .unwrap();
        }
        let pool = ax_global_db::open_and_init(&temp.path().join("global.db"))
            .await
            .unwrap();
        ax_global_db::access::bootstrap_owner(&pool, "owner", "Owner", None)
            .await
            .unwrap();
        let config: RemoteConfig = serde_json::from_value(json!({"public_url":"https://ax.example","oauth":{"issuer":"https://issuer.example/","jwks_uri":"https://issuer.example/jwks"},"projects":[{"id":"remote-a","root":a,"subjects":["private-subject"]},{"id":"remote-b","root":b,"subjects":["private-subject"]}]})).unwrap();
        let hub = WebHub::open(a, false, 7070).await.unwrap();
        let router = router_with_pool(hub.clone(), Some(pool.clone()));
        (temp, router, hub, pool, b, config)
    }
    async fn call(
        router: &Router,
        method: &str,
        path: &str,
        body: Value,
        csrf: bool,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("Host", "localhost:7070")
            .header("Content-Type", "application/json");
        if csrf {
            builder = builder.header("X-Ax-Csrf", remote_mcp::csrf());
        }
        let response = router
            .clone()
            .oneshot(builder.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        assert_eq!(response.headers()["cache-control"], "no-store");
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 300_000).await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
    #[tokio::test]
    async fn owner_api_imports_only_current_root_and_rejects_foreign_or_stale_export() {
        let (_temp, router, hub, pool, b, config) = fixture().await;
        let root = hub.read().await.project_root.clone();
        let workspace = catalog::workspace_id(&root).unwrap();
        let body = json!({"workspaceId":workspace,"config":config,"name":"Team","clients":{"axProxy":"native","cursor":"cursor-public","vscode":null}});
        assert_eq!(
            call(&router, "POST", "/import", body.clone(), false)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call(&router, "POST", "/import", body, true).await.0,
            StatusCode::OK
        );
        assert!(catalog::list_for_root(&pool, &b).await.unwrap().is_empty());
        let (_, list) = call(&router, "GET", "/", Value::Null, false).await;
        let public = list.to_string();
        assert!(!public.contains(root.to_str().unwrap()));
        assert!(!public.contains("private-subject"));
        let p = &list["publications"][0];
        let export = json!({"workspaceId":workspace,"configFingerprint":p["configFingerprint"],"client":"cursor","transport":"direct"});
        let endpoint = format!("/{}/export", p["id"].as_str().unwrap());
        let (status, result) = call(&router, "POST", &endpoint, export.clone(), true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(result["containsSecrets"], false);
        assert!(result["content"]
            .as_str()
            .unwrap()
            .contains("cursor-public"));
        assert!(!result.to_string().contains("private-subject"));
        let mut stale = export.clone();
        stale["configFingerprint"] = json!("old-version");
        assert_eq!(
            call(&router, "POST", &endpoint, stale, true).await.0,
            StatusCode::CONFLICT
        );
        hub.switch(b.clone()).await.unwrap();
        assert_eq!(
            call(&router, "POST", &endpoint, export.clone(), true)
                .await
                .0,
            StatusCode::CONFLICT
        );
        let mut foreign = export;
        foreign["workspaceId"] = json!(catalog::workspace_id(&b).unwrap());
        assert_eq!(
            call(&router, "POST", &endpoint, foreign, true).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(catalog::list_for_root(&pool, &root).await.unwrap().len(), 1);
    }
    #[tokio::test]
    async fn owner_api_rejects_bearer_reads_unknown_presets_and_large_imports() {
        let (_temp, router, hub, pool, _b, config) = fixture().await;
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("Host", "localhost:7070")
                    .header("Authorization", "Bearer axc_owner")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let root = hub.read().await.project_root.clone();
        let snapshot =
            ax_mcp::remote::publications::snapshot(&config, None, None, Some(&root)).unwrap();
        catalog::register(&pool, &snapshot, false).await.unwrap();
        let p = export_revision(
            catalog::list_for_root(&pool, &root)
                .await
                .unwrap()
                .remove(0),
        );
        let endpoint = format!("/{}/export", p.id);
        assert_eq!(call(&router, "POST", &endpoint, json!({"workspaceId":catalog::workspace_id(&root).unwrap(),"configFingerprint":p.config_fingerprint,"client":"untrusted","transport":"direct"}), true).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/import")
                    .header("Host", "localhost:7070")
                    .header("Content-Type", "application/json")
                    .header("X-Ax-Csrf", remote_mcp::csrf())
                    .body(Body::from("x".repeat(256 * 1024 + 1)))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(response.headers()["cache-control"], "no-store");
        sqlx::query("UPDATE team_members SET status='disabled'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            call(&router, "GET", "/", Value::Null, false).await.0,
            StatusCode::FORBIDDEN
        );
    }
    #[tokio::test]
    async fn local_setup_is_available_without_remote_registration_and_rejects_stale_workspace() {
        let (_temp, router, hub, _pool, b, _config) = fixture().await;
        let root = hub.read().await.project_root.canonicalize().unwrap();
        let id = catalog::workspace_id(&root).unwrap();
        let body = json!({"workspaceId":id,"client":"cursor","target":"thisComputer"});
        assert_eq!(
            call(&router, "POST", "/local/export", body.clone(), false)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        let (status, result) = call(&router, "POST", "/local/export", body.clone(), true).await;
        assert_eq!(status, StatusCode::OK);
        let config: Value = serde_json::from_str(result["content"].as_str().unwrap()).unwrap();
        assert_eq!(
            config["mcpServers"]["ax"]["args"][3],
            root.to_str().unwrap()
        );
        let (_, shared) = call(
            &router,
            "POST",
            "/local/export",
            json!({"workspaceId":id,"client":"cursor","target":"projectFolder"}),
            true,
        )
        .await;
        assert!(!shared["content"]
            .as_str()
            .unwrap()
            .contains(root.to_str().unwrap()));
        assert_eq!(shared["portable"], true);
        hub.switch(b).await.unwrap();
        assert_eq!(
            call(&router, "POST", "/local/export", body, true).await.0,
            StatusCode::CONFLICT
        );
    }
    #[tokio::test]
    async fn remote_transfer_exports_native_public_id_without_credentials_or_host_paths() {
        let (_temp, router, hub, pool, _b, config) = fixture().await;
        let root = hub.read().await.project_root.clone();
        let snapshot = ax_mcp::remote::publications::snapshot(
            &config,
            None,
            Some(ClientRegistrations {
                ax_proxy: Some("native-public".into()),
                cursor: Some("different-cursor-id".into()),
                vscode: None,
            }),
            Some(&root),
        )
        .unwrap();
        catalog::register(&pool, &snapshot, false).await.unwrap();
        let p = export_revision(
            catalog::list_for_root(&pool, &root)
                .await
                .unwrap()
                .remove(0),
        );
        let url = format!("/{}/connection", p.id);
        let body = json!({"workspaceId":catalog::workspace_id(&root).unwrap(),"configFingerprint":p.config_fingerprint});
        let (status, result) = call(&router, "POST", &url, body.clone(), true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(result["requiresClientId"], false);
        let details: Value = serde_json::from_str(result["content"].as_str().unwrap()).unwrap();
        assert_eq!(details["nativeClientId"], "native-public");
        assert_eq!(
            details["projectMcpUrl"],
            "https://ax.example/projects/remote-a/mcp"
        );
        assert!(!result.to_string().contains(root.to_str().unwrap()));
        assert!(!result.to_string().contains("different-cursor-id"));
        assert!(!result.to_string().contains("private-subject"));
        let mut stale = body;
        stale["configFingerprint"] = json!("stale");
        assert_eq!(
            call(&router, "POST", &url, stale, true).await.0,
            StatusCode::CONFLICT
        );
    }
}
