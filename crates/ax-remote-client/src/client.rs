use crate::{
    credentials::{self, Connection, Tokens},
    https_url,
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use oauth2::{CsrfToken, PkceCodeChallenge};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{oneshot, Mutex},
};
use url::Url;

#[derive(Clone, Serialize)]
pub struct LoginStatus {
    pub id: String,
    pub state: String,
    pub message: Option<String>,
}
#[derive(Serialize)]
pub struct LoginStart {
    pub id: String,
    pub authorization_url: String,
}
struct Attempt {
    status: LoginStatus,
    started: std::time::Instant,
    cancel: CallbackSender,
}
fn attempts() -> &'static Mutex<HashMap<String, Attempt>> {
    static ATTEMPTS: OnceLock<Mutex<HashMap<String, Attempt>>> = OnceLock::new();
    ATTEMPTS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "Cannot initialize HTTP client".into())
}

#[derive(Deserialize)]
struct ResourceMetadata {
    resource: String,
    authorization_servers: Vec<String>,
}
#[derive(Deserialize)]
struct OAuthMetadata {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    revocation_endpoint: Option<String>,
    #[serde(default)]
    code_challenge_methods_supported: Vec<String>,
    #[serde(default)]
    authorization_response_iss_parameter_supported: bool,
}

async fn bounded_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, String> {
    bounded_json_limit(response, 128 * 1024).await
}

async fn bounded_json_limit<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
    limit: usize,
) -> Result<T, String> {
    let mut response = response
        .error_for_status()
        .map_err(|_| "OAuth service returned an error")?;
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err("OAuth response is too large".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Cannot read OAuth response")?
    {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err("OAuth response is too large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid OAuth response".into())
}

async fn discover(url: &str) -> Result<(ResourceMetadata, OAuthMetadata), String> {
    discover_with(&http_client()?, url, https_url).await
}

/// `discover` with the HTTP client and the URL check (`https_url` outside tests)
/// passed in.
async fn discover_with(
    client: &reqwest::Client,
    url: &str,
    https_url: fn(&str) -> Result<Url, String>,
) -> Result<(ResourceMetadata, OAuthMetadata), String> {
    let project_url = https_url(url)?;
    if !project_url.path().starts_with("/projects/") || !project_url.path().ends_with("/mcp") {
        return Err("Use the server's /projects/PROJECT/mcp URL".into());
    }
    let origin = project_url.origin().ascii_serialization();
    let resource: ResourceMetadata = bounded_json(
        client
            .get(format!("{origin}/.well-known/oauth-protected-resource"))
            .send()
            .await
            .map_err(|_| "Cannot discover remote MCP server")?,
    )
    .await?;
    if resource.resource != origin {
        return Err("Remote resource does not match server origin".into());
    }
    let issuer = resource
        .authorization_servers
        .first()
        .ok_or("No OAuth issuer advertised")?;
    let issuer_url = https_url(issuer)?;
    let discovery = format!(
        "{}/.well-known/oauth-authorization-server{}",
        issuer_url.origin().ascii_serialization(),
        issuer_url.path().trim_end_matches('/')
    );
    let response = client
        .get(discovery)
        .send()
        .await
        .map_err(|_| "Cannot discover OAuth issuer")?;
    let metadata: OAuthMetadata = if response.status() == reqwest::StatusCode::NOT_FOUND {
        bounded_json(
            client
                .get(format!(
                    "{}/.well-known/openid-configuration",
                    issuer.trim_end_matches('/')
                ))
                .send()
                .await
                .map_err(|_| "Cannot discover OAuth issuer")?,
        )
        .await?
    } else {
        bounded_json(response).await?
    };
    if &metadata.issuer != issuer {
        return Err("OAuth issuer metadata does not match the advertised issuer".into());
    }
    https_url(&metadata.authorization_endpoint)?;
    https_url(&metadata.token_endpoint)?;
    if let Some(endpoint) = &metadata.revocation_endpoint {
        https_url(endpoint)?;
    }
    if !metadata
        .code_challenge_methods_supported
        .iter()
        .any(|s| s == "S256")
    {
        return Err("OAuth provider must support PKCE S256".into());
    }
    Ok((resource, metadata))
}

type CallbackSender = Arc<Mutex<Option<oneshot::Sender<Result<String, String>>>>>;

#[derive(Clone)]
struct CallbackState {
    expected_state: String,
    issuer: String,
    require_issuer: bool,
    port: u16,
    sender: CallbackSender,
}

fn same_secret(a: &str, b: &str) -> bool {
    bool::from(
        Sha256::digest(a.as_bytes())
            .as_slice()
            .ct_eq(Sha256::digest(b.as_bytes()).as_slice()),
    )
}

async fn callback(
    State(state): State<CallbackState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    if headers.get("host").and_then(|h| h.to_str().ok())
        != Some(format!("127.0.0.1:{}", state.port).as_str())
    {
        return (StatusCode::FORBIDDEN, "Invalid callback host");
    }
    if !query
        .get("state")
        .is_some_and(|value| same_secret(value, &state.expected_state))
    {
        return (
            StatusCode::BAD_REQUEST,
            "Invalid OAuth state; return to Ax and try again",
        );
    }
    let issuer_ok = match query.get("iss") {
        Some(issuer) => issuer == &state.issuer,
        None => !state.require_issuer,
    };
    if !issuer_ok {
        return (
            StatusCode::BAD_REQUEST,
            "Invalid OAuth issuer; return to Ax",
        );
    }
    let result = if query.contains_key("error") {
        Err("Sign-in cancelled or denied by the OAuth provider".into())
    } else {
        query
            .get("code")
            .filter(|c| !c.is_empty())
            .cloned()
            .ok_or_else(|| "OAuth callback has no authorization code".into())
    };
    let Some(sender) = state.sender.lock().await.take() else {
        return (
            StatusCode::BAD_REQUEST,
            "This sign-in has already completed",
        );
    };
    let _ = sender.send(result);
    (
        StatusCode::OK,
        "Authorization received. Return to Ax to see whether the connection completed.",
    )
}

pub async fn start_login(
    url: String,
    client_id: String,
    write: bool,
) -> Result<LoginStart, String> {
    start_login_scoped(url, client_id, write, false).await
}

/// The scope string of a sign-in. `ax:admin` is requested only on demand: an
/// authorization server that does not issue it rejects the whole request.
pub fn login_scope(write: bool, admin: bool) -> String {
    let mut scope = String::from("ax:context");
    if write || admin {
        scope.push_str(" ax:write");
    }
    if admin {
        scope.push_str(" ax:admin");
    }
    scope.push_str(" offline_access");
    scope
}

/// Like `start_login`, optionally also asking for the admin scope (which needs
/// write access and an admin grant on the server).
pub async fn start_login_scoped(
    url: String,
    client_id: String,
    write: bool,
    admin: bool,
) -> Result<LoginStart, String> {
    start_login_as(url, client_id, write, admin, credentials::USER_AX).await
}

/// Like `start_login_scoped`, recording `user` as a user of the stored
/// sign-in so that [`release`] by another user leaves it in place.
pub async fn start_login_as(
    url: String,
    client_id: String,
    write: bool,
    admin: bool,
    user: &'static str,
) -> Result<LoginStart, String> {
    let scope = login_scope(write, admin);
    if client_id.trim().is_empty() || client_id.len() > 2048 {
        return Err("A registered native OAuth client ID is required".into());
    }
    let (resource, metadata) = discover(&url).await?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|_| "Cannot bind OAuth callback listener")?;
    let port = listener
        .local_addr()
        .map_err(|_| "Cannot resolve callback port")?
        .port();
    let redirect = format!("http://127.0.0.1:{port}/callback");
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let csrf = CsrfToken::new_random();
    let (sender, receiver) = oneshot::channel();
    let state = CallbackState {
        expected_state: csrf.secret().clone(),
        issuer: metadata.issuer.clone(),
        require_issuer: metadata.authorization_response_iss_parameter_supported,
        port,
        sender: Arc::new(Mutex::new(Some(sender))),
    };
    let mut authorization = Url::parse(&metadata.authorization_endpoint)
        .map_err(|_| "Invalid authorization endpoint")?;
    authorization.query_pairs_mut().extend_pairs([
        ("response_type", "code"),
        ("client_id", &client_id),
        ("redirect_uri", &redirect),
        ("state", csrf.secret()),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("resource", &resource.resource),
        ("scope", scope.as_str()),
    ]);
    let id = uuid::Uuid::new_v4().to_string();
    {
        let mut all = attempts().lock().await;
        all.retain(|_, a| a.started.elapsed() < Duration::from_secs(900));
        if all.len() >= 16 {
            return Err("Too many pending sign-ins; wait for a previous attempt to finish".into());
        }
        all.insert(
            id.clone(),
            Attempt {
                status: LoginStatus {
                    id: id.clone(),
                    state: "pending".into(),
                    message: None,
                },
                started: std::time::Instant::now(),
                cancel: state.sender.clone(),
            },
        );
    }
    let callback_task = tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            Router::new()
                .route("/callback", get(callback))
                .with_state(state),
        )
        .await;
    });
    let task_id = id.clone();
    tokio::spawn(async move {
        let result = async {
            let code = tokio::time::timeout(Duration::from_secs(300), receiver)
                .await
                .map_err(|_| "Sign-in timed out")?
                .map_err(|_| "Sign-in stopped")??;
            let client = http_client()?;
            let (connection, tokens) = exchange_login(
                &client,
                url,
                client_id,
                (resource, metadata),
                &code,
                &redirect,
                verifier.secret(),
            )
            .await?;
            let connection = credentials::Connection {
                users: vec![user.to_string()],
                ..connection
            };
            tokio::task::spawn_blocking(move || credentials::save(connection, &tokens))
                .await
                .map_err(|_| "Credential storage task failed")??;
            Ok::<(), String>(())
        }
        .await;
        callback_task.abort();
        if let Some(attempt) = attempts().lock().await.get_mut(&task_id) {
            attempt.status.state = if result.is_ok() {
                "connected"
            } else {
                "failed"
            }
            .into();
            attempt.status.message = result.err();
        }
    });
    Ok(LoginStart {
        id,
        authorization_url: authorization.to_string(),
    })
}

