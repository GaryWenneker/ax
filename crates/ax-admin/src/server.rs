//! Loopback HTTP API for the browser UI. It never returns a key or token.
use crate::{
    login::{LoginBackend, LoginState},
    mcp::{McpError, Remote, RemoteInfo, StaticToken, TokenSource},
    secrets::SecretStore,
    store::{default_name, project_url, AuthKind, SavedConnection, Store},
};
use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use include_dir::{include_dir, Dir};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use tokio::sync::Mutex;

static UI: Dir<'_> = include_dir!("$OUT_DIR/admin_dist");

pub const CSRF_HEADER: &str = "x-ax-admin-csrf";
const MAX_BODY: usize = 256 * 1024;
const LOGIN_TTL: Duration = Duration::from_secs(900);

struct PendingLogin {
    name: String,
    url: String,
    client_id: String,
    write: bool,
    started: Instant,
}

pub struct App {
    store: Mutex<Store>,
    secrets: Arc<dyn SecretStore>,
    logins: Arc<dyn LoginBackend>,
    remotes: Mutex<HashMap<String, Arc<Remote>>>,
    pending: Mutex<HashMap<String, PendingLogin>>,
    csrf: String,
    port: u16,
}

impl App {
    /// `port` is the loopback port the listener is bound to; requests naming
    /// any other host are refused.
    pub fn new(
        home: PathBuf,
        secrets: Arc<dyn SecretStore>,
        logins: Arc<dyn LoginBackend>,
        port: u16,
    ) -> Result<Arc<Self>, String> {
        Ok(Arc::new(Self {
            store: Mutex::new(Store::open(&home)?),
            secrets,
            logins,
            remotes: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            csrf: format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple()),
            port,
        }))
    }

    fn allowed_hosts(&self) -> [String; 3] {
        [
            format!("127.0.0.1:{}", self.port),
            format!("localhost:{}", self.port),
            format!("[::1]:{}", self.port),
        ]
    }
}

/// Binds the listener. ax-admin has no option for a non-loopback address.
pub async fn bind(port: u16) -> Result<tokio::net::TcpListener, String> {
    tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| format!("Cannot listen on 127.0.0.1:{port}: {e}"))
}

struct ApiError(StatusCode, &'static str, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"code": self.1, "error": self.2}))).into_response()
    }
}

impl From<McpError> for ApiError {
    fn from(e: McpError) -> Self {
        let status = match e {
            McpError::UnknownTool(_) => StatusCode::NOT_FOUND,
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        };
        Self(status, e.code(), e.message())
    }
}

fn bad_request(message: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, "invalid_request", message.into())
}
fn internal(message: impl Into<String>) -> ApiError {
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, "local_error", message.into())
}
fn unknown_connection() -> ApiError {
    ApiError(StatusCode::NOT_FOUND, "unknown_connection", "Unknown connection".into())
}

type Api<T = Value> = Result<(StatusCode, Json<T>), ApiError>;

fn ok(value: Value) -> Api {
    Ok((StatusCode::OK, Json(value)))
}

async fn blocking<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, ApiError> {
    // The OS credential store may block (and on Linux drives its own executor).
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|_| internal("Credential store task failed"))?
        .map_err(internal)
}

/// Rejects requests that do not come from this application's own page.
async fn guard(State(app): State<Arc<App>>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    let hosts = app.allowed_hosts();
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    let forbidden = |message: &str| {
        let mut response = ApiError(StatusCode::FORBIDDEN, "forbidden", message.into()).into_response();
        no_store(&mut response);
        response
    };
    if !host.is_some_and(|host| hosts.iter().any(|allowed| allowed == host)) {
        return forbidden("Request host is not allowed");
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        let allowed = origin
            .to_str()
            .ok()
            .and_then(|origin| origin.strip_prefix("http://"))
            .is_some_and(|origin| hosts.iter().any(|allowed| allowed == origin));
        if !allowed {
            return forbidden("Request origin is not allowed");
        }
    }
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        let token = headers
            .get(CSRF_HEADER)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if token.len() != app.csrf.len() || !bool::from(token.as_bytes().ct_eq(app.csrf.as_bytes())) {
            return forbidden("Missing or invalid request token; reload the page");
        }
    }
    let mut response = next.run(request).await;
    no_store(&mut response);
    response
}

