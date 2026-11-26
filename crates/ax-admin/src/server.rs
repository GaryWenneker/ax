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
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::sync::Mutex;

static UI: Dir<'_> = include_dir!("$OUT_DIR/admin_dist");

pub const CSRF_HEADER: &str = "x-ax-admin-csrf";
const MAX_BODY: usize = 256 * 1024;
const LOGIN_TTL: Duration = Duration::from_secs(900);
const ABANDON_POLL: Duration = Duration::from_millis(500);
/// The page saves a finished sign-in within a second or two.
const SIGN_IN_GRACE: Duration = Duration::from_secs(60);

#[derive(Clone)]
struct PendingLogin {
    name: String,
    url: String,
    client_id: String,
    write: bool,
}

/// Cached clients, and a counter per connection that changes whenever its
/// address or key changes. A client built from what was read before a change
/// is not cached, so a key is never paired with an address it was not saved for.
#[derive(Default)]
struct Remotes {
    clients: HashMap<String, Arc<Remote>>,
    changes: HashMap<String, u64>,
}

impl Remotes {
    fn version(&self, id: &str) -> u64 {
        self.changes.get(id).copied().unwrap_or(0)
    }
    /// Forgets the client of a connection whose address or key is changing.
    fn invalidate(&mut self, id: &str) {
        *self.changes.entry(id.to_owned()).or_default() += 1;
        self.clients.remove(id);
    }
}