/// Exchange only with the discovered issuer and verify real project access before
/// handing credentials to storage. Transport is injectable for local issuer tests.
async fn exchange_login(
    client: &reqwest::Client,
    url: String,
    client_id: String,
    discovery: (ResourceMetadata, OAuthMetadata),
    code: &str,
    redirect: &str,
    verifier: &str,
) -> Result<(Connection, Tokens), String> {
    let (resource, metadata) = discovery;
    let response = client
        .post(&metadata.token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", &client_id),
            ("code", code),
            ("redirect_uri", redirect),
            ("code_verifier", verifier),
            ("resource", &resource.resource),
        ])
        .send()
        .await
        .map_err(|_| "Cannot exchange authorization code")?;
    let token: TokenResponse = bounded_json(response).await?;
    token.validate()?;
    verify_connection(client, &url, &token.access_token)
        .await
        .map_err(Verify::into_message)?;
    Ok((
        Connection {
            url,
            client_id,
            issuer: metadata.issuer,
            resource: resource.resource,
            token_endpoint: metadata.token_endpoint,
            revocation_endpoint: metadata.revocation_endpoint,
            expires_at: credentials::now() + token.expires_in,
            users: Vec::new(),
        },
        Tokens {
            access_token: token.access_token,
            refresh_token: token.refresh_token,
        },
    ))
}

async fn exchange_refresh(
    client: &reqwest::Client,
    connection: Connection,
    refresh: String,
) -> Result<(Connection, Tokens), String> {
    let response = client
        .post(&connection.token_endpoint)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", &connection.client_id),
            ("refresh_token", &refresh),
            ("resource", &connection.resource),
        ])
        .send()
        .await
        .map_err(|_| "Cannot refresh remote credentials")?;
    let response: TokenResponse = bounded_json(response).await?;
    response.validate()?;
    Ok((
        Connection {
            expires_at: credentials::now() + response.expires_in,
            ..connection
        },
        Tokens {
            access_token: response.access_token,
            refresh_token: Some(response.refresh_token.unwrap_or(refresh)),
        },
    ))
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    token_type: String,
    expires_in: u64,
}
impl TokenResponse {
    fn validate(&self) -> Result<(), String> {
        if self.access_token.is_empty()
            || !self.token_type.eq_ignore_ascii_case("bearer")
            || self.expires_in == 0
            || self.expires_in > 86400
        {
            Err("Provider did not issue a short-lived Bearer access token".into())
        } else {
            Ok(())
        }
    }
}

/// Why a token could not be shown to reach the project.
#[derive(Debug)]
enum Verify {
    /// The server answered 401 or 403: this account has no access.
    Denied,
    /// The server could not answer now (transport error, 5xx, 429, a bad reply).
    Unavailable(String),
}

impl Verify {
    fn into_message(self) -> String {
        match self {
            Verify::Denied => {
                "OAuth succeeded but this account cannot access the selected Ax project".into()
            }
            Verify::Unavailable(message) => message,
        }
    }
}