fn no_store(response: &mut Response) {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

pub fn router(app: Arc<App>) -> Router {
    let api = Router::new()
        .route("/connections", get(list_connections).post(create_connection))
        .route("/connections/test", post(test_connection))
        .route("/connections/{id}", put(update_connection).delete(remove_connection))
        .route("/connections/{id}/activate", post(activate_connection))
        .route("/login", post(start_login))
        .route("/login/{id}", get(login_status).delete(cancel_login))
        .route("/remote/info", get(remote_info))
        .route("/remote/tools/{name}", post(call_tool))
        .fallback(|| async {
            ApiError(StatusCode::NOT_FOUND, "not_found", "Unknown API route".into())
        });
    Router::new()
        .nest("/api", api)
        .fallback(static_file)
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY))
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .with_state(app)
}

async fn static_file(request: Request) -> Response {
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let path = request.uri().path().trim_start_matches('/');
    // Unknown paths are client-side routes of the single-page UI.
    let file = UI
        .get_file(path)
        .filter(|_| !path.is_empty())
        .or_else(|| UI.get_file("index.html"));
    let Some(file) = file else {
        return (StatusCode::NOT_FOUND, "ax-admin was built without its UI").into_response();
    };
    let kind = match file.path().extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("json") => "application/json; charset=utf-8",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    };
    Response::builder()
        .header(header::CONTENT_TYPE, kind)
        .header("x-content-type-options", "nosniff")
        .header(
            "content-security-policy",
            "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
        )
        .body(Body::from(file.contents()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn connection_json(connection: &SavedConnection, active: Option<&str>) -> Value {
    json!({
        "id": connection.id,
        "name": connection.name,
        "url": connection.url,
        "auth": connection.auth,
        "clientId": connection.client_id,
        "write": connection.write,
        "active": active == Some(connection.id.as_str()),
    })
}

async fn list_connections(State(app): State<Arc<App>>) -> Api {
    let store = app.store.lock().await;
    let active = store.active_id();
    ok(json!({
        "csrf": app.csrf,
        "activeId": active,
        "connections": store.list().iter().map(|c| connection_json(c, active)).collect::<Vec<_>>(),
    }))
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum AuthInput {
    ApiKey { key: String },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectionInput {
    /// Test or edit a saved connection with its stored credential.
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    auth: Option<AuthInput>,
}

fn parse<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(|_| bad_request("The request is not valid"))
}

fn api_key(auth: Option<AuthInput>) -> Result<String, ApiError> {
    let Some(AuthInput::ApiKey { key }) = auth else {
        return Err(bad_request("Enter the credentials for this environment"));
    };
    let key = key.trim().to_owned();
    if key.is_empty() {
        return Err(bad_request("Enter the credentials for this environment"));
    }
    if key.len() > 16384 || key.bytes().any(|b| b.is_ascii_whitespace() || b.is_ascii_control()) {
        return Err(bad_request("The key contains spaces or is too long"));
    }
    Ok(key)
}

fn connection_name(name: Option<String>, url: &url::Url) -> Result<String, ApiError> {
    let name = name.unwrap_or_default().trim().to_owned();
    if name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(bad_request("Use a name of at most 80 characters"));
    }
    Ok(if name.is_empty() { default_name(url) } else { name })
}

fn info_json(info: &RemoteInfo) -> Value {
    json!({
        "serverName": info.server_name,
        "serverVersion": info.server_version,
        "protocolVersion": info.protocol_version,
        "tools": info.tools,
    })
}

async fn probe(url: url::Url, tokens: Arc<dyn TokenSource>) -> Result<(Arc<Remote>, RemoteInfo), ApiError> {
    let remote = Arc::new(Remote::new(url, tokens)?);
    let info = remote.info().await?;
    Ok((remote, info))
}

impl App {
    async fn saved(&self, id: &str) -> Result<SavedConnection, ApiError> {
        self.store
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or_else(unknown_connection)
    }

    async fn stored_tokens(&self, connection: &SavedConnection) -> Result<Arc<dyn TokenSource>, ApiError> {
        match connection.auth {
            AuthKind::Oauth => Ok(self.logins.tokens(&connection.url)),
            AuthKind::ApiKey => {
                let secrets = self.secrets.clone();
                let id = connection.id.clone();
                let key = blocking(move || secrets.get(&id)).await?.ok_or_else(|| {
                    ApiError(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "credential_unavailable",
                        "The key for this connection is no longer in the OS credential store. Edit the connection and enter it again.".into(),
                    )
                })?;
                Ok(Arc::new(StaticToken(key)))
            }
        }
    }

    async fn remote(&self, connection: &SavedConnection) -> Result<Arc<Remote>, ApiError> {
        if let Some(remote) = self.remotes.lock().await.get(&connection.id) {
            return Ok(remote.clone());
        }
        let url = project_url(&connection.url).map_err(internal)?;
        let remote = Arc::new(Remote::new(url, self.stored_tokens(connection).await?)?);
        self.remotes
            .lock()
            .await
            .insert(connection.id.clone(), remote.clone());
        Ok(remote)
    }

    async fn active_remote(&self) -> Result<(SavedConnection, Arc<Remote>), ApiError> {
        let connection = self.store.lock().await.active().cloned().ok_or_else(|| {
            ApiError(
                StatusCode::CONFLICT,
                "no_active_connection",
                "Connect to an Ax environment first".into(),
            )
        })?;
        let remote = self.remote(&connection).await?;
        Ok((connection, remote))
    }
}

async fn test_connection(State(app): State<Arc<App>>, body: axum::body::Bytes) -> Api {
    let input: ConnectionInput = parse(&body)?;
    let (url, tokens): (url::Url, Arc<dyn TokenSource>) = match (&input.id, input.auth) {
        // A saved connection, optionally with a changed address.
        (Some(id), None) => {
            let saved = app.saved(id).await?;
            let url = project_url(input.url.as_deref().unwrap_or(&saved.url)).map_err(bad_request)?;
            (url, app.stored_tokens(&saved).await?)
        }
        (_, auth) => {
            let url = project_url(input.url.as_deref().unwrap_or("")).map_err(bad_request)?;
            (url, Arc::new(StaticToken(api_key(auth)?)))
        }
    };
    // The form reports the same input errors for Test as for Save.
    if input.name.is_some() {
        connection_name(input.name, &url)?;
    }
    let (_, info) = probe(url, tokens).await?;
    ok(json!({"ok": true, "info": info_json(&info)}))
}

async fn create_connection(State(app): State<Arc<App>>, body: axum::body::Bytes) -> Api {
    let input: ConnectionInput = parse(&body)?;
    if input.id.is_some() {
        return Err(bad_request("A new connection has no id"));
    }
    let url = project_url(input.url.as_deref().unwrap_or("")).map_err(bad_request)?;
    let key = api_key(input.auth)?;
    let name = connection_name(input.name, &url)?;
    // Nothing is stored until the server has accepted the address and key.
    let (remote, info) = probe(url.clone(), Arc::new(StaticToken(key.clone()))).await?;
    let connection = SavedConnection {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        url: url.to_string(),
        auth: AuthKind::ApiKey,
        client_id: None,
        write: false,
    };
    let secrets = app.secrets.clone();
    let id = connection.id.clone();
    blocking(move || secrets.set(&id, &key)).await?;
    if let Err(e) = app.store.lock().await.upsert(connection.clone(), true) {
        let secrets = app.secrets.clone();
        let id = connection.id.clone();
        let _ = blocking(move || secrets.delete(&id)).await;
        return Err(internal(e));
    }
    app.remotes.lock().await.insert(connection.id.clone(), remote);
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "connection": connection_json(&connection, Some(&connection.id)),
            "info": info_json(&info),
        })),
    ))
}

