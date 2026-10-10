use super::{
    config::https_url,
    credentials::{self, Connection, Tokens},
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
    let project_url = https_url(url)?;
    if !project_url.path().starts_with("/projects/") || !project_url.path().ends_with("/mcp") {
        return Err("Use the server's /projects/PROJECT/mcp URL".into());
    }
    let client = http_client()?;
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
        (
            "scope",
            if write {
                "ax:context ax:write offline_access"
            } else {
                "ax:context offline_access"
            },
        ),
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
    verify_connection(client, &url, &token.access_token).await?;
    Ok((
        Connection {
            url,
            client_id,
            issuer: metadata.issuer,
            resource: resource.resource,
            token_endpoint: metadata.token_endpoint,
            revocation_endpoint: metadata.revocation_endpoint,
            expires_at: credentials::now() + token.expires_in,
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
    // A refresh may revoke a project's grant; verify it before reporting renewal.
    verify_connection(client, &connection.url, &response.access_token).await?;
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

async fn verify_connection(client: &reqwest::Client, url: &str, token: &str) -> Result<(), String> {
    let response = client.post(url).bearer_auth(token).header("accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"ax-login","version":env!("CARGO_PKG_VERSION")}}}))
        .send().await.map_err(|_| "Cannot verify authenticated MCP access")?;
    if !response.status().is_success() {
        return Err(
            "OAuth succeeded but this account cannot access the selected Ax project".into(),
        );
    }
    let session = response
        .headers()
        .get("mcp-session-id")
        .and_then(|h| h.to_str().ok())
        .map(str::to_owned);
    let value: Value = bounded_json(response).await?;
    if value.get("result").is_none() {
        return Err("Remote MCP initialization failed".into());
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
    if connection.expires_at > credentials::now() + 30 {
        return Ok(tokens.access_token);
    }
    let refresh = tokens
        .refresh_token
        .ok_or("Sign-in expired; reconnect to Ax")?;
    let client = http_client()?;
    let (updated, tokens) = exchange_refresh(&client, connection, refresh).await?;
    let token = tokens.access_token.clone();
    tokio::task::spawn_blocking(move || credentials::save(updated, &tokens))
        .await
        .map_err(|_| "Credential update failed")??;
    Ok(token)
}

pub async fn logout(url: &str) -> Result<(), String> {
    let url = url.to_owned();
    // Serialize with refresh so an in-flight refresh cannot recreate a disconnected login.
    let lock_url = url.clone();
    let _refresh_lock = tokio::task::spawn_blocking(move || credentials::lock_refresh(&lock_url))
        .await
        .map_err(|_| "Credential lock task failed")??;
    let connection = credentials::connection(&url)?;
    let tokens = credentials::load_tokens(&url).ok();
    // Always remove local access even if remote revocation is unavailable.
    credentials::remove(&url)?;
    if let (Some(endpoint), Some(tokens)) = (connection.revocation_endpoint, tokens) {
        let token = tokens.refresh_token.unwrap_or(tokens.access_token);
        let response = http_client()?
            .post(endpoint)
            .form(&[("token", token), ("client_id", connection.client_id)])
            .send()
            .await;
        if !response.is_ok_and(|r| r.status().is_success()) {
            eprintln!(
                "Local connection removed; provider token revocation could not be confirmed."
            );
        }
    }
    Ok(())
}

/// Stdio bridge for IDEs that cannot connect to authenticated HTTP directly.
pub async fn proxy(url: String) -> Result<(), String> {
    https_url(&url)?;
    let client = http_client()?;
    let mut input = BufReader::new(tokio::io::stdin()).lines();
    let mut output = tokio::io::stdout();
    let mut session: Option<String> = None;
    let mut version = "2025-11-25".to_string();
    while let Some(line) = input.next_line().await.map_err(|e| e.to_string())? {
        if line.trim().is_empty() {
            continue;
        }
        if line.len() > 128 * 1024 {
            return Err("MCP request is too large".into());
        }
        let request: Value = serde_json::from_str(&line).map_err(|_| "Invalid JSON-RPC input")?;
        let token = access_token(&url).await?;
        let mut builder = client
            .post(&url)
            .bearer_auth(token)
            .header("accept", "application/json, text/event-stream")
            .json(&request);
        if let Some(id) = &session {
            builder = builder
                .header("mcp-session-id", id)
                .header("mcp-protocol-version", &version);
        }
        let response = builder
            .send()
            .await
            .map_err(|_| "Cannot reach remote MCP server")?;
        if !response.status().is_success() {
            return Err(format!(
                "Remote MCP returned HTTP {}; reconnect if the session expired",
                response.status()
            ));
        }
        if let Some(id) = response
            .headers()
            .get("mcp-session-id")
            .and_then(|h| h.to_str().ok())
        {
            session = Some(id.to_owned());
        }
        if response.status() == reqwest::StatusCode::ACCEPTED {
            continue;
        }
        let value: Value = bounded_json_limit(response, 8 * 1024 * 1024).await?;
        if request["method"] == "initialize" {
            if let Some(v) = value
                .pointer("/result/protocolVersion")
                .and_then(Value::as_str)
            {
                version = v.to_owned();
            }
        }
        output
            .write_all(format!("{value}\n").as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        output.flush().await.map_err(|e| e.to_string())?;
    }
    if let Some(session) = session {
        if let Ok(token) = access_token(&url).await {
            let _ = client
                .delete(&url)
                .bearer_auth(token)
                .header("mcp-session-id", session)
                .send()
                .await;
        }
    }
    Ok(())
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
}