async fn verify_connection(client: &reqwest::Client, url: &str, token: &str) -> Result<(), Verify> {
    let response = client.post(url).bearer_auth(token).header("accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"ax-login","version":env!("CARGO_PKG_VERSION")}}}))
        .send().await.map_err(|_| Verify::Unavailable("Cannot verify authenticated MCP access".into()))?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(Verify::Denied);
    }
    if !status.is_success() {
        return Err(Verify::Unavailable(format!(
            "Cannot verify MCP access: the server returned HTTP {}",
            status.as_u16()
        )));
    }
    let session = response
        .headers()
        .get("mcp-session-id")
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned);
    let value: Value = bounded_json(response).await.map_err(Verify::Unavailable)?;
    if value.get("result").is_none() {
        return Err(Verify::Unavailable(
            "Remote MCP initialization failed".into(),
        ));
    }
    if let Some(session) = session {
        let _ = client
            .delete(url)
            .bearer_auth(token)
            .header("mcp-session-id", session)
            .send()
            .await;
    }
    Ok(())
}

pub async fn login_status(id: &str) -> Option<LoginStatus> {
    attempts().lock().await.get(id).map(|a| a.status.clone())
}

/// Cancel a pending browser login without waiting for its timeout.
pub async fn cancel_login(id: &str) -> Result<(), String> {
    let cancel = attempts()
        .lock()
        .await
        .get(id)
        .ok_or("Sign-in attempt unavailable")?
        .cancel
        .clone();
    let sender = cancel.lock().await.take()
        .ok_or("Sign-in already completed or is verifying access; wait for the result, then disconnect if needed")?;
    sender
        .send(Err("Sign-in cancelled".into()))
        .map_err(|_| "Sign-in attempt already stopped".to_string())
}

pub async fn login(url: String, client_id: String, write: bool) -> Result<(), String> {
    let started = start_login(url, client_id, write).await?;
    eprintln!(
        "Authorize Ax in your browser: {}",
        started.authorization_url
    );
    open_browser(&started.authorization_url);
    loop {
        let status = login_status(&started.id)
            .await
            .ok_or("Sign-in attempt unavailable")?;
        match status.state.as_str() {
            "connected" => {
                println!("Connected to remote Ax.");
                return Ok(());
            }
            "failed" => return Err(status.message.unwrap_or("Sign-in failed".into())),
            _ => tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(500)) => {},
                _ = tokio::signal::ctrl_c() => {
                    cancel_login(&started.id).await?;
                    return Err("Sign-in cancelled".into());
                }
            },
        }
    }
}

pub fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    let command = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let command = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let command = std::process::Command::new("xdg-open").arg(url).spawn();
    let _ = command;
}

pub async fn access_token(url: &str) -> Result<String, String> {
    token_for(url, false).await
}

/// A new access token from the refresh token, even when the stored one has not
/// expired yet. Use after the server rejected the stored token.
pub async fn refresh_access_token(url: &str) -> Result<String, String> {
    token_for(url, true).await
}

async fn token_for(url: &str, force_refresh: bool) -> Result<String, String> {
    let url = url.to_owned();
    let lock_url = url.clone();
    let _refresh_lock = tokio::task::spawn_blocking(move || credentials::lock_refresh(&lock_url))
        .await
        .map_err(|_| "Credential lock task failed")??;
    let (connection, tokens) = tokio::task::spawn_blocking(move || {
        Ok::<_, String>((
            credentials::connection(&url)?,
            credentials::load_tokens(&url)?,
        ))
    })
    .await
    .map_err(|_| "Credential lookup failed")??;
    current_or_renewed(
        &http_client()?,
        connection,
        tokens,
        force_refresh,
        credentials::save,
    )
    .await
}

/// Seconds before expiry at which a stored access token is renewed.
const EXPIRY_MARGIN: u64 = 30;

/// The stored access token while it is valid for more than `EXPIRY_MARGIN`
/// seconds (and no refresh is forced), otherwise a renewed one stored with `save`.
async fn current_or_renewed<F>(
    client: &reqwest::Client,
    connection: Connection,
    tokens: Tokens,
    force_refresh: bool,
    save: F,
) -> Result<String, String>
where
    F: FnOnce(Connection, &Tokens) -> Result<(), String> + Send + 'static,
{
    if !force_refresh && connection.expires_at > credentials::now() + EXPIRY_MARGIN {
        return Ok(tokens.access_token);
    }
    let refresh = tokens
        .refresh_token
        .ok_or("Sign-in expired; reconnect to Ax")?;
    renew(client, connection, refresh, save).await
}

/// Redeem `refresh`, store the new tokens with `save` and return the new access token.
///
/// The provider may rotate the refresh token, so the new tokens are stored as soon
/// as it answers, before the project is checked. Only a 401 or 403 from the MCP
/// server fails the renewal; when the server is briefly unavailable the new
/// access token is returned and the request that needs it reports the outage.
async fn renew<F>(
    client: &reqwest::Client,
    connection: Connection,
    refresh: String,
    save: F,
) -> Result<String, String>
where
    F: FnOnce(Connection, &Tokens) -> Result<(), String> + Send + 'static,
{
    let url = connection.url.clone();
    let (updated, tokens) = exchange_refresh(client, connection, refresh).await?;
    let token = tokens.access_token.clone();
    tokio::task::spawn_blocking(move || save(updated, &tokens))
        .await
        .map_err(|_| "Credential update failed")??;
    // A refresh may revoke a project's grant; report that instead of a renewal.
    match verify_connection(client, &url, &token).await {
        Err(Verify::Denied) => Err(Verify::Denied.into_message()),
        Ok(()) | Err(Verify::Unavailable(_)) => Ok(token),
    }
}

/// Removes the sign-in for `url` for every user and revokes its tokens.
pub async fn logout(url: &str) -> Result<(), String> {
    let url = url.to_owned();
    // Serialize with refresh so an in-flight refresh cannot recreate a disconnected login.
    let lock_url = url.clone();
    let _refresh_lock = tokio::task::spawn_blocking(move || credentials::lock_refresh(&lock_url))
        .await
        .map_err(|_| "Credential lock task failed")??;
    revoke_locked(url).await
}

/// Lets `user` stop using the sign-in for `url`. The tokens are removed and
/// revoked only when no other user (such as `ax mcp remote`, or an entry
/// whose users are unknown) still uses them.
pub async fn release(url: &str, user: &'static str) -> Result<(), String> {
    let url = url.to_owned();
    let lock_url = url.clone();
    let _refresh_lock = tokio::task::spawn_blocking(move || credentials::lock_refresh(&lock_url))
        .await
        .map_err(|_| "Credential lock task failed")??;
    let release_url = url.clone();
    let unused = tokio::task::spawn_blocking(move || credentials::release(&release_url, user))
        .await
        .map_err(|_| "Credential storage task failed")??;
    if unused {
        revoke_locked(url).await
    } else {
        Ok(())
    }
}