async fn update_connection(
    State(app): State<Arc<App>>,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Api {
    let input: ConnectionInput = parse(&body)?;
    let saved = app.saved(&id).await?;
    let url = match input.url.as_deref() {
        Some(url) => project_url(url).map_err(bad_request)?,
        None => project_url(&saved.url).map_err(internal)?,
    };
    let url_changed = url.as_str() != saved.url;
    if saved.auth == AuthKind::Oauth && (url_changed || input.auth.is_some()) {
        return Err(bad_request(
            "A browser sign-in belongs to one address. Remove this connection and sign in to the other address.",
        ));
    }
    let new_key = match input.auth {
        Some(auth) => Some(api_key(Some(auth))?),
        None => None,
    };
    let mut updated = saved.clone();
    updated.url = url.to_string();
    if let Some(name) = input.name {
        updated.name = connection_name(Some(name), &url)?;
    }
    if url_changed || new_key.is_some() {
        let tokens: Arc<dyn TokenSource> = match &new_key {
            Some(key) => Arc::new(StaticToken(key.clone())),
            None => app.stored_tokens(&saved).await?,
        };
        probe(url, tokens).await?;
    }
    if let Some(key) = new_key {
        let secrets = app.secrets.clone();
        let id = id.clone();
        blocking(move || secrets.set(&id, &key)).await?;
    }
    let mut store = app.store.lock().await;
    store.upsert(updated.clone(), false).map_err(internal)?;
    let active = store.active_id().map(str::to_owned);
    drop(store);
    // The cached client holds the old address and key.
    app.remotes.lock().await.remove(&id);
    ok(json!({"connection": connection_json(&updated, active.as_deref())}))
}

async fn remove_connection(State(app): State<Arc<App>>, Path(id): Path<String>) -> Api {
    let removed = app
        .store
        .lock()
        .await
        .remove(&id)
        .map_err(internal)?
        .ok_or_else(unknown_connection)?;
    app.remotes.lock().await.remove(&id);
    match removed.auth {
        AuthKind::ApiKey => {
            let secrets = app.secrets.clone();
            let id = id.clone();
            blocking(move || secrets.delete(&id)).await?;
        }
        AuthKind::Oauth => {
            // Another saved connection may use the same sign-in.
            let shared = app
                .store
                .lock()
                .await
                .list()
                .iter()
                .any(|c| c.auth == AuthKind::Oauth && c.url == removed.url);
            if !shared {
                app.logins.logout(&removed.url).await.map_err(internal)?;
            }
        }
    }
    let active = app.store.lock().await.active_id().map(str::to_owned);
    ok(json!({"removed": id, "activeId": active}))
}

async fn activate_connection(State(app): State<Arc<App>>, Path(id): Path<String>) -> Api {
    let mut store = app.store.lock().await;
    if store.get(&id).is_none() {
        return Err(unknown_connection());
    }
    store.activate(&id).map_err(internal)?;
    ok(json!({"activeId": id}))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LoginInput {
    #[serde(default)]
    name: Option<String>,
    url: String,
    client_id: String,
    #[serde(default)]
    write: bool,
}

async fn start_login(State(app): State<Arc<App>>, body: axum::body::Bytes) -> Api {
    let input: LoginInput = parse(&body)?;
    let url = project_url(&input.url).map_err(bad_request)?;
    if url.scheme() != "https" {
        return Err(bad_request("Browser sign-in needs an https:// address"));
    }
    let client_id = input.client_id.trim().to_owned();
    if client_id.is_empty() || client_id.len() > 512 || client_id.chars().any(char::is_control) {
        return Err(bad_request(
            "Enter the public client ID from the environment's connection details",
        ));
    }
    let name = connection_name(input.name, &url)?;
    let started = app
        .logins
        .start(url.to_string(), client_id.clone(), input.write)
        .await
        .map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, "sign_in_failed", e))?;
    let mut pending = app.pending.lock().await;
    pending.retain(|_, p| p.started.elapsed() < LOGIN_TTL);
    pending.insert(
        started.id.clone(),
        PendingLogin {
            name,
            url: url.to_string(),
            client_id,
            write: input.write,
            started: Instant::now(),
        },
    );
    ok(json!({"id": started.id, "authorizationUrl": started.authorization_url}))
}

