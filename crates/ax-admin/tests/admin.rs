//! ax-admin against a real Ax remote MCP server (docs/specs/ax-admin/SPEC.md).
//! Scripted servers are used only for behaviour a healthy Ax cannot be made to
//! show on demand: older protocol versions, expired sessions, redirects.
use ax_admin::{
    login::{BoxFuture, LoginBackend, LoginStarted, LoginState},
    mcp::{requires_admin, Limits, McpError, Remote, StaticToken, TokenFuture, TokenSource},
    secrets::{MemoryStore, SecretStore},
    server::{bind, router, App, CSRF_HEADER},
};
use ax_mcp::remote::{
    auth::TokenVerifier,
    config::{ApiKey, OAuthConfig, Project, RemoteConfig},
    server::{router as remote_router, RemoteState},
};
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    Router,
};
use jsonwebtoken::jwk::JwkSet;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, Once,
    },
    time::Duration,
};
use tower::ServiceExt;

const CONTEXT_KEY: &str = "alpha-context-secret-key";
const ADMIN_KEY: &str = "alpha-admin-secret-key";
const BETA_KEY: &str = "beta-context-secret-key";
const PORT: u16 = 7080;
/// How often ax-admin asks an open sign-in where it stands, in these tests.
const SIGN_IN_POLL: Duration = Duration::from_millis(20);

/// Points the usage database of the real server at a temporary file. Every
/// test calls this before anything else: `call_once` holds back every other
/// test thread until the variables are set, so no thread reads the
/// environment while it changes.
fn isolate_usage_db() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir = tempfile::tempdir().unwrap().keep();
        std::env::set_var("AX_USAGE_DB", dir.join("usage.db"));
        std::env::remove_var("AX_MCP_TOOLS");
    });
}

fn key(id: &str, secret: &str, project: &str, admin: bool) -> ApiKey {
    ApiKey {
        id: id.into(),
        sha256: hex::encode(Sha256::digest(secret.as_bytes())),
        projects: vec![project.into()],
        write: admin,
        admin,
    }
}

/// A real Ax remote MCP server with two indexed projects, on a loopback port.
struct Upstream {
    dir: tempfile::TempDir,
    base: String,
}

impl Upstream {
    async fn start() -> Self {
        isolate_usage_db();
        let dir = tempfile::tempdir().unwrap();
        let mut projects = Vec::new();
        for id in ["alpha", "beta"] {
            let root = dir.path().join(id);
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(
                root.join("lib.rs"),
                "pub fn remote_fixture() -> u32 { 1 }\n",
            )
            .unwrap();
            let mut ax = ax_core::Ax::init(&root).await.unwrap();
            ax.index_all(
                ax_extraction::orchestrator::IndexOptions {
                    quiet: true,
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
            drop(ax);
            projects.push(Project {
                id: id.into(),
                root,
                subjects: vec![id.into()],
                write_subjects: vec![id.into()],
                admin_subjects: vec![],
            });
        }
        let mut config = RemoteConfig {
            public_url: "https://ax.example.com".into(),
            oauth: OAuthConfig {
                built_in: false,
                issuer: "https://issuer.example.com/".into(),
                jwks_uri: "https://issuer.example.com/jwks".into(),
                clock_skew_seconds: ax_mcp::remote::config::DEFAULT_CLOCK_SKEW_SECONDS,
            },
            projects,
            api_keys: vec![
                key("ctx", CONTEXT_KEY, "alpha", false),
                key("adm", ADMIN_KEY, "alpha", true),
                key("beta", BETA_KEY, "beta", false),
            ],
            allowed_origins: vec![],
        };
        config.validate().unwrap();
        let config = Arc::new(config);
        let verifier = TokenVerifier::with_keys(
            config.clone(),
            JwkSet { keys: vec![] },
            reqwest::Client::new(),
        );
        let base = serve(remote_router(RemoteState::new(config, verifier))).await;
        Self { dir, base }
    }

    fn url(&self, project: &str) -> String {
        format!("{}/projects/{project}/mcp", self.base)
    }
}

async fn serve(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://127.0.0.1:{port}")
}

/// A sign-in backend driven by the test instead of a browser.
#[derive(Default)]
struct FakeLogins {
    /// Where each attempt stands, by id. `set` moves the newest one.
    states: Mutex<Vec<(String, LoginState)>>,
    token: Mutex<String>,
    /// (address, client id, write, admin) of every started sign-in.
    started: Mutex<Vec<(String, String, bool, bool)>>,
    /// The browser already answered: cancelling is refused.
    cancel_too_late: std::sync::atomic::AtomicBool,
    cancelled: AtomicUsize,
    logged_out: Mutex<Vec<String>>,
}

impl FakeLogins {
    /// The browser's answer for the attempt that was started last.
    fn set(&self, state: LoginState) {
        self.states
            .lock()
            .unwrap()
            .last_mut()
            .expect("a started sign-in")
            .1 = state;
    }
    /// The browser's answer for one attempt.
    fn set_for(&self, id: &str, state: LoginState) {
        let mut states = self.states.lock().unwrap();
        states
            .iter_mut()
            .find(|(attempt, _)| attempt == id)
            .expect("a started sign-in")
            .1 = state;
    }
    fn logged_out(&self) -> Vec<String> {
        self.logged_out.lock().unwrap().clone()
    }
}

impl LoginBackend for FakeLogins {
    fn start(
        &self,
        url: String,
        client_id: String,
        write: bool,
        admin: bool,
    ) -> BoxFuture<'_, Result<LoginStarted, String>> {
        Box::pin(async move {
            let mut started = self.started.lock().unwrap();
            started.push((url.clone(), client_id, write, admin));
            let id = format!("attempt-{}", started.len());
            self.states
                .lock()
                .unwrap()
                .push((id.clone(), LoginState::Pending));
            Ok(LoginStarted {
                id,
                authorization_url: format!("https://issuer.example.com/authorize?resource={url}"),
            })
        })
    }
    fn status<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Option<LoginState>> {
        Box::pin(async move {
            let states = self.states.lock().unwrap();
            states
                .iter()
                .find(|(attempt, _)| attempt == id)
                .map(|(_, state)| state.clone())
        })
    }
    fn cancel<'a>(&'a self, _id: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            if self.cancel_too_late.load(Ordering::SeqCst) {
                return Err("Sign-in already completed or is verifying access".into());
            }
            self.cancelled.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
    fn logout<'a>(&'a self, url: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            self.logged_out.lock().unwrap().push(url.to_owned());
            Ok(())
        })
    }
    fn tokens(&self, _url: &str) -> Arc<dyn TokenSource> {
        Arc::new(StaticToken(self.token.lock().unwrap().clone()))
    }
}

/// One ax-admin instance. `restart` builds a second instance on the same
/// configuration directory and credential store, like a process restart.
struct Admin {
    home: PathBuf,
    _home_guard: Option<tempfile::TempDir>,
    secrets: Arc<MemoryStore>,
    logins: Arc<FakeLogins>,
    router: Router,
    csrf: String,
    /// Every response body, for the "no secret leaves the process" check.
    seen: Arc<Mutex<String>>,
}

impl Admin {
    async fn start() -> Self {
        let guard = tempfile::tempdir().unwrap();
        let home = guard.path().join("ax-admin-home");
        Self::build(
            home,
            Some(guard),
            Arc::new(MemoryStore::default()),
            Arc::new(FakeLogins::default()),
        )
        .await
    }

    async fn build(
        home: PathBuf,
        guard: Option<tempfile::TempDir>,
        secrets: Arc<MemoryStore>,
        logins: Arc<FakeLogins>,
    ) -> Self {
        Self::build_with_grace(home, guard, secrets, logins, Duration::from_secs(60)).await
    }

    /// `grace`: how long a finished sign-in may wait to be saved.
    async fn build_with_grace(
        home: PathBuf,
        guard: Option<tempfile::TempDir>,
        secrets: Arc<MemoryStore>,
        logins: Arc<FakeLogins>,
        grace: Duration,
    ) -> Self {
        // A short poll, so the fixed waits below cover many polls of every
        // open sign-in even on a loaded machine.
        let app = App::with_sign_in_timing(
            home.clone(),
            secrets.clone(),
            logins.clone(),
            PORT,
            grace,
            SIGN_IN_POLL,
        )
        .unwrap();
        let mut admin = Self {
            home,
            _home_guard: guard,
            secrets,
            logins,
            router: router(app),
            csrf: String::new(),
            seen: Arc::new(Mutex::new(String::new())),
        };
        let (status, body) = admin.get("/api/connections").await;
        assert_eq!(status, StatusCode::OK);
        admin.csrf = body["csrf"].as_str().unwrap().to_owned();
        admin
    }

    async fn restart(self) -> Self {
        let Self {
            home,
            _home_guard,
            secrets,
            logins,
            ..
        } = self;
        Self::build(home, _home_guard, secrets, logins).await
    }