pub struct App {
    /// File-backed, so it is only touched on the blocking pool (`App::store`).
    store: Arc<std::sync::Mutex<Store>>,
    secrets: Arc<dyn SecretStore>,
    logins: Arc<dyn LoginBackend>,
    /// One client per connection. The lock is only ever held for a moment:
    /// nothing slow (network, credential store) happens under it.
    remotes: Mutex<Remotes>,
    /// How long a finished sign-in may wait to be saved before it is dropped.
    sign_in_grace: Duration,
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
        Self::with_sign_in_grace(home, secrets, logins, port, SIGN_IN_GRACE)
    }

    /// Like `new`, with another wait before a finished sign-in that the page
    /// never saved is dropped.
    pub fn with_sign_in_grace(
        home: PathBuf,
        secrets: Arc<dyn SecretStore>,
        logins: Arc<dyn LoginBackend>,
        port: u16,
        sign_in_grace: Duration,
    ) -> Result<Arc<Self>, String> {
        Ok(Arc::new(Self {
            store: Arc::new(std::sync::Mutex::new(Store::open(&home)?)),
            secrets,
            logins,
            remotes: Mutex::new(Remotes::default()),
            sign_in_grace,
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
            // The environment could not be reached, or something else answered.
            McpError::Unreachable | McpError::Redirect | McpError::NotMcp(_) => StatusCode::BAD_GATEWAY,
            McpError::Timeout | McpError::StillRunning => StatusCode::GATEWAY_TIMEOUT,
            McpError::Busy => StatusCode::TOO_MANY_REQUESTS,
            McpError::Reindexing => StatusCode::SERVICE_UNAVAILABLE,
            // The environment answered and said no.
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        };
        Self(status, e.code(), e.message())
    }
}

/// A local failure that leaves something behind. Never contains a secret.
fn warn(what: &str, error: &str) {
    eprintln!("ax-admin: {what}: {error}");
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
        .route("/login/{id}/complete", post(complete_login))
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
    let (connections, active) = app
        .store(|store| (store.list().to_vec(), store.active_id().map(str::to_owned)))
        .await?;
    ok(json!({
        "csrf": app.csrf,
        "activeId": active,
        "connections": connections.iter().map(|c| connection_json(c, active.as_deref())).collect::<Vec<_>>(),
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
    /// Runs `work` on the saved connections, off the async threads.
    async fn store<T: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Store) -> T + Send + 'static,
    ) -> Result<T, ApiError> {
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || {
            // A panic elsewhere cannot leave the store half-changed: it only
            // changes after a successful write.
            let mut guard = store.lock().unwrap_or_else(|e| e.into_inner());
            work(&mut guard)
        })
        .await
        .map_err(|_| internal("Configuration task failed"))
    }

    async fn saved(&self, id: &str) -> Result<SavedConnection, ApiError> {
        let id = id.to_owned();
        self.store(move |store| store.get(&id).cloned())
            .await?
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

    /// The client of the active connection.
    async fn active_remote(&self) -> Result<(SavedConnection, Arc<Remote>), ApiError> {
        for _ in 0..4 {
            // The counters are noted before anything is read. A change that
            // happens after this point, however quickly, makes the check at
            // the end fail, so nothing read here can be cached stale.
            let versions = self.remotes.lock().await.changes.clone();
            let connection = self
                .store(|store| store.active().cloned())
                .await?
                .ok_or_else(|| {
                    ApiError(
                        StatusCode::CONFLICT,
                        "no_active_connection",
                        "Connect to an Ax environment first".into(),
                    )
                })?;
            let version = versions.get(&connection.id).copied().unwrap_or(0);
            if let Some(remote) = self.remotes.lock().await.clients.get(&connection.id) {
                return Ok((connection, remote.clone()));
            }
            // Slow: the credential store may ask the user to unlock it.
            let url = project_url(&connection.url).map_err(internal)?;
            let remote = Arc::new(Remote::new(url, self.stored_tokens(&connection).await?)?);
            let mut remotes = self.remotes.lock().await;
            if remotes.version(&connection.id) == version {
                remotes.clients.insert(connection.id.clone(), remote.clone());
                return Ok((connection, remote));
            }
            // The connection changed meanwhile: read it again.
        }
        Err(ApiError(
            StatusCode::CONFLICT,
            "connection_changing",
            "This connection is being changed. Try again.".into(),
        ))
    }

    /// Whether a saved connection (other than `except`) uses the sign-in for `url`.
    async fn sign_in_is_used(&self, url: &str, except: Option<&str>) -> Result<bool, ApiError> {
        let (url, except) = (url.to_owned(), except.map(str::to_owned));
        self.store(move |store| {
            store.list().iter().any(|c| {
                c.auth == AuthKind::Oauth && c.url == url && Some(c.id.as_str()) != except.as_deref()
            })
        })
        .await
    }

    /// For the end of an attempt that stored nothing itself: an earlier attempt
    /// for the same address may have left tokens that were kept only because
    /// this one was still open. Having nothing to remove is the normal case.
    async fn drop_leftover_sign_in(&self, url: &str) {
        if self.pending.lock().await.values().any(|p| p.url == url) {
            return;
        }
        if let Ok(false) = self.sign_in_is_used(url, None).await {
            // "Not signed in" is the expected answer here.
            let _ = self.logins.logout(url).await;
        }
    }

    /// Forgets the tokens of a sign-in that no saved connection uses.
    async fn drop_unused_sign_in(&self, url: &str) {
        // A newer attempt for the same address may have just stored them.
        if self.pending.lock().await.values().any(|p| p.url == url) {
            return;
        }
        match self.sign_in_is_used(url, None).await {
            Ok(true) => {}
            Ok(false) => {
                if let Err(e) = self.logins.logout(url).await {
                    warn("tokens of an unused sign-in were not removed", &e);
                }
            }
            Err(e) => warn("could not check whether a sign-in is still used", &e.2),
        }
    }
}

/// Follows one sign-in to its end. The page normally saves it within a second
/// of the browser answering. If the page is gone by then (closed, reloaded,
/// lost its connection to ax-admin), the attempt is dropped after a grace
/// period and its tokens removed, so no sign-in is left behind unseen.
fn watch_sign_in(app: Arc<App>, id: String, url: String) {
    tokio::spawn(async move {
        for _ in 0..(LOGIN_TTL.as_secs() * 2) {
            // Saved or cancelled meanwhile: those paths clean up themselves.
            if !app.pending.lock().await.contains_key(&id) {
                return;
            }
            match app.logins.status(&id).await {
                Some(LoginState::Pending) => tokio::time::sleep(ABANDON_POLL).await,
                Some(LoginState::Connected) => {
                    tokio::time::sleep(app.sign_in_grace).await;
                    if app.pending.lock().await.remove(&id).is_some() {
                        app.drop_unused_sign_in(&url).await;
                    }
                    return;
                }
                Some(LoginState::Failed(_)) | None => {
                    app.pending.lock().await.remove(&id);
                    app.drop_leftover_sign_in(&url).await;
                    return;
                }
            }
        }
        app.pending.lock().await.remove(&id);
        app.drop_leftover_sign_in(&url).await;
    });
}

/// A sign-in the user gave up on may still complete in the browser. Wait for
/// its outcome and remove the tokens it stored, so nothing is left behind.
fn abandon(app: Arc<App>, id: String, url: String) {
    tokio::spawn(async move {
        for _ in 0..(LOGIN_TTL.as_secs() * 2) {
            match app.logins.status(&id).await {
                Some(LoginState::Pending) => tokio::time::sleep(ABANDON_POLL).await,
                Some(LoginState::Connected) => {
                    app.drop_unused_sign_in(&url).await;
                    return;
                }
                Some(LoginState::Failed(_)) | None => return,
            }
        }
    });
}

async fn test_connection(State(app): State<Arc<App>>, body: axum::body::Bytes) -> Api {
    let input: ConnectionInput = parse(&body)?;
    let (url, tokens): (url::Url, Arc<dyn TokenSource>) = match input.id {
        // A saved credential is only ever sent to the address it was saved for.
        Some(id) => {
            if input.url.is_some() || input.auth.is_some() || input.name.is_some() {
                return Err(bad_request(
                    "A saved connection is tested as it is saved. To try another address or key, enter both.",
                ));
            }
            let saved = app.saved(&id).await?;
            let url = project_url(&saved.url).map_err(internal)?;
            (url, app.stored_tokens(&saved).await?)
        }
        None => {
            let url = project_url(input.url.as_deref().unwrap_or("")).map_err(bad_request)?;
            let key = api_key(input.auth)?;
            // The form reports the same input errors for Test as for Save.
            connection_name(input.name, &url)?;
            (url, Arc::new(StaticToken(key)))
        }
    };
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
    let saved = connection.clone();
    if let Err(e) = app.store(move |store| store.upsert(saved, true)).await.and_then(|r| r.map_err(internal)) {
        // Without its entry the key would be unreachable: take it out again.
        let secrets = app.secrets.clone();
        let id = connection.id.clone();
        if let Err(undo) = blocking(move || secrets.delete(&id)).await {
            warn("the key of a connection that could not be saved was not removed", &undo.2);
        }
        return Err(e);
    }
    app.remotes.lock().await.clients.insert(connection.id.clone(), remote);
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
    // A saved key belongs to the address it was saved for. It is never sent
    // to another address; the user enters the key for the new one.
    if url_changed && new_key.is_none() {
        return Err(bad_request("Enter the key again to use another address"));
    }
    let mut updated = saved.clone();
    updated.url = url.to_string();
    if let Some(name) = input.name {
        updated.name = connection_name(Some(name), &url)?;
    }
    if let Some(key) = &new_key {
        probe(url, Arc::new(StaticToken(key.clone()))).await?;
    }
    // From here the key and the entry change. A client built in between is
    // not cached (see `Remotes`), and none is left over afterwards.
    app.remotes.lock().await.invalidate(&id);
    let previous_key = match &new_key {
        Some(key) => {
            let (secrets, id, key) = (app.secrets.clone(), id.clone(), key.clone());
            Some(
                blocking(move || {
                    let previous = secrets.get(&id)?;
                    secrets.set(&id, &key)?;
                    Ok(previous)
                })
                .await?,
            )
        }
        None => None,
    };
    let to_save = updated.clone();
    let result = app
        .store(move |store| {
            store
                .upsert(to_save, false)
                .map(|()| store.active_id().map(str::to_owned))
        })
        .await
        .and_then(|r| r.map_err(internal));
    let active = match result {
        Ok(active) => active,
        Err(e) => {
            // The entry still names the old address: put its key back.
            if let Some(previous) = previous_key {
                let (secrets, id) = (app.secrets.clone(), id.clone());
                let undo = blocking(move || match previous {
                    Some(key) => secrets.set(&id, &key),
                    None => secrets.delete(&id),
                })
                .await;
                if let Err(undo) = undo {
                    warn("the previous key of a connection could not be restored", &undo.2);
                }
            }
            app.remotes.lock().await.invalidate(&id);
            return Err(e);
        }
    };
    app.remotes.lock().await.invalidate(&id);
    ok(json!({"connection": connection_json(&updated, active.as_deref())}))
}

async fn remove_connection(State(app): State<Arc<App>>, Path(id): Path<String>) -> Api {
    let saved = app.saved(&id).await?;
    app.remotes.lock().await.invalidate(&id);
    // The credential goes first. If the entry cannot be removed afterwards it
    // is still listed, and removing it again works.
    match saved.auth {
        AuthKind::ApiKey => {
            let (secrets, key_id) = (app.secrets.clone(), id.clone());
            blocking(move || secrets.delete(&key_id)).await?;
        }
        AuthKind::Oauth => {
            // Another saved connection may use the same sign-in.
            if !app.sign_in_is_used(&saved.url, Some(&id)).await? {
                app.logins.logout(&saved.url).await.map_err(internal)?;
            }
        }
    }
    let remove_id = id.clone();
    let active = app
        .store(move |store| {
            store
                .remove(&remove_id)
                .map(|_| store.active_id().map(str::to_owned))
        })
        .await?
        .map_err(internal)?;
    app.remotes.lock().await.invalidate(&id);
    ok(json!({"removed": id, "activeId": active}))
}

async fn activate_connection(State(app): State<Arc<App>>, Path(id): Path<String>) -> Api {
    let target = id.clone();
    let found = app
        .store(move |store| match store.get(&target) {
            Some(_) => store.activate(&target).map(|()| true),
            None => Ok(false),
        })
        .await?
        .map_err(internal)?;
    if !found {
        return Err(unknown_connection());
    }
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
    /// Also ask for the admin scope (the management pages).
    #[serde(default)]
    admin: bool,
}

fn unknown_sign_in() -> ApiError {
    ApiError(
        StatusCode::NOT_FOUND,
        "unknown_sign_in",
        "This sign-in is no longer active. Start again.".into(),
    )
}

async fn start_login(State(app): State<Arc<App>>, body: axum::body::Bytes) -> Api {
    let input: LoginInput = parse(&body)?;
    let url = project_url(&input.url).map_err(bad_request)?;
    let client_id = input.client_id.trim().to_owned();
    if client_id.is_empty() || client_id.len() > 512 || client_id.chars().any(char::is_control) {
        return Err(bad_request(
            "Enter the public client ID from the environment's connection details",
        ));
    }
    let name = connection_name(input.name, &url)?;
    let write = input.write || input.admin;
    let started = app
        .logins
        .start(url.to_string(), client_id.clone(), write, input.admin)
        .await
        .map_err(|e| ApiError(StatusCode::UNPROCESSABLE_ENTITY, "sign_in_failed", e))?;
    // Recorded before its watcher starts: the watcher stops as soon as it
    // does not find the attempt.
    app.pending.lock().await.insert(
        started.id.clone(),
        PendingLogin {
            name,
            url: url.to_string(),
            client_id,
            write,
        },
    );
    watch_sign_in(app.clone(), started.id.clone(), url.to_string());
    ok(json!({"id": started.id, "authorizationUrl": started.authorization_url}))
}

/// Reports where a sign-in stands. It changes nothing; `complete_login` saves.
async fn login_status(State(app): State<Arc<App>>, Path(id): Path<String>) -> Api {
    if !app.pending.lock().await.contains_key(&id) {
        return Err(unknown_sign_in());
    }
    match app.logins.status(&id).await {
        Some(LoginState::Pending) => ok(json!({"state": "pending"})),
        Some(LoginState::Connected) => ok(json!({"state": "ready"})),
        Some(LoginState::Failed(message)) => ok(json!({"state": "failed", "message": message})),
        None => ok(json!({"state": "failed", "message": "The sign-in expired. Start again."})),
    }
}

/// Saves the connection of a finished sign-in, once the server accepts its token.
async fn complete_login(State(app): State<Arc<App>>, Path(id): Path<String>) -> Api {
    let state = match app.pending.lock().await.get(&id) {
        Some(_) => app.logins.status(&id).await,
        None => return Err(unknown_sign_in()),
    };
    let failed = |message: String| ok(json!({"state": "failed", "message": message}));
    match state {
        Some(LoginState::Pending) => {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "sign_in_pending",
                "The sign-in is not finished yet".into(),
            ))
        }
        Some(LoginState::Failed(message)) => {
            app.pending.lock().await.remove(&id);
            return failed(message);
        }
        None => {
            app.pending.lock().await.remove(&id);
            return failed("The sign-in expired. Start again.".into());
        }
        Some(LoginState::Connected) => {}
    }
    // Taken out first, so two requests cannot save the connection twice.
    let Some(login) = app.pending.lock().await.remove(&id) else {
        return Err(unknown_sign_in());
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
    let (remote, info) = match probe(url, app.logins.tokens(&connection.url)).await {
        Ok(v) => v,
        Err(e) => {
            app.drop_unused_sign_in(&connection.url).await;
            return failed(e.2);
        }
    };
    let to_save = connection.clone();
    if let Err(e) = app
        .store(move |store| store.upsert(to_save, true))
        .await
        .and_then(|r| r.map_err(internal))
    {
        app.drop_unused_sign_in(&connection.url).await;
        return Err(e);
    }
    app.remotes.lock().await.clients.insert(connection.id.clone(), remote);
    ok(json!({
        "state": "connected",
        "connection": connection_json(&connection, Some(&connection.id)),
        "info": info_json(&info),
    }))
}

async fn cancel_login(State(app): State<Arc<App>>, Path(id): Path<String>) -> Api {
    let Some(login) = app.pending.lock().await.remove(&id) else {
        return Err(unknown_sign_in());
    };
    if app.logins.cancel(&id).await.is_err() {
        // Too late to stop: the browser already answered and the tokens are
        // being stored. Remove them when that has happened.
        abandon(app.clone(), id, login.url);
    } else {
        app.drop_leftover_sign_in(&login.url).await;
    }
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
    /// The connection the page shows. A call for another one is refused.
    connection: String,
    #[serde(default)]
    arguments: Option<Value>,
}

async fn call_tool(
    State(app): State<Arc<App>>,
    Path(name): Path<String>,
    body: axum::body::Bytes,
) -> Api {
    let input: CallInput = parse(&body)?;
    let arguments = input.arguments.unwrap_or_else(|| json!({}));
    if !arguments.is_object() {
        return Err(bad_request("Tool arguments must be an object"));
    }
    let (connection, remote) = app.active_remote().await?;
    // Another window may have switched environments since this page loaded.
    if connection.id != input.connection {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "connection_changed",
            format!("{} is the active environment now. Reload to continue.", connection.name),
        ));
    }
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