async fn login_status(State(app): State<Arc<App>>, Path(id): Path<String>) -> Api {
    if !app.pending.lock().await.contains_key(&id) {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "unknown_sign_in",
            "This sign-in is no longer active. Start again.".into(),
        ));
    }
    match app.logins.status(&id).await {
        Some(LoginState::Pending) => ok(json!({"state": "pending"})),
        Some(LoginState::Connected) => {
            // Taken out first, so two polls cannot save the connection twice.
            let Some(login) = app.pending.lock().await.remove(&id) else {
                return ok(json!({"state": "pending"}));
            };
            let connection = SavedConnection {
                id: uuid::Uuid::new_v4().to_string(),
                name: login.name,
                url: login.url,
                auth: AuthKind::Oauth,
                client_id: Some(login.client_id),
                write: login.write,
            };
            let url = project_url(&connection.url).map_err(internal)?;
            let probed = probe(url, app.logins.tokens(&connection.url)).await;
            let (remote, info) = match probed {
                Ok(v) => v,
                Err(e) => {
                    let _ = app.logins.logout(&connection.url).await;
                    return ok(json!({"state": "failed", "message": e.2}));
                }
            };
            app.store
                .lock()
                .await
                .upsert(connection.clone(), true)
                .map_err(internal)?;
            app.remotes.lock().await.insert(connection.id.clone(), remote);
            ok(json!({
                "state": "connected",
                "connection": connection_json(&connection, Some(&connection.id)),
                "info": info_json(&info),
            }))
        }
        Some(LoginState::Failed(message)) => {
            app.pending.lock().await.remove(&id);
            ok(json!({"state": "failed", "message": message}))
        }
        None => {
            app.pending.lock().await.remove(&id);
            ok(json!({"state": "failed", "message": "The sign-in expired. Start again."}))
        }
    }
}