/// The body of `logout`; the caller holds the refresh lock of `url`.
async fn revoke_locked(url: String) -> Result<(), String> {
    let connection = credentials::connection(&url)?;
    let tokens = credentials::load_tokens(&url).ok();
    // Always remove local access even if remote revocation is unavailable.
    credentials::remove(&url)?;
    if let (Some(endpoint), Some(tokens)) = (connection.revocation_endpoint, tokens) {
        if !revoke_at_provider(&http_client()?, &endpoint, &connection.client_id, tokens).await {
            eprintln!(
                "Local connection removed; provider token revocation could not be confirmed."
            );
        }
    }
    Ok(())
}

/// Asks the provider to revoke the refresh token (or the access token when there
/// is none). True when the provider confirmed it.
async fn revoke_at_provider(
    client: &reqwest::Client,
    endpoint: &str,
    client_id: &str,
    tokens: Tokens,
) -> bool {
    let token = tokens.refresh_token.unwrap_or(tokens.access_token);
    client
        .post(endpoint)
        .form(&[("token", token.as_str()), ("client_id", client_id)])
        .send()
        .await
        .is_ok_and(|r| r.status().is_success())
}

/// Stdio bridge for IDEs that cannot connect to authenticated HTTP directly.
pub async fn proxy(url: String) -> Result<(), String> {
    https_url(&url)?;
    let client = http_client()?;
    proxy_io(
        &client,
        &url,
        &StoredTokens(&url),
        BufReader::new(tokio::io::stdin()),
        tokio::io::stdout(),
    )
    .await
}

/// Where the proxy gets its bearer token.
trait TokenSource {
    async fn access(&self) -> Result<String, String>;
    async fn refresh(&self) -> Result<String, String>;
}

/// The tokens stored for one remote address.
struct StoredTokens<'a>(&'a str);

impl TokenSource for StoredTokens<'_> {
    async fn access(&self) -> Result<String, String> {
        access_token(self.0).await
    }
    async fn refresh(&self) -> Result<String, String> {
        refresh_access_token(self.0).await
    }
}

/// The largest request line the proxy forwards.
const MAX_REQUEST_LINE: usize = 128 * 1024;
/// The largest reply the proxy reads from the server.
const MAX_REPLY: usize = 8 * 1024 * 1024;
/// How much of an HTTP error body the proxy reads for its message.
const MAX_ERROR_BODY: usize = 16 * 1024;

/// The MCP session the proxy holds with the server on behalf of the IDE.
struct ProxySession {
    id: Option<String>,
    version: String,
    /// The IDE's `initialize` request, replayed when the server lost the session.
    initialize: Option<Value>,
}

/// Why one request could not be answered by the server.
struct ProxyError {
    message: String,
    http_status: Option<u16>,
}

impl From<String> for ProxyError {
    fn from(message: String) -> Self {
        ProxyError {
            message,
            http_status: None,
        }
    }
}

fn rpc_error(id: Value, code: i64, message: &str, http_status: Option<u16>) -> Value {
    let mut error = json!({ "code": code, "message": message });
    if let Some(status) = http_status {
        error["data"] = json!({ "httpStatus": status });
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": error })
}

async fn write_line<W: tokio::io::AsyncWrite + Unpin>(
    output: &mut W,
    value: &Value,
) -> Result<(), String> {
    output
        .write_all(format!("{value}\n").as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    output.flush().await.map_err(|e| e.to_string())
}

/// The proxy loop: one JSON-RPC message per line from `reader`, one reply per
/// line to `writer`.
///
/// A request the server does not answer (an HTTP error status, an unreachable
/// server, a line that is not JSON) gets a JSON-RPC error with its id and the
/// loop goes on; a notification gets no reply. A 401 refreshes the token once
/// and retries; a 404 for a known session initializes a new session with the
/// IDE's own `initialize` request and retries.
async fn proxy_io<R, W, T>(
    client: &reqwest::Client,
    url: &str,
    tokens: &T,
    reader: R,
    mut output: W,
) -> Result<(), String>
where
    R: tokio::io::AsyncBufRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
    T: TokenSource,
{
    let mut input = reader.lines();
    let mut session = ProxySession {
        id: None,
        version: "2025-11-25".to_string(),
        initialize: None,
    };
    while let Some(line) = input.next_line().await.map_err(|e| e.to_string())? {
        if line.trim().is_empty() {
            continue;
        }
        if line.len() > MAX_REQUEST_LINE {
            let reply = rpc_error(Value::Null, -32600, "MCP request is too large", None);
            write_line(&mut output, &reply).await?;
            continue;
        }
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            let reply = rpc_error(Value::Null, -32700, "Invalid JSON-RPC input", None);
            write_line(&mut output, &reply).await?;
            continue;
        };
        match forward(client, url, tokens, &mut session, &request).await {
            Ok(Some(value)) => write_line(&mut output, &value).await?,
            Ok(None) => {}
            Err(error) => {
                // A notification has no id and gets no reply.
                if let Some(id) = request.get("id") {
                    let reply = rpc_error(id.clone(), -32000, &error.message, error.http_status);
                    write_line(&mut output, &reply).await?;
                }
            }
        }
    }
    if let Some(id) = session.id {
        if let Ok(token) = tokens.access().await {
            let _ = client
                .delete(url)
                .bearer_auth(token)
                .header("mcp-session-id", id)
                .send()
                .await;
        }
    }
    Ok(())
}

async fn send(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    session: &ProxySession,
    request: &Value,
) -> Result<reqwest::Response, ProxyError> {
    let mut builder = client
        .post(url)
        .bearer_auth(token)
        .header("accept", "application/json, text/event-stream")
        .json(request);
    if let Some(id) = &session.id {
        builder = builder
            .header("mcp-session-id", id)
            .header("mcp-protocol-version", &session.version);
    }
    builder
        .send()
        .await
        .map_err(|_| ProxyError::from("Cannot reach remote MCP server".to_string()))
}

/// Sends one request to the server and returns its reply (`None` for 202).
async fn forward<T: TokenSource>(
    client: &reqwest::Client,
    url: &str,
    tokens: &T,
    session: &mut ProxySession,
    request: &Value,
) -> Result<Option<Value>, ProxyError> {
    let is_initialize = request["method"] == "initialize";
    let mut token = tokens.access().await?;
    let mut refreshed = false;
    let mut reinitialized = false;
    loop {
        let response = send(client, url, &token, session, request).await?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED && !refreshed {
            refreshed = true;
            token = tokens.refresh().await?;
            continue;
        }
        if status == reqwest::StatusCode::NOT_FOUND
            && session.id.is_some()
            && !is_initialize
            && !reinitialized
        {
            if let Some(initialize) = session.initialize.clone() {
                reinitialized = true;
                session.id = None;
                reinitialize(client, url, &token, session, &initialize).await?;
                continue;
            }
        }
        if !status.is_success() {
            return Err(http_error(response).await);
        }
        if let Some(id) = response
            .headers()
            .get("mcp-session-id")
            .and_then(|h| h.to_str().ok())
        {
            session.id = Some(id.to_owned());
        }
        if status == reqwest::StatusCode::ACCEPTED {
            return Ok(None);
        }
        let value: Value = bounded_json_limit(response, MAX_REPLY)
            .await
            .map_err(|_| ProxyError::from("Invalid reply from remote MCP server".to_string()))?;
        if is_initialize {
            if let Some(v) = value
                .pointer("/result/protocolVersion")
                .and_then(Value::as_str)
            {
                session.version = v.to_owned();
            }
            session.initialize = Some(request.clone());
        }
        return Ok(Some(value));
    }
}

/// Starts a new server session with the IDE's earlier `initialize` request.
async fn reinitialize(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    session: &mut ProxySession,
    initialize: &Value,
) -> Result<(), ProxyError> {
    let response = send(client, url, token, session, initialize).await?;
    if !response.status().is_success() {
        return Err(http_error(response).await);
    }
    session.id = response
        .headers()
        .get("mcp-session-id")
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned);
    let value: Value = bounded_json_limit(response, MAX_REPLY)
        .await
        .map_err(|_| ProxyError::from("Invalid reply from remote MCP server".to_string()))?;
    if let Some(v) = value
        .pointer("/result/protocolVersion")
        .and_then(Value::as_str)
    {
        session.version = v.to_owned();
    }
    let initialized = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
    let _ = send(client, url, token, session, &initialized).await;
    Ok(())
}