    async fn send(&self, request: Request<Body>) -> (StatusCode, HeaderMap, Value) {
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = to_bytes(response.into_body(), 16 * 1024 * 1024)
            .await
            .unwrap();
        self.seen
            .lock()
            .unwrap()
            .push_str(&String::from_utf8_lossy(&bytes));
        (
            status,
            headers,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    fn request(&self, method: &str, path: &str) -> axum::http::request::Builder {
        Request::builder()
            .method(method)
            .uri(path)
            .header("host", format!("127.0.0.1:{PORT}"))
    }

    async fn get(&self, path: &str) -> (StatusCode, Value) {
        let (status, _, body) = self
            .send(self.request("GET", path).body(Body::empty()).unwrap())
            .await;
        (status, body)
    }

    async fn call(&self, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
        let (status, _, body) = self
            .send(
                self.request(method, path)
                    .header("content-type", "application/json")
                    .header(CSRF_HEADER, &self.csrf)
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await;
        (status, body)
    }

    async fn add(&self, name: &str, url: &str, secret: &str) -> String {
        let (status, body) = self
            .call(
                "POST",
                "/api/connections",
                json!({"name": name, "url": url, "auth": {"type": "api_key", "key": secret}}),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        body["connection"]["id"].as_str().unwrap().to_owned()
    }

    /// Calls a tool the way the page does: for the connection it shows.
    async fn tool(&self, name: &str, arguments: Value) -> (StatusCode, Value) {
        let (_, list) = self.get("/api/connections").await;
        let connection = list["activeId"].as_str().unwrap_or("none").to_owned();
        self.call(
            "POST",
            &format!("/api/remote/tools/{name}"),
            json!({"connection": connection, "arguments": arguments}),
        )
        .await
    }

    async fn tool_names(&self) -> Vec<String> {
        let (status, body) = self.get("/api/remote/info").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        body["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect()
    }

    fn config_file(&self) -> PathBuf {
        self.home.join("connections.json")
    }
}

// ---------------------------------------------------------------- connections

#[tokio::test]
async fn a1_without_a_connection_the_remote_api_is_closed() {
    isolate_usage_db();
    let admin = Admin::start().await;
    let (_, list) = admin.get("/api/connections").await;
    assert_eq!(list["connections"], json!([]));
    assert_eq!(list["activeId"], Value::Null);
    let (status, body) = admin.get("/api/remote/info").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "no_active_connection");
    let (status, body) = admin.tool("ax_status", json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "no_active_connection");
    // A call must say which connection it is for.
    let (status, _) = admin
        .call(
            "POST",
            "/api/remote/tools/ax_status",
            json!({"arguments": {}}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a2_a_connection_needs_an_address_and_credentials() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let admin = Admin::start().await;
    let url = upstream.url("alpha");
    let cases = [
        json!({"url": "", "auth": {"type": "api_key", "key": CONTEXT_KEY}}),
        json!({"auth": {"type": "api_key", "key": CONTEXT_KEY}}),
        json!({"url": url}),
        json!({"url": url, "auth": {"type": "api_key", "key": ""}}),
        json!({"url": url, "auth": {"type": "api_key", "key": "   "}}),
        json!({"url": url, "auth": {"type": "api_key", "key": "two words"}}),
        json!({"url": "http://ax.example.com/projects/alpha/mcp", "auth": {"type": "api_key", "key": CONTEXT_KEY}}),
        json!({"url": format!("{}/projects/alpha", upstream.base), "auth": {"type": "api_key", "key": CONTEXT_KEY}}),
        json!({"url": "https://user:pw@ax.example.com/projects/alpha/mcp", "auth": {"type": "api_key", "key": CONTEXT_KEY}}),
        json!({"url": url, "auth": {"type": "api_key", "key": CONTEXT_KEY}, "name": "x".repeat(81)}),
    ];
    for case in cases {
        for path in ["/api/connections", "/api/connections/test"] {
            let (status, body) = admin.call("POST", path, case.clone()).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{path} {case} -> {body}");
            assert_eq!(body["code"], "invalid_request");
        }
    }
    assert!(!admin.config_file().exists());
    assert!(admin.secrets.is_empty());
}

#[tokio::test]
async fn a3_a_valid_address_and_key_are_saved_activated_and_usable() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let admin = Admin::start().await;
    let (status, tested) = admin
        .call(
            "POST",
            "/api/connections/test",
            json!({"url": upstream.url("alpha"), "auth": {"type": "api_key", "key": CONTEXT_KEY}}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{tested}");
    assert_eq!(tested["info"]["protocolVersion"], "2025-11-25");
    assert!(!admin.config_file().exists(), "a test saves nothing");

    let (status, created) = admin
        .call(
            "POST",
            "/api/connections",
            json!({"url": upstream.url("alpha"), "auth": {"type": "api_key", "key": CONTEXT_KEY}}),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["connection"]["id"].as_str().unwrap();
    assert_eq!(created["connection"]["name"], "127.0.0.1 / alpha");
    assert_eq!(created["connection"]["active"], true);
    let (_, list) = admin.get("/api/connections").await;
    assert_eq!(list["activeId"], id);
    assert!(admin.config_file().exists());
    assert_eq!(admin.secrets.get(id).unwrap().as_deref(), Some(CONTEXT_KEY));

    let (status, info) = admin.get("/api/remote/info").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(info["connection"]["id"], id);
    let names = admin.tool_names().await;
    assert!(names.contains(&"ax_status".to_string()));

    let (status, result) = admin.tool("ax_status", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["isError"], false);
    assert!(
        result["text"].as_str().unwrap().contains("files"),
        "{result}"
    );
    let (_, found) = admin
        .tool("ax_search", json!({"query": "remote_fixture"}))
        .await;
    assert!(
        found["text"].as_str().unwrap().contains("remote_fixture"),
        "{found}"
    );
}

#[tokio::test]
async fn a4_a_wrong_key_is_reported_and_not_saved() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let admin = Admin::start().await;
    for path in ["/api/connections", "/api/connections/test"] {
        let (status, body) = admin
            .call(
                "POST",
                path,
                json!({"url": upstream.url("alpha"), "auth": {"type": "api_key", "key": "not-the-key"}}),
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["code"], "credentials_rejected");
        assert_eq!(body["error"], "The server rejected these credentials.");
    }
    // A valid key for another project is refused too.
    let (status, body) = admin
        .call(
            "POST",
            "/api/connections",
            json!({"url": upstream.url("beta"), "auth": {"type": "api_key", "key": CONTEXT_KEY}}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "forbidden");
    let (status, body) = admin
        .call(
            "POST",
            "/api/connections",
            json!({"url": format!("{}/projects/gamma/mcp", upstream.base), "auth": {"type": "api_key", "key": CONTEXT_KEY}}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "unknown_project");
    assert!(!admin.config_file().exists());
    assert!(admin.secrets.is_empty());
}

#[derive(Default)]
struct Hits {
    count: AtomicUsize,
    authorization: Mutex<Vec<String>>,
}

async fn record(State(hits): State<Arc<Hits>>, headers: HeaderMap) {
    hits.count.fetch_add(1, Ordering::SeqCst);
    if let Some(value) = headers.get("authorization").and_then(|v| v.to_str().ok()) {
        hits.authorization.lock().unwrap().push(value.to_owned());
    }
}

#[tokio::test]
async fn a5_unreachable_non_mcp_and_redirecting_addresses_are_distinct_errors() {
    isolate_usage_db();
    let admin = Admin::start().await;
    let attempt = |url: String| {
        let admin = &admin;
        async move {
            let (status, body) = admin
                .call(
                    "POST",
                    "/api/connections",
                    json!({"url": url, "auth": {"type": "api_key", "key": CONTEXT_KEY}}),
                )
                .await;
            // The environment could not be reached or something else answered.
            assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
            body["code"].as_str().unwrap().to_owned()
        }
    };

    // Nothing listens on this port.
    let closed = {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        listener.local_addr().unwrap().port()
    };
    assert_eq!(
        attempt(format!("http://127.0.0.1:{closed}/projects/alpha/mcp")).await,
        "unreachable"
    );

    let html = serve(Router::new().fallback(|| async {
        (
            [("content-type", "text/html")],
            "<html><body>Sign in</body></html>",
        )
    }))
    .await;
    assert_eq!(
        attempt(format!("{html}/projects/alpha/mcp")).await,
        "not_mcp"
    );

    let gateway =
        serve(Router::new().fallback(|| async { (StatusCode::BAD_GATEWAY, "Bad gateway") })).await;
    assert_eq!(
        attempt(format!("{gateway}/projects/alpha/mcp")).await,
        "not_mcp"
    );

    // A redirect is reported, never followed, and the key is not sent on.
    let target_hits = Arc::new(Hits::default());
    let target = serve(
        Router::new()
            .fallback(|state: State<Arc<Hits>>, headers: HeaderMap| async move {
                record(state, headers).await;
                StatusCode::OK
            })
            .with_state(target_hits.clone()),
    )
    .await;
    let location = format!("{target}/projects/alpha/mcp");
    let redirecting = serve(Router::new().fallback(move || {
        let location = location.clone();
        async move { (StatusCode::TEMPORARY_REDIRECT, [("location", location)]) }
    }))
    .await;
    assert_eq!(
        attempt(format!("{redirecting}/projects/alpha/mcp")).await,
        "redirect"
    );
    assert_eq!(target_hits.count.load(Ordering::SeqCst), 0);
    assert!(target_hits.authorization.lock().unwrap().is_empty());

    assert!(!admin.config_file().exists());
    assert!(admin.secrets.is_empty());
}

#[tokio::test]
async fn a6_switching_changes_which_server_answers() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let admin = Admin::start().await;
    let alpha = admin
        .add("Alpha admin", &upstream.url("alpha"), ADMIN_KEY)
        .await;
    let beta = admin.add("Beta", &upstream.url("beta"), BETA_KEY).await;

    // The connection added last is active.
    let (_, list) = admin.get("/api/connections").await;
    assert_eq!(list["activeId"], beta.as_str());
    assert_eq!(list["connections"].as_array().unwrap().len(), 2);
    let beta_tools = admin.tool_names().await;
    assert!(!beta_tools.contains(&"ax_sync".to_string()));
    assert!(!beta_tools.contains(&"ax_remember".to_string()));

    let (status, _) = admin
        .call(
            "POST",
            &format!("/api/connections/{alpha}/activate"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, info) = admin.get("/api/remote/info").await;
    assert_eq!(info["connection"]["id"], alpha.as_str());
    let alpha_tools = admin.tool_names().await;
    for tool in ["ax_sync", "ax_index", "ax_logs", "ax_files", "ax_remember"] {
        assert!(alpha_tools.contains(&tool.to_string()), "{tool}");
    }

    // An admin call reaches the project of the active connection.
    std::fs::write(
        upstream.dir.path().join("alpha/added.rs"),
        "pub fn added_through_admin() -> u32 { 4 }\n",
    )
    .unwrap();
    let (status, synced) = admin.tool("ax_sync", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{synced}");
    assert_eq!(synced["isError"], false, "{synced}");
    let search = json!({"query": "added_through_admin"});
    let (_, found) = admin.tool("ax_search", search.clone()).await;
    assert!(
        found["text"].as_str().unwrap().contains("added.rs"),
        "{found}"
    );

    // Back on beta: no admin tools, and alpha's symbol is not there.
    admin
        .call(
            "POST",
            &format!("/api/connections/{beta}/activate"),
            json!({}),
        )
        .await;
    let (status, denied) = admin.tool("ax_sync", json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(denied["code"], "unknown_tool");
    let (_, found) = admin.tool("ax_search", search).await;
    assert!(
        !found["text"].as_str().unwrap().contains("added.rs"),
        "{found}"
    );

    let (status, body) = admin
        .call(
            "POST",
            "/api/connections/does-not-exist/activate",
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "unknown_connection");
}

#[tokio::test]
async fn a7_removing_deletes_the_entry_and_its_key() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let admin = Admin::start().await;
    let alpha = admin
        .add("Alpha", &upstream.url("alpha"), CONTEXT_KEY)
        .await;
    let beta = admin.add("Beta", &upstream.url("beta"), BETA_KEY).await;
    assert_eq!(admin.secrets.len(), 2);

    // Removing the active connection activates the remaining one.
    let (status, body) = admin
        .call("DELETE", &format!("/api/connections/{beta}"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["activeId"], alpha.as_str());
    assert_eq!(admin.secrets.get(&beta).unwrap(), None);
    let file = std::fs::read_to_string(admin.config_file()).unwrap();
    assert!(!file.contains(&beta) && file.contains(&alpha));
    let (status, info) = admin.get("/api/remote/info").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(info["connection"]["id"], alpha.as_str());

    // Removing the last one returns to the unconnected state.
    let (_, body) = admin
        .call("DELETE", &format!("/api/connections/{alpha}"), json!({}))
        .await;
    assert_eq!(body["activeId"], Value::Null);
    assert!(admin.secrets.is_empty());
    let (status, body) = admin.get("/api/remote/info").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "no_active_connection");
    let (status, _) = admin
        .call("DELETE", &format!("/api/connections/{alpha}"), json!({}))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a8_a_restart_restores_connections_and_the_active_one() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let admin = Admin::start().await;
    let alpha = admin
        .add("Alpha", &upstream.url("alpha"), CONTEXT_KEY)
        .await;
    admin.add("Beta", &upstream.url("beta"), BETA_KEY).await;
    admin
        .call(
            "POST",
            &format!("/api/connections/{alpha}/activate"),
            json!({}),
        )
        .await;

    let admin = admin.restart().await;
    let (_, list) = admin.get("/api/connections").await;
    assert_eq!(list["connections"].as_array().unwrap().len(), 2);
    assert_eq!(list["activeId"], alpha.as_str());
    let (status, info) = admin.get("/api/remote/info").await;
    assert_eq!(status, StatusCode::OK, "{info}");
    assert_eq!(info["connection"]["name"], "Alpha");
}

#[tokio::test]
async fn a8_editing_keeps_the_key_unless_a_new_one_is_given() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let admin = Admin::start().await;
    let id = admin
        .add("Alpha", &upstream.url("alpha"), CONTEXT_KEY)
        .await;
    let path = format!("/api/connections/{id}");

    let (status, body) = admin
        .call("PUT", &path, json!({"name": "Production"}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["connection"]["name"], "Production");
    assert_eq!(
        admin.secrets.get(&id).unwrap().as_deref(),
        Some(CONTEXT_KEY)
    );

    // A new key is checked before it replaces the stored one.
    let (status, body) = admin
        .call(
            "PUT",
            &path,
            json!({"auth": {"type": "api_key", "key": "wrong-key"}}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "credentials_rejected");
    assert_eq!(
        admin.secrets.get(&id).unwrap().as_deref(),
        Some(CONTEXT_KEY)
    );

    let (status, _) = admin
        .call(
            "PUT",
            &path,
            json!({"auth": {"type": "api_key", "key": ADMIN_KEY}}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(admin.secrets.get(&id).unwrap().as_deref(), Some(ADMIN_KEY));
    assert!(admin.tool_names().await.contains(&"ax_index".to_string()));

    // Another address with a key the server there refuses: nothing changes.
    let (status, body) = admin
        .call(
            "PUT",
            &path,
            json!({"url": upstream.url("beta"), "auth": {"type": "api_key", "key": ADMIN_KEY}}),
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "forbidden");
    let (_, list) = admin.get("/api/connections").await;
    assert_eq!(list["connections"][0]["url"], upstream.url("alpha"));
    // Another address with the right key for it.
    let (status, body) = admin
        .call(
            "PUT",
            &path,
            json!({"url": upstream.url("beta"), "auth": {"type": "api_key", "key": BETA_KEY}}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(admin.secrets.get(&id).unwrap().as_deref(), Some(BETA_KEY));
    assert!(!admin.tool_names().await.contains(&"ax_index".to_string()));

    // A saved connection can be tested without retyping its key.
    let (status, body) = admin
        .call("POST", "/api/connections/test", json!({"id": id}))
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

/// An edit that checks a new key takes a moment. A removal of the same
/// connection in that moment wins: the edit does not bring the entry back, and
/// no key is left in the credential store for it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_edit_does_not_bring_back_a_connection_removed_meanwhile() {
    isolate_usage_db();
    use std::sync::atomic::AtomicBool;
    #[derive(Default)]
    struct Gate {
        closed: AtomicBool,
        waiting: AtomicBool,
    }
    let gate = Arc::new(Gate::default());
    let app = Router::new()
        .fallback(|State(gate): State<Arc<Gate>>, body: String| async move {
            let request: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            match request["method"].as_str().unwrap_or("") {
                "initialize" => {
                    while gate.closed.load(Ordering::SeqCst) {
                        gate.waiting.store(true, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                    initialized("2025-11-25", "s")
                }
                "notifications/initialized" => StatusCode::ACCEPTED.into_response(),
                _ => tool_list(&["ax_status"]),
            }
        })
        .with_state(gate.clone());
    let url = format!("{}/projects/alpha/mcp", serve(app).await);
    let admin = Admin::start().await;
    let id = admin.add("Alpha", &url, "first-key").await;
    let path = format!("/api/connections/{id}");

    gate.closed.store(true, Ordering::SeqCst);
    let edit = admin.call(
        "PUT",
        &path,
        json!({"name": "Renamed", "auth": {"type": "api_key", "key": "second-key"}}),
    );
    let remove = async {
        while !gate.waiting.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let removed = admin.call("DELETE", &path, json!({})).await;
        gate.closed.store(false, Ordering::SeqCst);
        removed
    };
    let ((edit_status, edit_body), (remove_status, remove_body)) = tokio::join!(edit, remove);
    assert_eq!(remove_status, StatusCode::OK, "{remove_body}");
    assert_eq!(edit_status, StatusCode::NOT_FOUND, "{edit_body}");
    assert_eq!(edit_body["code"], "unknown_connection");
    let (_, list) = admin.get("/api/connections").await;
    assert_eq!(list["connections"], json!([]), "{list}");
    assert_eq!(admin.secrets.get(&id).unwrap(), None);
}

#[tokio::test]
async fn a_saved_key_is_never_sent_to_another_address() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let admin = Admin::start().await;
    let id = admin.add("Alpha", &upstream.url("alpha"), ADMIN_KEY).await;
    // Somebody else's server, ready to record whatever it is sent.
    let hits = Arc::new(Hits::default());
    let other = serve(
        Router::new()
            .fallback(|state: State<Arc<Hits>>, headers: HeaderMap| async move {
                record(state, headers).await;
                StatusCode::UNAUTHORIZED
            })
            .with_state(hits.clone()),
    )
    .await;
    let elsewhere = format!("{other}/projects/alpha/mcp");

    for body in [
        json!({"id": id, "url": elsewhere}),
        json!({"id": id, "name": "x"}),
        json!({"id": id, "auth": {"type": "api_key", "key": "another"}}),
    ] {
        let (status, response) = admin.call("POST", "/api/connections/test", body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    }
    let (status, response) = admin
        .call(
            "PUT",
            &format!("/api/connections/{id}"),
            json!({"url": elsewhere}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        response["error"],
        "Enter the key again to use another address"
    );

    assert_eq!(
        hits.count.load(Ordering::SeqCst),
        0,
        "nothing was sent there"
    );
    assert!(hits.authorization.lock().unwrap().is_empty());
    let (_, list) = admin.get("/api/connections").await;
    assert_eq!(list["connections"][0]["url"], upstream.url("alpha"));
    assert_eq!(admin.secrets.get(&id).unwrap().as_deref(), Some(ADMIN_KEY));
}

#[tokio::test]
async fn a_failed_save_leaves_keys_and_connections_as_they_were() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let admin = Admin::start().await;
    let id = admin
        .add("Alpha", &upstream.url("alpha"), CONTEXT_KEY)
        .await;
    // A directory where the lock file belongs: connections.json can no longer
    // be changed, for root too (file permissions do not stop root).
    let lock_path = admin.home.join("connections.json.lock");
    let lock = |broken: bool| {
        let _ = std::fs::remove_file(&lock_path);
        if broken {
            std::fs::create_dir(&lock_path).unwrap()
        } else {
            std::fs::remove_dir(&lock_path).unwrap()
        }
    };

    lock(true);
    // Editing: the new key is accepted by the server but the entry cannot be saved.
    let (status, body) = admin
        .call(
            "PUT",
            &format!("/api/connections/{id}"),
            json!({"name": "Renamed", "auth": {"type": "api_key", "key": ADMIN_KEY}}),
        )
        .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_eq!(
        admin.secrets.get(&id).unwrap().as_deref(),
        Some(CONTEXT_KEY),
        "old key restored"
    );
    // Adding: no key is left without an entry.
    let (status, _) = admin
        .call(
            "POST",
            "/api/connections",
            json!({"url": upstream.url("beta"), "auth": {"type": "api_key", "key": BETA_KEY}}),
        )
        .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(admin.secrets.len(), 1);
    // Switching and removing fail without changing what is shown.
    let (status, _) = admin
        .call("DELETE", &format!("/api/connections/{id}"), json!({}))
        .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let (_, list) = admin.get("/api/connections").await;
    assert_eq!(list["connections"].as_array().unwrap().len(), 1);
    assert_eq!(list["connections"][0]["name"], "Alpha");
    assert_eq!(list["activeId"], id.as_str());

    lock(false);
    // The failed removal already deleted the key; removing again completes it.
    let (status, _) = admin
        .call("DELETE", &format!("/api/connections/{id}"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(admin.secrets.is_empty());
    assert_eq!(
        admin.restart().await.get("/api/connections").await.1["connections"],
        json!([])
    );
}

#[tokio::test]
async fn a_call_for_a_connection_that_is_no_longer_active_is_refused() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let admin = Admin::start().await;
    let alpha = admin
        .add("Alpha admin", &upstream.url("alpha"), ADMIN_KEY)
        .await;
    let beta = admin.add("Beta", &upstream.url("beta"), BETA_KEY).await;
    // This page still shows alpha, but another window made beta active.
    for name in ["ax_status", "ax_index"] {
        let (status, body) = admin
            .call(
                "POST",
                &format!("/api/remote/tools/{name}"),
                json!({"connection": alpha, "arguments": {}}),
            )
            .await;
        assert_eq!(status, StatusCode::CONFLICT, "{name}");
        assert_eq!(body["code"], "connection_changed");
        assert_eq!(
            body["error"],
            "Beta is the active environment now. Reload to continue."
        );
    }
    let (status, _) = admin
        .call(
            "POST",
            "/api/remote/tools/ax_status",
            json!({"connection": beta, "arguments": {}}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a9_no_key_reaches_the_configuration_file_or_any_response() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let admin = Admin::start().await;
    let id = admin.add("Alpha", &upstream.url("alpha"), ADMIN_KEY).await;
    admin.add("Beta", &upstream.url("beta"), BETA_KEY).await;
    admin.get("/api/connections").await;
    admin.get("/api/remote/info").await;
    admin
        .call("POST", "/api/connections/test", json!({"id": id}))
        .await;
    admin
        .call(
            "POST",
            "/api/connections/test",
            json!({"url": upstream.url("alpha"), "auth": {"type": "api_key", "key": CONTEXT_KEY}}),
        )
        .await;
    admin
        .call(
            "PUT",
            &format!("/api/connections/{id}"),
            json!({"name": "Renamed", "auth": {"type": "api_key", "key": ADMIN_KEY}}),
        )
        .await;
    admin
        .call(
            "POST",
            "/api/connections",
            json!({"url": upstream.url("beta"), "auth": {"type": "api_key", "key": "wrong-secret-key"}}),
        )
        .await;
    admin.tool("ax_status", json!({})).await;
    admin
        .call("DELETE", &format!("/api/connections/{id}"), json!({}))
        .await;
    // A browser sign-in: its token is a secret too.
    *admin.logins.token.lock().unwrap() = CONTEXT_KEY.to_owned();
    let (_, started) = admin
        .call(
            "POST",
            "/api/login",
            json!({"url": upstream.url("alpha"), "clientId": "native-client"}),
        )
        .await;
    admin.logins.set(LoginState::Connected);
    let complete = format!("/api/login/{}/complete", started["id"].as_str().unwrap());
    admin.get(&complete.replace("/complete", "")).await;
    let (_, done) = admin.call("POST", &complete, json!({})).await;
    assert_eq!(done["state"], "connected");
    admin.get("/api/remote/info").await;
    admin.tool("ax_status", json!({})).await;

    let file = std::fs::read_to_string(admin.config_file()).unwrap();
    let responses = admin.seen.lock().unwrap().clone();
    assert!(
        responses.len() > 1000,
        "the check must look at real traffic"
    );
    for secret in [ADMIN_KEY, BETA_KEY, CONTEXT_KEY, "wrong-secret-key"] {
        assert!(!file.contains(secret), "{secret} in connections.json");
        assert!(!responses.contains(secret), "{secret} in an API response");
        let hash = hex::encode(Sha256::digest(secret.as_bytes()));
        assert!(!file.contains(&hash) && !responses.contains(&hash));
    }
    // The file holds names and addresses only.
    let saved: Value = serde_json::from_str(&file).unwrap();
    for connection in saved["connections"].as_array().unwrap() {
        let mut fields: Vec<_> = connection.as_object().unwrap().keys().cloned().collect();
        fields.sort();
        let expected: &[&str] = if connection["auth"] == "oauth" {
            &["auth", "clientId", "id", "name", "url", "write"]
        } else {
            &["auth", "id", "name", "url", "write"]
        };
        assert_eq!(fields, expected);
    }
}

#[tokio::test]
async fn a10_browser_sign_in_saves_only_after_it_succeeds() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let admin = Admin::start().await;
    *admin.logins.token.lock().unwrap() = CONTEXT_KEY.to_owned();
    let url = upstream.url("alpha");

    let (status, _) = admin
        .call("POST", "/api/login", json!({"url": url, "clientId": " "}))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = admin
        .call(
            "POST",
            "/api/login",
            json!({"url": "https://ax.example.com/", "clientId": "native"}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(admin.logins.started.lock().unwrap().is_empty());

    // Cancelled before the browser answered: nothing is saved or left behind.
    let start = json!({"name": "Hosted", "url": url, "clientId": "native-client", "write": true});
    let (status, started) = admin.call("POST", "/api/login", start.clone()).await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert_eq!(started["id"], "attempt-1");
    assert!(started["authorizationUrl"]
        .as_str()
        .unwrap()
        .starts_with("https://issuer.example.com/authorize"));
    assert_eq!(
        admin.logins.started.lock().unwrap()[0],
        (url.clone(), "native-client".to_string(), true, false)
    );
    let (_, pending) = admin.get("/api/login/attempt-1").await;
    assert_eq!(pending["state"], "pending");
    let (status, body) = admin
        .call("POST", "/api/login/attempt-1/complete", json!({}))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "sign_in_pending");
    let (status, _) = admin
        .call("DELETE", "/api/login/attempt-1", json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(admin.logins.cancelled.load(Ordering::SeqCst), 1);
    let (status, _) = admin.get("/api/login/attempt-1").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = admin
        .call("POST", "/api/login/attempt-1/complete", json!({}))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!admin.config_file().exists());

    // Failed in the browser: nothing is saved.
    admin.call("POST", "/api/login", start.clone()).await;
    admin
        .logins
        .set(LoginState::Failed("Access was denied".into()));
    let (_, failed) = admin.get("/api/login/attempt-2").await;
    assert_eq!(
        (failed["state"].as_str(), failed["message"].as_str()),
        (Some("failed"), Some("Access was denied"))
    );
    let (_, failed) = admin
        .call("POST", "/api/login/attempt-2/complete", json!({}))
        .await;
    assert_eq!(failed["state"], "failed");
    assert!(!admin.config_file().exists());

    // Neither the cancelled nor the denied attempt stored anything, so neither
    // may sign anything out: `ax mcp remote login` may be using this address.
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(
        admin.logins.logged_out().is_empty(),
        "{:?}",
        admin.logins.logged_out()
    );

    // Signed in, but the server refuses the token: nothing saved, tokens dropped.
    *admin.logins.token.lock().unwrap() = "a-token-the-server-refuses".to_owned();
    admin.call("POST", "/api/login", start.clone()).await;
    admin.logins.set(LoginState::Connected);
    // Asking where it stands changes nothing, however often.
    for _ in 0..3 {
        let (_, ready) = admin.get("/api/login/attempt-3").await;
        assert_eq!(ready["state"], "ready");
    }
    assert!(!admin.config_file().exists());
    let (_, refused) = admin
        .call("POST", "/api/login/attempt-3/complete", json!({}))
        .await;
    assert_eq!(refused["state"], "failed", "{refused}");
    assert_eq!(refused["message"], "The server rejected these credentials.");
    assert!(!admin.config_file().exists());
    assert_eq!(admin.logins.logged_out(), std::slice::from_ref(&url));
    let (_, list) = admin.get("/api/connections").await;
    assert_eq!(list["connections"], json!([]));
}

#[tokio::test]
async fn a10_a_finished_sign_in_is_saved_once_and_removed_with_its_tokens() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let admin = Admin::start().await;
    *admin.logins.token.lock().unwrap() = ADMIN_KEY.to_owned();
    let url = upstream.url("alpha");

    let (_, started) = admin
        .call(
            "POST",
            "/api/login",
            json!({"name": "Hosted", "url": url, "clientId": "native-client", "admin": true}),
        )
        .await;
    // Asking for admin access asks for write access too.
    assert_eq!(
        admin.logins.started.lock().unwrap()[0],
        (url.clone(), "native-client".to_string(), true, true)
    );
    admin.logins.set(LoginState::Connected);
    let complete = format!("/api/login/{}/complete", started["id"].as_str().unwrap());
    // Completing needs the request token like every other change.
    let (status, _, _) = admin
        .send(
            admin
                .request("POST", &complete)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(!admin.config_file().exists());

    let (status, done) = admin.call("POST", &complete, json!({})).await;
    assert_eq!(status, StatusCode::OK, "{done}");
    assert_eq!(done["state"], "connected");
    let connection = &done["connection"];
    assert_eq!(connection["auth"], "oauth");
    assert_eq!(connection["clientId"], "native-client");
    assert_eq!(connection["write"], true);
    assert_eq!(connection["active"], true);
    let id = connection["id"].as_str().unwrap().to_owned();
    // A second completion finds nothing to save.
    let (status, _) = admin.call("POST", &complete, json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, list) = admin.get("/api/connections").await;
    assert_eq!(list["connections"].as_array().unwrap().len(), 1);
    assert!(admin.secrets.is_empty(), "a sign-in stores no API key");
    assert!(admin.tool_names().await.contains(&"ax_logs".to_string()));
    let (status, result) = admin.tool("ax_status", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{result}");

    // A sign-in belongs to one address and has no key to edit.
    let path = format!("/api/connections/{id}");
    for body in [
        json!({"url": upstream.url("beta")}),
        json!({"auth": {"type": "api_key", "key": BETA_KEY}}),
    ] {
        let (status, response) = admin.call("PUT", &path, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    }
    let (status, renamed) = admin
        .call("PUT", &path, json!({"name": "Production"}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(renamed["connection"]["name"], "Production");

    // After a restart the sign-in is still there, with what was asked for.
    let admin = admin.restart().await;
    let (_, list) = admin.get("/api/connections").await;
    assert_eq!(list["connections"][0]["clientId"], "native-client");
    assert_eq!(list["connections"][0]["name"], "Production");
    let file = std::fs::read_to_string(admin.config_file()).unwrap();
    assert!(!file.contains(ADMIN_KEY), "the token is not in the file");

    // A second connection on the same sign-in keeps the tokens alive.
    let (_, second) = admin
        .call(
            "POST",
            "/api/login",
            json!({"name": "Same sign-in", "url": url, "clientId": "native-client"}),
        )
        .await;
    admin.logins.set(LoginState::Connected);
    let second_complete = format!("/api/login/{}/complete", second["id"].as_str().unwrap());
    let (_, second) = admin.call("POST", &second_complete, json!({})).await;
    let second_id = second["connection"]["id"].as_str().unwrap().to_owned();
    admin.call("DELETE", &path, json!({})).await;
    assert!(
        admin.logins.logged_out().is_empty(),
        "still used by the other connection"
    );
    // Removing the last one signs out.
    let (status, _) = admin
        .call(
            "DELETE",
            &format!("/api/connections/{second_id}"),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(admin.logins.logged_out(), [url]);
}

#[tokio::test]
async fn a10_a_sign_in_the_page_never_saved_is_dropped_with_its_tokens() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let guard = tempfile::tempdir().unwrap();
    let admin = Admin::build_with_grace(
        guard.path().join("home"),
        None,
        Arc::new(MemoryStore::default()),
        Arc::new(FakeLogins::default()),
        Duration::from_millis(300),
    )
    .await;
    *admin.logins.token.lock().unwrap() = CONTEXT_KEY.to_owned();
    let url = upstream.url("alpha");
    // The page starts a sign-in and then goes away (closed, reloaded).
    let (_, started) = admin
        .call(
            "POST",
            "/api/login",
            json!({"url": url, "clientId": "native-client"}),
        )
        .await;
    let id = started["id"].as_str().unwrap().to_owned();
    // The user finishes in the browser anyway: tokens are stored.
    admin.logins.set(LoginState::Connected);
    for _ in 0..50 {
        if !admin.logins.logged_out().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(admin.logins.logged_out(), std::slice::from_ref(&url));
    let (status, _) = admin.get(&format!("/api/login/{id}")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "the attempt is gone");
    assert!(!admin.config_file().exists());

    // A finishes with no page (tokens stored); B for the same address is open
    // when A's grace period ends, so the tokens stay. B is then cancelled
    // before the browser answers: that is the moment they are removed.
    admin.logins.logged_out.lock().unwrap().clear();
    let (_, first) = admin
        .call(
            "POST",
            "/api/login",
            json!({"url": url, "clientId": "native-client"}),
        )
        .await;
    let (_, second) = admin
        .call(
            "POST",
            "/api/login",
            json!({"url": url, "clientId": "native-client"}),
        )
        .await;
    admin
        .logins
        .set_for(first["id"].as_str().unwrap(), LoginState::Connected);
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(
        admin.logins.logged_out().is_empty(),
        "kept while the second attempt is open"
    );
    let (status, _) = admin
        .call(
            "DELETE",
            &format!("/api/login/{}", second["id"].as_str().unwrap()),
            json!({}),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(admin.logins.logged_out(), std::slice::from_ref(&url));

    // Both attempts end unsaved: the first is kept for the second, the second
    // is then dropped with the tokens. After that the address is forgotten, so
    // a later cancelled attempt cannot sign out tokens that came from elsewhere
    // (`ax mcp remote login`) in the meantime.
    admin.logins.logged_out.lock().unwrap().clear();
    let (_, first) = admin
        .call(
            "POST",
            "/api/login",
            json!({"url": url, "clientId": "native-client"}),
        )
        .await;
    admin
        .call(
            "POST",
            "/api/login",
            json!({"url": url, "clientId": "native-client"}),
        )
        .await;
    admin
        .logins
        .set_for(first["id"].as_str().unwrap(), LoginState::Connected);
    tokio::time::sleep(Duration::from_millis(900)).await;
    admin.logins.set(LoginState::Connected);
    for _ in 0..40 {
        if !admin.logins.logged_out().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(admin.logins.logged_out(), std::slice::from_ref(&url));
    admin.logins.logged_out.lock().unwrap().clear();
    let (_, later) = admin
        .call(
            "POST",
            "/api/login",
            json!({"url": url, "clientId": "native-client"}),
        )
        .await;
    admin
        .call(
            "DELETE",
            &format!("/api/login/{}", later["id"].as_str().unwrap()),
            json!({}),
        )
        .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        admin.logins.logged_out().is_empty(),
        "a later cancelled attempt signs nothing out"
    );

    // A page that does save in time keeps its tokens.
    admin.logins.logged_out.lock().unwrap().clear();
    let (_, started) = admin
        .call(
            "POST",
            "/api/login",
            json!({"url": url, "clientId": "native-client"}),
        )
        .await;
    admin.logins.set(LoginState::Connected);
    let (_, done) = admin
        .call(
            "POST",
            &format!("/api/login/{}/complete", started["id"].as_str().unwrap()),
            json!({}),
        )
        .await;
    assert_eq!(done["state"], "connected");
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(admin.logins.logged_out().is_empty());
}

#[tokio::test]
async fn a_slow_environment_does_not_hold_up_the_others() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let admin = Admin::start().await;
    let other = admin.add("Beta", &upstream.url("beta"), BETA_KEY).await;
    admin
        .add("Alpha", &upstream.url("alpha"), CONTEXT_KEY)
        .await;
    // An address that says no only once the work on the active environment is
    // done, or after a minute. If the edit held that work up, the address is
    // released by the minute and not by the work: the order shows it, not a
    // measured duration.
    #[derive(Default)]
    struct Slow {
        asked: std::sync::atomic::AtomicBool,
        work_done: std::sync::atomic::AtomicBool,
        released_by_work: std::sync::atomic::AtomicBool,
    }
    let gate = Arc::new(Slow::default());
    let slow = serve(
        Router::new()
            .fallback(|State(gate): State<Arc<Slow>>| async move {
                gate.asked.store(true, Ordering::SeqCst);
                for _ in 0..600 {
                    if gate.work_done.load(Ordering::SeqCst) {
                        gate.released_by_work.store(true, Ordering::SeqCst);
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                StatusCode::UNAUTHORIZED
            })
            .with_state(gate.clone()),
    )
    .await;
    let path = format!("/api/connections/{other}");
    let edit = admin.call(
        "PUT",
        &path,
        json!({"url": format!("{slow}/projects/beta/mcp"), "auth": {"type": "api_key", "key": "new-key"}}),
    );
    let work = async {
        while !gate.asked.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let (status, _) = admin.tool("ax_status", json!({})).await;
        let (info_status, _) = admin.get("/api/remote/info").await;
        gate.work_done.store(true, Ordering::SeqCst);
        (status, info_status)
    };
    let ((edit_status, edit_body), (status, info_status)) = tokio::join!(edit, work);
    assert_eq!(edit_status, StatusCode::UNPROCESSABLE_ENTITY, "{edit_body}");
    assert_eq!((status, info_status), (StatusCode::OK, StatusCode::OK));
    assert!(
        gate.released_by_work.load(Ordering::SeqCst),
        "the active environment answered while the slow address was still checking"
    );
    // The refused edit changed nothing.
    assert_eq!(
        admin.secrets.get(&other).unwrap().as_deref(),
        Some(BETA_KEY)
    );
}

#[tokio::test]
async fn a10_tokens_of_a_sign_in_nobody_keeps_are_removed() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let admin = Admin::start().await;
    *admin.logins.token.lock().unwrap() = CONTEXT_KEY.to_owned();
    let url = upstream.url("alpha");
    let start = json!({"url": url, "clientId": "native-client"});

    // Cancel arrives after the browser answered: the tokens are stored a
    // moment later, and removed again.
    admin.logins.cancel_too_late.store(true, Ordering::SeqCst);
    let (_, started) = admin.call("POST", "/api/login", start.clone()).await;
    let id = started["id"].as_str().unwrap().to_owned();
    let (status, _) = admin
        .call("DELETE", &format!("/api/login/{id}"), json!({}))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(admin.logins.logged_out().is_empty());
    admin.logins.set(LoginState::Connected);
    for _ in 0..40 {
        if !admin.logins.logged_out().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(admin.logins.logged_out(), std::slice::from_ref(&url));
    assert!(!admin.config_file().exists());

    // A is cancelled too late, and B for the same address is started at once.
    // When A's tokens turn up they may be B's: B is still waiting, so they stay.
    admin.logins.logged_out.lock().unwrap().clear();
    let (_, first) = admin.call("POST", "/api/login", start.clone()).await;
    admin
        .call(
            "DELETE",
            &format!("/api/login/{}", first["id"].as_str().unwrap()),
            json!({}),
        )
        .await;
    admin.logins.cancel_too_late.store(false, Ordering::SeqCst);
    let (_, second) = admin.call("POST", "/api/login", start.clone()).await;
    admin
        .logins
        .set_for(first["id"].as_str().unwrap(), LoginState::Connected);
    admin.logins.set(LoginState::Connected);
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(
        admin.logins.logged_out().is_empty(),
        "the newer attempt keeps its tokens"
    );
    let (_, saved) = admin
        .call(
            "POST",
            &format!("/api/login/{}/complete", second["id"].as_str().unwrap()),
            json!({}),
        )
        .await;
    assert_eq!(saved["state"], "connected");
    admin
        .call(
            "DELETE",
            &format!(
                "/api/connections/{}",
                saved["connection"]["id"].as_str().unwrap()
            ),
            json!({}),
        )
        .await;
    admin.logins.logged_out.lock().unwrap().clear();

    // With a saved connection on that sign-in, a later attempt that fails or
    // is abandoned must not sign the saved one out.
    admin.logins.cancel_too_late.store(false, Ordering::SeqCst);
    admin.logins.logged_out.lock().unwrap().clear();
    let (_, kept) = admin.call("POST", "/api/login", start.clone()).await;
    admin.logins.set(LoginState::Connected);
    let (_, kept) = admin
        .call(
            "POST",
            &format!("/api/login/{}/complete", kept["id"].as_str().unwrap()),
            json!({}),
        )
        .await;
    assert_eq!(kept["state"], "connected");
    // The next attempt signs in, but the server now refuses the token.
    let (_, again) = admin.call("POST", "/api/login", start.clone()).await;
    *admin.logins.token.lock().unwrap() = "a-token-the-server-refuses".to_owned();
    admin.logins.set(LoginState::Connected);
    let (_, refused) = admin
        .call(
            "POST",
            &format!("/api/login/{}/complete", again["id"].as_str().unwrap()),
            json!({}),
        )
        .await;
    assert_eq!(refused["state"], "failed");
    // And one that is cancelled too late.
    admin.logins.cancel_too_late.store(true, Ordering::SeqCst);
    let (_, late) = admin.call("POST", "/api/login", start).await;
    admin.logins.set(LoginState::Connected);
    admin
        .call(
            "DELETE",
            &format!("/api/login/{}", late["id"].as_str().unwrap()),
            json!({}),
        )
        .await;
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(
        admin.logins.logged_out().is_empty(),
        "the saved connection keeps its tokens"
    );
    let (_, list) = admin.get("/api/connections").await;
    assert_eq!(list["connections"].as_array().unwrap().len(), 1);
}

// ----------------------------------------------------------------- MCP client

/// A scripted MCP server. `reply` sees the JSON-RPC method, how many times it
/// was called before, and the request headers.
type Reply = Box<dyn Fn(&str, usize, &HeaderMap, &Value) -> Response + Send + Sync>;

struct Script {
    calls: Mutex<Vec<(String, HeaderMap)>>,
    reply: Reply,
}

impl Script {
    fn count(&self, method: &str) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(m, _)| m == method)
            .count()
    }
    fn headers(&self, method: &str) -> Vec<HeaderMap> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(m, _)| m == method)
            .map(|(_, h)| h.clone())
            .collect()
    }
}

fn rpc_ok(result: Value) -> Response {
    axum::Json(json!({"jsonrpc": "2.0", "id": 1, "result": result})).into_response()
}

fn initialized(version: &str, session: &str) -> Response {
    let mut response = rpc_ok(json!({
        "protocolVersion": version,
        "serverInfo": {"name": "scripted", "version": "0.0.1"},
        "capabilities": {}
    }));
    response
        .headers_mut()
        .insert("mcp-session-id", session.parse().unwrap());
    response
}

fn tool_list(names: &[&str]) -> Response {
    rpc_ok(
        json!({"tools": names.iter().map(|n| json!({"name": n, "inputSchema": {"type": "object", "properties": {}}})).collect::<Vec<_>>()}),
    )
}

async fn scripted(
    reply: impl Fn(&str, usize, &HeaderMap, &Value) -> Response + Send + Sync + 'static,
) -> (Arc<Script>, url::Url) {
    let script = Arc::new(Script {
        calls: Mutex::new(Vec::new()),
        reply: Box::new(reply),
    });
    let app = Router::new()
        .fallback(
            |State(script): State<Arc<Script>>, headers: HeaderMap, body: String| async move {
                let request: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                let method = request["method"].as_str().unwrap_or("").to_owned();
                let nth = script.count(&method);
                script
                    .calls
                    .lock()
                    .unwrap()
                    .push((method.clone(), headers.clone()));
                if method == "notifications/initialized" {
                    return StatusCode::ACCEPTED.into_response();
                }
                (script.reply)(&method, nth, &headers, &request)
            },
        )
        .with_state(script.clone());
    let base = serve(app).await;
    (
        script,
        format!("{base}/projects/alpha/mcp").parse().unwrap(),
    )
}

fn remote(url: url::Url) -> Remote {
    Remote::new(url, Arc::new(StaticToken("scripted-key".into()))).unwrap()
}

#[tokio::test]
async fn b1_the_negotiated_protocol_version_and_session_are_sent_on_later_calls() {
    isolate_usage_db();
    for version in ["2025-11-25", "2025-06-18", "2025-03-26"] {
        let (script, url) = scripted(move |method, _, _, _| match method {
            "initialize" => initialized(version, "session-1"),
            _ => tool_list(&["ax_status"]),
        })
        .await;
        let info = remote(url).info().await.unwrap();
        assert_eq!(info.protocol_version, version);
        assert_eq!(info.server_name, "scripted");
        let init = &script.headers("initialize")[0];
        assert!(init.get("mcp-session-id").is_none());
        assert_eq!(init["authorization"], "Bearer scripted-key");
        assert_eq!(init["accept"], "application/json, text/event-stream");
        assert!(
            init.get("origin").is_none(),
            "a native client sends no Origin"
        );
        for method in ["notifications/initialized", "tools/list"] {
            let headers = &script.headers(method)[0];
            assert_eq!(headers["mcp-protocol-version"], version, "{method}");
            assert_eq!(headers["mcp-session-id"], "session-1", "{method}");
        }
    }
    // A version this client does not speak is an error, not a guess.
    let (_, url) = scripted(|method, _, _, _| match method {
        "initialize" => initialized("2099-01-01", "s"),
        _ => tool_list(&[]),
    })
    .await;
    assert!(matches!(
        remote(url).info().await,
        Err(McpError::Protocol(_))
    ));
}

fn expired() -> Response {
    (
        StatusCode::NOT_FOUND,
        axum::Json(json!({"error": "Unknown or expired MCP session"})),
    )
        .into_response()
}

#[tokio::test]
async fn b2_an_expired_session_is_reopened_once() {
    isolate_usage_db();
    // First tools/list fails with an expired session; the retry succeeds.
    let (script, url) = scripted(|method, nth, headers, _| match method {
        "initialize" => initialized("2025-11-25", if nth == 0 { "old" } else { "new" }),
        "tools/list" if headers["mcp-session-id"] == "old" => expired(),
        _ => tool_list(&["ax_status"]),
    })
    .await;
    let info = remote(url).info().await.unwrap();
    assert_eq!(info.tools.len(), 1);
    assert_eq!(script.count("initialize"), 2);
    assert_eq!(script.count("tools/list"), 2);

    // A server that keeps answering "expired" is reported after one retry.
    let (script, url) = scripted(|method, _, _, _| match method {
        "initialize" => initialized("2025-11-25", "s"),
        _ => expired(),
    })
    .await;
    assert_eq!(
        remote(url).info().await.unwrap_err(),
        McpError::UnknownProject
    );
    assert_eq!(script.count("initialize"), 2);
    assert_eq!(script.count("tools/list"), 2);
}

/// Two calls in flight on the same expired session renew it once. The second
/// call's "expired" answer arrives after the first call opened a new session;
/// that new session must be used, not thrown away and opened a third time.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn b2_two_calls_on_one_expired_session_reopen_it_once() {
    isolate_usage_db();
    #[derive(Default)]
    struct Restarted {
        inits: AtomicUsize,
        stale_calls: AtomicUsize,
    }
    let state = Arc::new(Restarted::default());
    let app = Router::new()
        .fallback(
            |State(state): State<Arc<Restarted>>, headers: HeaderMap, body: String| async move {
                let request: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                let session = headers
                    .get("mcp-session-id")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_owned();
                match request["method"].as_str().unwrap_or("") {
                    "initialize" => {
                        let n = state.inits.fetch_add(1, Ordering::SeqCst);
                        initialized("2025-11-25", &format!("s{n}"))
                    }
                    "notifications/initialized" => StatusCode::ACCEPTED.into_response(),
                    "tools/list" => tool_list(&["ax_status"]),
                    _ if session == "s0" => {
                        // The server restarted: the first session is gone. The
                        // second stale call is answered only after the first
                        // one has opened a new session.
                        if state.stale_calls.fetch_add(1, Ordering::SeqCst) > 0 {
                            for _ in 0..500 {
                                if state.inits.load(Ordering::SeqCst) >= 2 {
                                    break;
                                }
                                tokio::time::sleep(Duration::from_millis(10)).await;
                            }
                        }
                        expired()
                    }
                    _ => rpc_ok(json!({"content": [{"type": "text", "text": "ok"}]})),
                }
            },
        )
        .with_state(state.clone());
    let base = serve(app).await;
    let client = Arc::new(remote(
        format!("{base}/projects/alpha/mcp").parse().unwrap(),
    ));
    client.info().await.unwrap();
    let (a, b) = tokio::join!(
        client.call_tool("ax_status", json!({})),
        client.call_tool("ax_status", json!({}))
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(state.stale_calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        state.inits.load(Ordering::SeqCst),
        2,
        "the expired session is renewed once, and the renewed one is kept"
    );
}

/// The long admin timeout follows what the server says about a tool (its
/// `ax:admin` scope), not a list of names kept in ax-admin.
#[tokio::test]
async fn a_tool_the_server_marks_as_admin_gets_the_admin_timeout() {
    isolate_usage_db();
    fn listed(name: &str, scopes: &[&str]) -> Value {
        let schemes = json!([{"type": "oauth2", "scopes": scopes}]);
        json!({"name": name, "inputSchema": {"type": "object", "properties": {}},
            "securitySchemes": schemes, "_meta": {"securitySchemes": schemes}})
    }
    let app = Router::new().fallback(|body: String| async move {
        let request: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        match request["method"].as_str().unwrap_or("") {
            "initialize" => initialized("2025-11-25", "s"),
            "notifications/initialized" => StatusCode::ACCEPTED.into_response(),
            "tools/list" => rpc_ok(json!({"tools": [
                listed("ax_compact", &["ax:context", "ax:write", "ax:admin"]),
                listed("ax_status", &["ax:context"]),
            ]})),
            _ => {
                // Longer than a context call may take, well within an admin call.
                tokio::time::sleep(Duration::from_millis(600)).await;
                rpc_ok(json!({"content": []}))
            }
        }
    });
    let url: url::Url = format!("{}/projects/alpha/mcp", serve(app).await)
        .parse()
        .unwrap();
    let limits = Limits {
        handshake: Duration::from_secs(5),
        call: Duration::from_millis(200),
        admin_call: Duration::from_secs(10),
        max_bytes: 64 * 1024,
    };
    let client = Remote::with_limits(url, Arc::new(StaticToken("k".into())), limits).unwrap();
    client
        .call_tool("ax_compact", json!({}))
        .await
        .expect("an admin tool may take as long as an admin call");
    assert_eq!(
        client.call_tool("ax_status", json!({})).await.unwrap_err(),
        McpError::Timeout,
        "a context tool keeps the short timeout"
    );
}

/// The real remote server marks its management tools with the admin scope and
/// its context tools without it, which is what ax-admin goes by.
#[tokio::test]
async fn the_remote_server_marks_exactly_its_management_tools_as_admin() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let tools = |key: &str| {
        let url: url::Url = upstream.url("alpha").parse().unwrap();
        let key = key.to_owned();
        async move {
            Remote::new(url, Arc::new(StaticToken(key)))
                .unwrap()
                .info()
                .await
                .unwrap()
                .tools
        }
    };
    let admin: Vec<String> = tools(ADMIN_KEY)
        .await
        .iter()
        .filter(|tool| requires_admin(tool))
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect();
    for name in [
        "ax_sync",
        "ax_index",
        "ax_policy_index",
        "ax_files",
        "ax_logs",
    ] {
        assert!(admin.iter().any(|n| n == name), "{name} in {admin:?}");
    }
    assert!(!admin.iter().any(|n| n == "ax_status" || n == "ax_explore"));
    assert!(tools(CONTEXT_KEY)
        .await
        .iter()
        .all(|tool| !requires_admin(tool)));
}

struct Rotating {
    asked: AtomicUsize,
    refreshable: bool,
}

impl TokenSource for Rotating {
    fn token(&self, after_rejection: bool) -> TokenFuture<'_> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { Ok(if after_rejection { "fresh" } else { "stale" }.to_owned()) })
    }
    fn refreshable(&self) -> bool {
        self.refreshable
    }
}

#[tokio::test]
async fn b3_a_rejected_oauth_token_is_refreshed_once_and_an_api_key_is_not() {
    isolate_usage_db();
    let reply = |method: &str, _: usize, headers: &HeaderMap, _: &Value| {
        if headers["authorization"] != "Bearer fresh" {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        match method {
            "initialize" => initialized("2025-11-25", "s"),
            _ => tool_list(&["ax_status"]),
        }
    };
    let (script, url) = scripted(reply).await;
    let tokens = Arc::new(Rotating {
        asked: AtomicUsize::new(0),
        refreshable: true,
    });
    let info = Remote::new(url, tokens.clone())
        .unwrap()
        .info()
        .await
        .unwrap();
    assert_eq!(info.tools.len(), 1);
    assert_eq!(tokens.asked.load(Ordering::SeqCst), 2);
    assert_eq!(script.count("initialize"), 2);

    // An API key has nothing to refresh: one attempt, then the rejection.
    let (script, url) = scripted(reply).await;
    let tokens = Arc::new(Rotating {
        asked: AtomicUsize::new(0),
        refreshable: false,
    });
    let error = Remote::new(url, tokens.clone())
        .unwrap()
        .info()
        .await
        .unwrap_err();
    assert_eq!(error, McpError::Rejected);
    assert_eq!(tokens.asked.load(Ordering::SeqCst), 1);
    assert_eq!(script.count("initialize"), 1);

    // A refreshed token that is rejected again is reported, not retried forever.
    let (script, url) = scripted(|_, _, _, _| StatusCode::UNAUTHORIZED.into_response()).await;
    let tokens = Arc::new(Rotating {
        asked: AtomicUsize::new(0),
        refreshable: true,
    });
    let error = Remote::new(url, tokens.clone())
        .unwrap()
        .info()
        .await
        .unwrap_err();
    assert_eq!(error, McpError::Rejected);
    assert_eq!(script.count("initialize"), 2);
}

#[tokio::test]
async fn the_servers_timeouts_and_busy_answers_are_told_apart() {
    isolate_usage_db();
    async fn outcome(status: StatusCode, body: Option<Value>) -> McpError {
        let (_, url) = scripted(move |method, _, _, _| match method {
            "initialize" => initialized("2025-11-25", "s"),
            "tools/list" => tool_list(&["ax_index"]),
            _ => match &body {
                Some(body) => (status, axum::Json(body.clone())).into_response(),
                None => (status, "upstream trouble").into_response(),
            },
        })
        .await;
        remote(url)
            .call_tool("ax_index", json!({}))
            .await
            .unwrap_err()
    }
    let gateway_timeout = StatusCode::GATEWAY_TIMEOUT;
    // Only this answer means the work goes on.
    assert_eq!(
        outcome(
            gateway_timeout,
            Some(json!({"error": "The operation is still running on the server", "running": true}))
        )
        .await,
        McpError::StillRunning
    );
    // Ax gave up on the call itself.
    assert_eq!(
        outcome(gateway_timeout, Some(json!({"error": "Ax tool timed out"}))).await,
        McpError::Timeout
    );
    assert_eq!(
        outcome(
            gateway_timeout,
            Some(json!({"error": "x", "running": false}))
        )
        .await,
        McpError::Timeout
    );
    // A gateway's own page is not an answer from Ax.
    assert_eq!(outcome(gateway_timeout, None).await, McpError::NotMcp(504));
    assert_eq!(
        outcome(StatusCode::SERVICE_UNAVAILABLE, Some(json!({"error": "This project is being re-indexed; try again when it is done", "reindexing": true}))).await,
        McpError::Reindexing
    );
    // A platform's own "service unavailable", even as JSON, is not that.
    assert_eq!(
        outcome(
            StatusCode::SERVICE_UNAVAILABLE,
            Some(json!({"error": "no healthy upstream"}))
        )
        .await,
        McpError::NotMcp(503)
    );
    assert_eq!(
        outcome(StatusCode::SERVICE_UNAVAILABLE, None).await,
        McpError::NotMcp(503)
    );
    assert_eq!(
        outcome(StatusCode::TOO_MANY_REQUESTS, None).await,
        McpError::Busy
    );
}

#[tokio::test]
async fn b4_b5_only_listed_tools_are_called_and_unknown_listed_tools_work() {
    isolate_usage_db();
    let (script, url) = scripted(|method, _, _, request| match method {
        "initialize" => initialized("2025-11-25", "s"),
        "tools/list" => tool_list(&["ax_status", "ax_future_tool"]),
        _ => rpc_ok(json!({
            "content": [{"type": "text", "text": format!("called {}", request["params"]["name"].as_str().unwrap())}],
            "structuredContent": {"arguments": request["params"]["arguments"]},
        })),
    })
    .await;
    let remote = remote(url);
    let info = remote.info().await.unwrap();
    let names: Vec<_> = info
        .tools
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["ax_status", "ax_future_tool"]);
    assert!(!names.contains(&"ax_remember"));

    let before = script.calls.lock().unwrap().len();
    for hidden in ["ax_remember", "ax_index", "ax_ship", "", "ax_status "] {
        assert_eq!(
            remote.call_tool(hidden, json!({})).await.unwrap_err(),
            McpError::UnknownTool(hidden.into())
        );
    }
    assert_eq!(
        script.calls.lock().unwrap().len(),
        before,
        "no request was sent"
    );

    // A tool this version of ax-admin has never heard of is still callable.
    let result = remote
        .call_tool("ax_future_tool", json!({"depth": 2}))
        .await
        .unwrap();
    assert_eq!(result["content"][0]["text"], "called ax_future_tool");
    assert_eq!(
        result["structuredContent"]["arguments"],
        json!({"depth": 2})
    );
    assert_eq!(script.count("tools/call"), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn b6_slow_and_oversized_answers_are_errors() {
    isolate_usage_db();
    let limits = Limits {
        handshake: Duration::from_millis(300),
        call: Duration::from_millis(300),
        admin_call: Duration::from_millis(1500),
        max_bytes: 64 * 1024,
    };
    let tokens = || -> Arc<dyn TokenSource> { Arc::new(StaticToken("k".into())) };

    // A server that accepts the connection and never answers.
    let silent = serve(Router::new().fallback(|| async {
        tokio::time::sleep(Duration::from_secs(30)).await;
        StatusCode::OK
    }))
    .await;
    let url: url::Url = format!("{silent}/projects/alpha/mcp").parse().unwrap();
    let started = std::time::Instant::now();
    let error = Remote::with_limits(url, tokens(), limits)
        .unwrap()
        .info()
        .await
        .unwrap_err();
    assert_eq!(error, McpError::Timeout);
    assert!(started.elapsed() < Duration::from_secs(5));

    // Tool calls: the context timeout applies to ax_status, the longer one to
    // the tools the server lists with the admin scope, as the real server does.
    let (_, url) = scripted(|method, _, _, request| match method {
        "initialize" => initialized("2025-11-25", "s"),
        "tools/list" => {
            let admin = json!([{"type": "oauth2", "scopes": ["ax:context", "ax:write", "ax:admin"]}]);
            rpc_ok(json!({"tools": [
                {"name": "ax_status", "inputSchema": {"type": "object"}},
                {"name": "ax_index", "inputSchema": {"type": "object"}, "securitySchemes": admin},
                {"name": "ax_files", "inputSchema": {"type": "object"}, "_meta": {"securitySchemes": admin}},
            ]}))
        }
        _ => {
            let name = request["params"]["name"].as_str().unwrap().to_owned();
            std::thread::sleep(Duration::from_millis(700));
            rpc_ok(json!({"content": [{"type": "text", "text": name}]}))
        }
    })
    .await;
    let remote = Remote::with_limits(url, tokens(), limits).unwrap();
    assert_eq!(
        remote.call_tool("ax_status", json!({})).await.unwrap_err(),
        McpError::Timeout
    );
    assert!(remote.call_tool("ax_index", json!({})).await.is_ok());
    assert!(remote.call_tool("ax_files", json!({})).await.is_ok());

    // An answer larger than the limit, with and without a Content-Length.
    let (_, url) = scripted(|method, _, _, _| match method {
        "initialize" => initialized("2025-11-25", "s"),
        _ => rpc_ok(json!({"tools": [], "padding": "x".repeat(200 * 1024)})),
    })
    .await;
    let error = Remote::with_limits(url, tokens(), limits)
        .unwrap()
        .info()
        .await
        .unwrap_err();
    assert_eq!(error, McpError::TooLarge);
    let (_, url) = scripted(|method, _, _, _| match method {
        "initialize" => initialized("2025-11-25", "s"),
        _ => {
            let chunks = (0..64).map(|_| Ok::<_, std::io::Error>(vec![b' '; 4096]));
            Response::new(Body::from_stream(Chunks(chunks)))
        }
    })
    .await;
    let error = Remote::with_limits(url, tokens(), limits)
        .unwrap()
        .info()
        .await
        .unwrap_err();
    assert_eq!(error, McpError::TooLarge);
}

/// A stream over an iterator, so the body is sent without a Content-Length.
struct Chunks<I>(I);

impl<I: Iterator + Unpin> futures_core::Stream for Chunks<I> {
    type Item = I::Item;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<I::Item>> {
        std::task::Poll::Ready(self.0.next())
    }
}

// ------------------------------------------------------------ loopback server

#[tokio::test]
async fn c1_the_listener_is_bound_to_loopback() {
    isolate_usage_db();
    let listener = bind(0).await.unwrap();
    assert!(listener.local_addr().unwrap().ip().is_loopback());
}

#[tokio::test]
async fn c2_other_hosts_and_origins_are_refused() {
    isolate_usage_db();
    let admin = Admin::start().await;
    let get = |host: Option<&str>, origin: Option<&str>| {
        let mut request = Request::builder().method("GET").uri("/api/connections");
        if let Some(host) = host {
            request = request.header("host", host);
        }
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        admin.send(request.body(Body::empty()).unwrap())
    };
    let ok = format!("127.0.0.1:{PORT}");
    for host in [ok.as_str(), "localhost:7080", "[::1]:7080"] {
        assert_eq!(get(Some(host), None).await.0, StatusCode::OK, "{host}");
    }
    for host in [
        None,
        Some("ax.example.com"),
        Some("127.0.0.1"),
        Some("127.0.0.1:7081"),
        Some("127.0.0.1.example.com:7080"),
        Some("192.168.1.10:7080"),
        Some("evil.localhost:7080"),
    ] {
        let (status, headers, body) = get(host, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{host:?}");
        assert_eq!(body["error"], "Request host is not allowed");
        assert_eq!(headers["cache-control"], "no-store");
        assert!(body.get("csrf").is_none());
    }
    assert_eq!(
        get(Some(&ok), Some("http://127.0.0.1:7080")).await.0,
        StatusCode::OK
    );
    assert_eq!(
        get(Some(&ok), Some("http://localhost:7080")).await.0,
        StatusCode::OK
    );
    for origin in [
        "https://evil.example.com",
        "http://127.0.0.1:9999",
        "https://127.0.0.1:7080",
        "http://127.0.0.1:7080.evil.example.com",
        "null",
    ] {
        let (status, _, body) = get(Some(&ok), Some(origin)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{origin}");
        assert_eq!(body["error"], "Request origin is not allowed");
    }
    // The page itself is behind the same check.
    let (status, _, _) = admin
        .send(
            Request::builder()
                .uri("/")
                .header("host", "ax.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn c3_changes_need_the_request_token() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let admin = Admin::start().await;
    let id = admin
        .add("Alpha", &upstream.url("alpha"), CONTEXT_KEY)
        .await;
    let body = json!({"url": upstream.url("beta"), "auth": {"type": "api_key", "key": BETA_KEY}});
    let routes = [
        ("POST", "/api/connections".to_string()),
        ("POST", "/api/connections/test".to_string()),
        ("PUT", format!("/api/connections/{id}")),
        ("DELETE", format!("/api/connections/{id}")),
        ("POST", format!("/api/connections/{id}/activate")),
        ("POST", "/api/login".to_string()),
        ("DELETE", "/api/login/attempt-1".to_string()),
        ("POST", "/api/login/attempt-1/complete".to_string()),
        ("POST", "/api/remote/tools/ax_status".to_string()),
    ];
    for (method, path) in &routes {
        for token in [None, Some(""), Some("wrong"), Some(&admin.csrf[1..])] {
            let mut request = admin
                .request(method, path)
                .header("content-type", "application/json");
            if let Some(token) = token {
                request = request.header(CSRF_HEADER, token);
            }
            let (status, _, response) = admin
                .send(request.body(Body::from(body.to_string())).unwrap())
                .await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{method} {path} {token:?}");
            assert_eq!(response["code"], "forbidden");
        }
    }
    // Nothing was changed by the refused requests.
    let (_, list) = admin.get("/api/connections").await;
    assert_eq!(list["connections"].as_array().unwrap().len(), 1);
    assert_eq!(list["connections"][0]["name"], "Alpha");
    assert_eq!(admin.secrets.len(), 1);
}

#[tokio::test]
async fn c4_responses_are_never_cached() {
    isolate_usage_db();
    let upstream = Upstream::start().await;
    let admin = Admin::start().await;
    let check = |headers: &HeaderMap, what: &str| {
        assert_eq!(headers["cache-control"], "no-store", "{what}");
    };
    for path in [
        "/api/connections",
        "/api/remote/info",
        "/api/login/none",
        "/api/nope",
        "/",
        "/some/page",
    ] {
        let (_, headers, _) = admin
            .send(admin.request("GET", path).body(Body::empty()).unwrap())
            .await;
        check(&headers, path);
    }
    let id = admin
        .add("Alpha", &upstream.url("alpha"), CONTEXT_KEY)
        .await;
    let (_, headers, _) = admin
        .send(
            admin
                .request("GET", "/api/remote/info")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    check(&headers, "info with a connection");
    let (status, headers, _) = admin
        .send(
            admin
                .request("POST", "/api/remote/tools/ax_status")
                .header(CSRF_HEADER, &admin.csrf)
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"connection": id, "arguments": {}}).to_string(),
                ))
                .unwrap(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    check(&headers, "tool call");
}

#[tokio::test]
async fn the_interface_is_served_with_a_restrictive_content_policy() {
    isolate_usage_db();
    let admin = Admin::start().await;
    for path in ["/", "/memory", "/index.html"] {
        let response = admin
            .router
            .clone()
            .oneshot(admin.request("GET", path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert_eq!(
            response.headers()["content-type"],
            "text/html; charset=utf-8"
        );
        let policy = response.headers()["content-security-policy"]
            .to_str()
            .unwrap();
        assert!(policy.contains("default-src 'self'") && policy.contains("frame-ancestors 'none'"));
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("<meta charset=\"UTF-8\""));
    }
}