async fn cancel_login(State(app): State<Arc<App>>, Path(id): Path<String>) -> Api {
    if app.pending.lock().await.remove(&id).is_none() {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "unknown_sign_in",
            "This sign-in is no longer active.".into(),
        ));
    }
    // Already finished or expired attempts have nothing left to cancel.
    let _ = app.logins.cancel(&id).await;
    ok(json!({"cancelled": true}))
}

async fn remote_info(State(app): State<Arc<App>>) -> Api {
    let (connection, remote) = app.active_remote().await?;
    let info = remote.info().await?;
    let mut value = info_json(&info);
    value["connection"] = connection_json(&connection, Some(&connection.id));
    ok(value)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CallInput {
    #[serde(default)]
    arguments: Option<Value>,
}

async fn call_tool(
    State(app): State<Arc<App>>,
    Path(name): Path<String>,
    body: axum::body::Bytes,
) -> Api {
    let input: CallInput = if body.is_empty() {
        CallInput { arguments: None }
    } else {
        parse(&body)?
    };
    let arguments = input.arguments.unwrap_or_else(|| json!({}));
    if !arguments.is_object() {
        return Err(bad_request("Tool arguments must be an object"));
    }
    let (_, remote) = app.active_remote().await?;
    let result = remote.call_tool(&name, arguments).await?;
    let text = result
        .get("content")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    ok(json!({
        "text": text,
        "structuredContent": result.get("structuredContent").cloned().unwrap_or(Value::Null),
        "isError": result.get("isError").and_then(Value::as_bool).unwrap_or(false),
    }))
}