/// The error for a non-2xx reply, with the server's own error text.
async fn http_error(mut response: reqwest::Response) -> ProxyError {
    let status = response.status();
    let mut body = Vec::new();
    while let Ok(Some(chunk)) = response.chunk().await {
        let room = MAX_ERROR_BODY.saturating_sub(body.len());
        body.extend_from_slice(&chunk[..chunk.len().min(room)]);
        if body.len() >= MAX_ERROR_BODY {
            break;
        }
    }
    let detail = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.as_str().or_else(|| e.get("message")?.as_str()))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| {
            ax_utils::truncate_chars(String::from_utf8_lossy(&body).trim(), 500, "...")
        });
    let message = if detail.is_empty() {
        format!("Remote MCP returned HTTP {}", status.as_u16())
    } else {
        format!("Remote MCP returned HTTP {}: {detail}", status.as_u16())
    };
    ProxyError {
        message,
        http_status: Some(status.as_u16()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn browser_callback_rejects_host_state_and_issuer_then_consumes_code_once() {
        let (sender, mut receiver) = oneshot::channel();
        let app = Router::new()
            .route("/callback", get(callback))
            .with_state(CallbackState {
                expected_state: "random-state".into(),
                issuer: "https://issuer.example/".into(),
                require_issuer: true,
                port: 1234,
                sender: Arc::new(Mutex::new(Some(sender))),
            });
        for (host, query) in [
            (
                "attacker.example",
                "state=random-state&iss=https%3A%2F%2Fissuer.example%2F&code=good",
            ),
            (
                "127.0.0.1:1234",
                "state=wrong&iss=https%3A%2F%2Fissuer.example%2F&code=good",
            ),
            (
                "127.0.0.1:1234",
                "state=random-state&iss=https%3A%2F%2Fwrong.example%2F&code=good",
            ),
            ("127.0.0.1:1234", "state=random-state&code=good"),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/callback?{query}"))
                        .header("host", host)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert!(!response.status().is_success());
            assert!(matches!(
                receiver.try_recv(),
                Err(oneshot::error::TryRecvError::Empty)
            ));
        }
        let request = || {
            Request::builder()
                .uri("/callback?state=random-state&iss=https%3A%2F%2Fissuer.example%2F&code=good")
                .header("host", "127.0.0.1:1234")
                .body(Body::empty())
                .unwrap()
        };
        assert_eq!(
            app.clone().oneshot(request()).await.unwrap().status(),
            StatusCode::OK
        );
        assert_eq!(receiver.await.unwrap().unwrap(), "good");
        assert_eq!(
            app.oneshot(request()).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn browser_denial_completes_with_an_error_instead_of_a_connection() {
        let (sender, receiver) = oneshot::channel();
        let app = Router::new()
            .route("/callback", get(callback))
            .with_state(CallbackState {
                expected_state: "state".into(),
                issuer: "https://issuer.example/".into(),
                require_issuer: false,
                port: 1234,
                sender: Arc::new(Mutex::new(Some(sender))),
            });
        let request = Request::builder()
            .uri("/callback?state=state&error=access_denied")
            .header("host", "127.0.0.1:1234")
            .body(Body::empty())
            .unwrap();
        assert_eq!(app.oneshot(request).await.unwrap().status(), StatusCode::OK);
        assert!(receiver.await.unwrap().unwrap_err().contains("denied"));
    }
    #[tokio::test]
    async fn cancelling_login_completes_callback_and_unknown_attempt_is_rejected() {
        let id = uuid::Uuid::new_v4().to_string();
        let (sender, receiver) = oneshot::channel();
        attempts().lock().await.insert(
            id.clone(),
            Attempt {
                status: LoginStatus {
                    id: id.clone(),
                    state: "pending".into(),
                    message: None,
                },
                started: std::time::Instant::now(),
                cancel: Arc::new(Mutex::new(Some(sender))),
            },
        );
        cancel_login(&id).await.unwrap();
        assert!(receiver.await.unwrap().unwrap_err().contains("cancelled"));
        assert!(cancel_login(&id).await.is_err());
        attempts().lock().await.remove(&id);
        assert!(cancel_login(&id).await.is_err());
    }

    #[test]
    fn secret_comparison_requires_exact_state() {
        assert!(same_secret("a", "a"));
        assert!(!same_secret("a", "b"));
        assert!(!same_secret("", "a"));
    }
    #[test]
    fn token_response_requires_short_lived_bearer() {
        let mut t = TokenResponse {
            access_token: "secret".into(),
            refresh_token: None,
            token_type: "Bearer".into(),
            expires_in: 3600,
        };
        assert!(t.validate().is_ok());
        t.token_type = "mac".into();
        assert!(t.validate().is_err());
    }
}

#[cfg(test)]
mod scope_tests {
    use super::login_scope;

    #[test]
    fn the_admin_scope_is_requested_only_on_demand_and_implies_write() {
        assert_eq!(login_scope(false, false), "ax:context offline_access");
        assert_eq!(
            login_scope(true, false),
            "ax:context ax:write offline_access"
        );
        assert_eq!(
            login_scope(true, true),
            "ax:context ax:write ax:admin offline_access"
        );
        assert_eq!(
            login_scope(false, true),
            "ax:context ax:write ax:admin offline_access"
        );
    }
}

#[cfg(test)]
mod exchange_tests {
    use super::*;
    use axum::{extract::Form, routing::post, Json};

    async fn token_endpoint(Form(form): Form<HashMap<String, String>>) -> Json<Value> {
        assert_eq!(
            form.get("resource").map(String::as_str),
            Some("https://ax.example.com")
        );
        assert_eq!(form.get("client_id").map(String::as_str), Some("native"));
        if form["grant_type"] == "authorization_code" {
            assert_eq!(form["code"], "one-use-code");
            assert_eq!(form["code_verifier"], "pkce-secret");
            assert_eq!(form["redirect_uri"], "http://127.0.0.1:1234/callback");
            Json(
                json!({"access_token":"access-one","refresh_token":"refresh-one","token_type":"Bearer","expires_in":600}),
            )
        } else {
            assert_eq!(form["refresh_token"], "refresh-one");
            Json(
                json!({"access_token":"access-two","refresh_token":"refresh-two","token_type":"Bearer","expires_in":600}),
            )
        }
    }
    async fn mcp(headers: HeaderMap) -> impl IntoResponse {
        if headers.get("authorization").and_then(|h| h.to_str().ok()) == Some("Bearer access-one")
            || headers.get("authorization").and_then(|h| h.to_str().ok())
                == Some("Bearer access-two")
        {
            (
                StatusCode::OK,
                Json(json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25"}})),
            )
        } else {
            (StatusCode::FORBIDDEN, Json(json!({"error":"denied"})))
        }
    }
    fn resource() -> ResourceMetadata {
        ResourceMetadata {
            resource: "https://ax.example.com".into(),
            authorization_servers: vec!["https://issuer.example/".into()],
        }
    }
    fn metadata(base: &str) -> OAuthMetadata {
        OAuthMetadata {
            issuer: "https://issuer.example/".into(),
            authorization_endpoint: format!("{base}/authorize"),
            token_endpoint: format!("{base}/token"),
            revocation_endpoint: None,
            code_challenge_methods_supported: vec!["S256".into()],
            authorization_response_iss_parameter_supported: true,
        }
    }
    #[tokio::test]
    async fn local_provider_exchange_checks_pkce_resource_project_access_and_rotating_refresh() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new()
            .route("/token", post(token_endpoint))
            .route("/projects/demo/mcp", post(mcp))
            .route(
                "/projects/denied/mcp",
                post(|| async { StatusCode::FORBIDDEN }),
            );
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = http_client().unwrap();
        let (connection, tokens) = exchange_login(
            &client,
            format!("{base}/projects/demo/mcp"),
            "native".into(),
            (resource(), metadata(&base)),
            "one-use-code",
            "http://127.0.0.1:1234/callback",
            "pkce-secret",
        )
        .await
        .unwrap();
        assert_eq!(tokens.access_token, "access-one");
        assert_eq!(tokens.refresh_token.as_deref(), Some("refresh-one"));
        let (_, refreshed) = exchange_refresh(&client, connection, "refresh-one".into())
            .await
            .unwrap();
        assert_eq!(refreshed.access_token, "access-two");
        assert_eq!(refreshed.refresh_token.as_deref(), Some("refresh-two"));
        let denied = exchange_login(
            &client,
            format!("{base}/projects/denied/mcp"),
            "native".into(),
            (resource(), metadata(&base)),
            "one-use-code",
            "http://127.0.0.1:1234/callback",
            "pkce-secret",
        )
        .await;
        assert!(denied.err().unwrap().contains("cannot access"));
        task.abort();
    }

    type Saved = Arc<std::sync::Mutex<Vec<(Connection, Option<String>)>>>;

    fn recorder() -> (
        Saved,
        impl FnOnce(Connection, &Tokens) -> Result<(), String> + Send + 'static,
    ) {
        let saved: Saved = Arc::default();
        let sink = saved.clone();
        (saved, move |connection: Connection, tokens: &Tokens| {
            sink.lock()
                .unwrap()
                .push((connection, tokens.refresh_token.clone()));
            Ok(())
        })
    }

    fn connection_to(base: &str, project: &str) -> Connection {
        Connection {
            url: format!("{base}/projects/{project}/mcp"),
            client_id: "native".into(),
            issuer: "https://issuer.example/".into(),
            resource: "https://ax.example.com".into(),
            token_endpoint: format!("{base}/token"),
            revocation_endpoint: None,
            expires_at: 1,
            users: vec![credentials::USER_AX.into()],
        }
    }

    async fn provider_with_mcp_status(status: StatusCode) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new().route("/token", post(token_endpoint)).route(
            "/projects/demo/mcp",
            post(move || async move { (status, Json(json!({"error":"busy"}))) }),
        );
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (base, task)
    }

    #[tokio::test]
    async fn a_rotated_refresh_token_is_kept_when_the_server_is_briefly_unavailable() {
        for status in [
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::GATEWAY_TIMEOUT,
        ] {
            let (base, task) = provider_with_mcp_status(status).await;
            let (saved, save) = recorder();
            let token = renew(
                &http_client().unwrap(),
                connection_to(&base, "demo"),
                "refresh-one".into(),
                save,
            )
            .await;
            assert_eq!(token.as_deref(), Ok("access-two"), "{status}");
            let saved = saved.lock().unwrap();
            assert_eq!(saved.len(), 1, "{status}");
            assert_eq!(saved[0].1.as_deref(), Some("refresh-two"));
            assert!(saved[0].0.expires_at > credentials::now());
            task.abort();
        }
    }

    #[tokio::test]
    async fn a_revoked_grant_is_reported_but_the_rotated_refresh_token_is_still_stored() {
        for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
            let (base, task) = provider_with_mcp_status(status).await;
            let (saved, save) = recorder();
            let result = renew(
                &http_client().unwrap(),
                connection_to(&base, "demo"),
                "refresh-one".into(),
                save,
            )
            .await;
            assert!(result.unwrap_err().contains("cannot access"), "{status}");
            assert_eq!(saved.lock().unwrap()[0].1.as_deref(), Some("refresh-two"));
            task.abort();
        }
    }
}

#[cfg(test)]
mod proxy_tests {
    use super::*;
    use axum::{routing::post, Json};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Server {
        session: std::sync::Mutex<Option<String>>,
        sessions_started: AtomicUsize,
        restarted: std::sync::atomic::AtomicBool,
        calls: std::sync::Mutex<Vec<String>>,
    }

    fn err(status: StatusCode, message: &str) -> axum::response::Response {
        (status, Json(json!({ "error": message }))).into_response()
    }

    async fn mcp(
        State(server): State<Arc<Server>>,
        headers: HeaderMap,
        Json(request): Json<Value>,
    ) -> axum::response::Response {
        let header = |name: &str| headers.get(name).and_then(|h| h.to_str().ok());
        if header("authorization") != Some("Bearer fresh") {
            return err(StatusCode::UNAUTHORIZED, "Authentication required");
        }
        let method = request["method"].as_str().unwrap_or_default().to_string();
        server.calls.lock().unwrap().push(method.clone());
        if method == "initialize" {
            let n = server.sessions_started.fetch_add(1, Ordering::SeqCst);
            let id = format!("session-{n}");
            *server.session.lock().unwrap() = Some(id.clone());
            let mut response = Json(json!({
                "jsonrpc": "2.0",
                "id": request["id"],
                "result": { "protocolVersion": "2025-06-18" }
            }))
            .into_response();
            response
                .headers_mut()
                .insert("mcp-session-id", id.parse().unwrap());
            return response;
        }
        let current = server.session.lock().unwrap().clone();
        if current.is_none() || header("mcp-session-id") != current.as_deref() {
            return err(StatusCode::NOT_FOUND, "Unknown or expired MCP session");
        }
        assert_eq!(header("mcp-protocol-version"), Some("2025-06-18"));
        if request.get("id").is_none() {
            return StatusCode::ACCEPTED.into_response();
        }
        match request.pointer("/params/name").and_then(Value::as_str) {
            Some("forbidden") => err(StatusCode::FORBIDDEN, "Tool is unavailable remotely"),
            Some("busy") => err(
                StatusCode::SERVICE_UNAVAILABLE,
                "Project is being re-indexed",
            ),
            Some("restart") if !server.restarted.swap(true, Ordering::SeqCst) => {
                *server.session.lock().unwrap() = None;
                err(StatusCode::NOT_FOUND, "Unknown or expired MCP session")
            }
            _ => Json(json!({"jsonrpc":"2.0","id":request["id"],"result":{"ok":true}}))
                .into_response(),
        }
    }

    struct FakeTokens {
        current: std::sync::Mutex<String>,
        refreshes: AtomicUsize,
    }

    impl FakeTokens {
        fn new(token: &str) -> Self {
            FakeTokens {
                current: std::sync::Mutex::new(token.into()),
                refreshes: AtomicUsize::new(0),
            }
        }
    }

    impl TokenSource for FakeTokens {
        async fn access(&self) -> Result<String, String> {
            Ok(self.current.lock().unwrap().clone())
        }
        async fn refresh(&self) -> Result<String, String> {
            self.refreshes.fetch_add(1, Ordering::SeqCst);
            *self.current.lock().unwrap() = "fresh".into();
            Ok("fresh".into())
        }
    }

    async fn serve() -> (String, Arc<Server>, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let url = format!(
            "http://{}/projects/demo/mcp",
            listener.local_addr().unwrap()
        );
        let server = Arc::new(Server::default());
        let app = Router::new()
            .route(
                "/projects/demo/mcp",
                post(mcp).delete(|| async { StatusCode::OK }),
            )
            .with_state(server.clone());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (url, server, task)
    }

    async fn run(url: &str, tokens: &FakeTokens, lines: &[String]) -> Vec<Value> {
        let input = lines.join("\n") + "\n";
        let mut output = Vec::new();
        proxy_io(
            &http_client().unwrap(),
            url,
            tokens,
            input.as_bytes(),
            &mut output,
        )
        .await
        .expect("the proxy keeps running until its input ends");
        String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn initialize(id: u64) -> String {
        json!({"jsonrpc":"2.0","id":id,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"ide","version":"1"}}}).to_string()
    }

    fn call(id: u64, name: &str) -> String {
        json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":{}}})
            .to_string()
    }

    #[tokio::test]
    async fn per_call_http_errors_become_json_rpc_errors_and_the_proxy_keeps_running() {
        let (url, _server, task) = serve().await;
        let tokens = FakeTokens::new("fresh");
        let replies = run(
            &url,
            &tokens,
            &[
                initialize(1),
                json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string(),
                "{not json".into(),
                call(2, "forbidden"),
                call(3, "busy"),
                format!("\"{}\"", "x".repeat(200 * 1024)),
                call(4, "ok"),
            ],
        )
        .await;
        assert_eq!(replies.len(), 6, "{replies:?}");
        assert_eq!(replies[0]["id"], 1);
        assert_eq!(replies[1]["id"], Value::Null);
        assert_eq!(replies[1]["error"]["code"], -32700);
        assert_eq!(replies[2]["id"], 2);
        assert!(replies[2]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Tool is unavailable remotely"));
        assert_eq!(replies[2]["error"]["data"]["httpStatus"], 403);
        assert_eq!(replies[3]["id"], 3);
        assert!(replies[3]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("re-indexed"));
        assert_eq!(replies[4]["id"], Value::Null);
        assert_eq!(replies[4]["error"]["code"], -32600);
        assert_eq!(replies[5]["id"], 4);
        assert_eq!(replies[5]["result"]["ok"], true);
        task.abort();
    }

    #[tokio::test]
    async fn a_rejected_token_is_refreshed_once_and_the_request_retried() {
        let (url, _server, task) = serve().await;
        let tokens = FakeTokens::new("stale");
        let replies = run(&url, &tokens, &[initialize(1), call(2, "ok")]).await;
        assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(replies[1]["result"]["ok"], true);
        assert_eq!(tokens.refreshes.load(Ordering::SeqCst), 1);
        task.abort();
    }

    #[tokio::test]
    async fn an_expired_session_is_initialized_again_and_the_request_retried() {
        let (url, server, task) = serve().await;
        let tokens = FakeTokens::new("fresh");
        let replies = run(&url, &tokens, &[initialize(1), call(2, "restart")]).await;
        assert_eq!(replies.len(), 2, "{replies:?}");
        assert_eq!(replies[1]["id"], 2);
        assert_eq!(replies[1]["result"]["ok"], true);
        assert_eq!(server.sessions_started.load(Ordering::SeqCst), 2);
        task.abort();
    }
}

#[cfg(test)]
mod discovery_tests {
    use super::*;
    use axum::{extract::Form, routing::post, Json};

    #[derive(Clone, Default)]
    struct Provider {
        base: String,
        wrong_resource: bool,
        wrong_issuer: bool,
        no_s256: bool,
        only_openid: bool,
    }

    impl Provider {
        fn issuer(&self) -> String {
            format!("{}/tenant", self.base)
        }
        fn metadata(&self) -> Value {
            let issuer = if self.wrong_issuer {
                "https://other.example/tenant".to_string()
            } else {
                self.issuer()
            };
            let methods = if self.no_s256 {
                vec!["plain"]
            } else {
                vec!["S256"]
            };
            json!({
                "issuer": issuer,
                "authorization_endpoint": format!("{}/authorize", self.base),
                "token_endpoint": format!("{}/token", self.base),
                "code_challenge_methods_supported": methods,
            })
        }
    }

    /// Accepts the loopback http URLs of the local test provider.
    fn any_url(value: &str) -> Result<Url, String> {
        Url::parse(value).map_err(|_| "Invalid URL".to_string())
    }

    async fn serve(provider: Provider) -> (Provider, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let provider = Provider {
            base: format!("http://{}", listener.local_addr().unwrap()),
            ..provider
        };
        let state = provider.clone();
        let app = Router::new()
            .route(
                "/.well-known/oauth-protected-resource",
                get(|State(p): State<Provider>| async move {
                    let resource = if p.wrong_resource {
                        "https://elsewhere.example".to_string()
                    } else {
                        p.base.clone()
                    };
                    Json(json!({"resource": resource, "authorization_servers": [p.issuer()]}))
                }),
            )
            .route(
                "/.well-known/oauth-authorization-server/tenant",
                get(|State(p): State<Provider>| async move {
                    if p.only_openid {
                        StatusCode::NOT_FOUND.into_response()
                    } else {
                        Json(p.metadata()).into_response()
                    }
                }),
            )
            .route(
                "/tenant/.well-known/openid-configuration",
                get(|State(p): State<Provider>| async move { Json(p.metadata()) }),
            )
            .with_state(state);
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (provider, task)
    }

    async fn discover_from(provider: Provider) -> Result<(String, String), String> {
        let (provider, task) = serve(provider).await;
        let url = format!("{}/projects/demo/mcp", provider.base);
        let result = discover_with(&http_client().unwrap(), &url, any_url).await;
        task.abort();
        result.map(|(resource, metadata)| (resource.resource, metadata.issuer))
    }

    #[tokio::test]
    async fn discovery_finds_the_issuer_through_oauth_metadata_or_the_openid_fallback() {
        for only_openid in [false, true] {
            let (resource, issuer) = discover_from(Provider {
                only_openid,
                ..Provider::default()
            })
            .await
            .unwrap();
            assert!(resource.starts_with("http://127.0.0.1:"));
            assert_eq!(issuer, format!("{resource}/tenant"));
        }
    }

    #[tokio::test]
    async fn discovery_rejects_a_foreign_resource_a_mismatched_issuer_and_no_pkce() {
        let cases = [
            (
                Provider {
                    wrong_resource: true,
                    ..Provider::default()
                },
                "does not match server origin",
            ),
            (
                Provider {
                    wrong_issuer: true,
                    ..Provider::default()
                },
                "does not match the advertised issuer",
            ),
            (
                Provider {
                    wrong_issuer: true,
                    only_openid: true,
                    ..Provider::default()
                },
                "does not match the advertised issuer",
            ),
            (
                Provider {
                    no_s256: true,
                    ..Provider::default()
                },
                "PKCE S256",
            ),
        ];
        for (provider, expected) in cases {
            let error = discover_from(provider).await.unwrap_err();
            assert!(error.contains(expected), "{error}");
        }
    }

    #[tokio::test]
    async fn discovery_requires_an_https_project_mcp_url() {
        let client = http_client().unwrap();
        for url in [
            "http://ax.example/projects/demo/mcp",
            "https://ax.example/other",
            "https://user:pw@ax.example/projects/demo/mcp",
        ] {
            assert!(
                discover_with(&client, url, https_url).await.is_err(),
                "{url}"
            );
        }
    }

    fn connection(token_endpoint: &str, expires_at: u64) -> Connection {
        Connection {
            url: "http://127.0.0.1:9/projects/demo/mcp".into(),
            client_id: "native".into(),
            issuer: "https://issuer.example/".into(),
            resource: "https://ax.example.com".into(),
            token_endpoint: token_endpoint.into(),
            revocation_endpoint: None,
            expires_at,
            users: Vec::new(),
        }
    }

    fn tokens(refresh: Option<&str>) -> Tokens {
        Tokens {
            access_token: "stored-access".into(),
            refresh_token: refresh.map(str::to_owned),
        }
    }

    fn never_saved(_: Connection, _: &Tokens) -> Result<(), String> {
        panic!("nothing is renewed")
    }

    #[tokio::test]
    async fn a_stored_token_is_used_until_thirty_seconds_before_it_expires() {
        let client = http_client().unwrap();
        // Port 9 (discard) is not served: any network use would fail.
        let unreachable = "http://127.0.0.1:9/token";
        let fresh = credentials::now() + EXPIRY_MARGIN + 60;
        let token = current_or_renewed(
            &client,
            connection(unreachable, fresh),
            tokens(Some("r")),
            false,
            never_saved,
        )
        .await;
        assert_eq!(token.as_deref(), Ok("stored-access"));
        for (expires_at, force) in [
            (credentials::now() + EXPIRY_MARGIN - 1, false),
            (fresh, true),
        ] {
            let error = current_or_renewed(
                &client,
                connection(unreachable, expires_at),
                tokens(Some("r")),
                force,
                never_saved,
            )
            .await
            .unwrap_err();
            assert!(error.contains("Cannot refresh"), "{error}");
        }
    }

    #[tokio::test]
    async fn an_expired_sign_in_without_a_refresh_token_asks_to_reconnect() {
        let error = current_or_renewed(
            &http_client().unwrap(),
            connection("http://127.0.0.1:9/token", 1),
            tokens(None),
            false,
            never_saved,
        )
        .await
        .unwrap_err();
        assert!(error.contains("reconnect"), "{error}");
    }

    #[tokio::test]
    async fn logout_revokes_the_refresh_token_or_else_the_access_token() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen: Arc<Mutex<Vec<HashMap<String, String>>>> = Arc::default();
        let sink = seen.clone();
        let app = Router::new()
            .route(
                "/revoke",
                post(move |Form(form): Form<HashMap<String, String>>| {
                    let sink = sink.clone();
                    async move {
                        sink.lock().await.push(form);
                        StatusCode::OK
                    }
                }),
            )
            .route("/broken", post(|| async { StatusCode::BAD_REQUEST }));
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = http_client().unwrap();
        let endpoint = format!("{base}/revoke");
        assert!(revoke_at_provider(&client, &endpoint, "native", tokens(Some("refresh"))).await);
        assert!(revoke_at_provider(&client, &endpoint, "native", tokens(None)).await);
        assert!(
            !revoke_at_provider(&client, &format!("{base}/broken"), "native", tokens(None)).await
        );
        let seen = seen.lock().await;
        assert_eq!(seen[0]["token"], "refresh");
        assert_eq!(seen[0]["client_id"], "native");
        assert_eq!(seen[1]["token"], "stored-access");
        task.abort();
    }
}
