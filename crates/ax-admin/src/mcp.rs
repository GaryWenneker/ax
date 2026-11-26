//! MCP client for one remote Ax project (Streamable HTTP, JSON responses).
//!
//! The bearer credential is sent to the saved address only: redirects are never
//! followed, and every response is bounded in time and size.
use serde::Serialize;
use serde_json::{json, Value};
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use url::Url;

/// Newest first. The server answers with the version it will speak.
pub const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26"];
pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
/// The remote server gives a context tool 120 seconds.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(130);
/// Index, sync and policy import can run for minutes on a large project.
pub const ADMIN_CALL_TIMEOUT: Duration = Duration::from_secs(600);
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);
const SLOW_ADMIN_TOOLS: &[&str] = &["ax_index", "ax_sync", "ax_policy_index"];

pub fn call_timeout(tool: &str) -> Duration {
    if SLOW_ADMIN_TOOLS.contains(&tool) {
        ADMIN_CALL_TIMEOUT
    } else {
        CALL_TIMEOUT
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum McpError {
    /// No connection could be made (DNS, refused, TLS).
    Unreachable,
    Timeout,
    /// The server answered with a redirect. It is not followed.
    Redirect,
    /// HTTP 401: the credential is missing, wrong or expired.
    Rejected,
    /// HTTP 403: the credential is valid but may not do this.
    Forbidden(String),
    /// HTTP 404 on the project address.
    UnknownProject,
    /// The answer is not an MCP response (an HTML page, a proxy error).
    NotMcp(u16),
    TooLarge,
    Busy,
    /// A JSON-RPC error from the server.
    Rpc(i64, String),
    UnknownTool(String),
    /// The local credential could not be read or refreshed.
    Credential(String),
    Protocol(String),
}

impl McpError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unreachable => "unreachable",
            Self::Timeout => "timeout",
            Self::Redirect => "redirect",
            Self::Rejected => "credentials_rejected",
            Self::Forbidden(_) => "forbidden",
            Self::UnknownProject => "unknown_project",
            Self::NotMcp(_) => "not_mcp",
            Self::TooLarge => "response_too_large",
            Self::Busy => "server_busy",
            Self::Rpc(..) => "server_error",
            Self::UnknownTool(_) => "unknown_tool",
            Self::Credential(_) => "credential_unavailable",
            Self::Protocol(_) => "protocol_error",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Unreachable => "The server could not be reached. Check the address and your network.".into(),
            Self::Timeout => "The server did not answer in time.".into(),
            Self::Redirect => "The address redirects somewhere else. Enter the final address; credentials are not sent to a redirect.".into(),
            Self::Rejected => "The server rejected these credentials.".into(),
            Self::Forbidden(detail) => format!("Access denied by the server: {detail}"),
            Self::UnknownProject => "The server does not know this project. Check the project in the address.".into(),
            Self::NotMcp(status) => format!("The address did not answer as an Ax remote MCP server (HTTP {status})."),
            Self::TooLarge => "The server's answer is too large.".into(),
            Self::Busy => "The server is busy. Try again in a moment.".into(),
            Self::Rpc(_, message) => format!("The server reported an error: {message}"),
            Self::UnknownTool(name) => format!("The server does not offer {name} to this connection."),
            Self::Credential(detail) => detail.clone(),
            Self::Protocol(detail) => format!("Unexpected answer from the server: {detail}"),
        }
    }
}

pub type TokenFuture<'a> = Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>>;

/// Supplies the bearer credential for a connection.
pub trait TokenSource: Send + Sync {
    /// `after_rejection` is true on the single retry that follows an HTTP 401.
    fn token(&self, after_rejection: bool) -> TokenFuture<'_>;
    /// Whether a new token may exist after a rejection (OAuth), or not (API key).
    fn refreshable(&self) -> bool;
}

pub struct StaticToken(pub String);

impl TokenSource for StaticToken {
    fn token(&self, _after_rejection: bool) -> TokenFuture<'_> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
    fn refreshable(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub handshake: Duration,
    pub call: Duration,
    pub admin_call: Duration,
    pub max_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            handshake: HANDSHAKE_TIMEOUT,
            call: CALL_TIMEOUT,
            admin_call: ADMIN_CALL_TIMEOUT,
            max_bytes: MAX_RESPONSE_BYTES,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInfo {
    pub server_name: String,
    pub server_version: String,
    pub protocol_version: String,
    pub tools: Vec<Value>,
}

#[derive(Clone)]
struct Session {
    id: Option<String>,
    version: String,
    server_name: String,
    server_version: String,
}

pub struct Remote {
    url: Url,
    http: reqwest::Client,
    tokens: Arc<dyn TokenSource>,
    limits: Limits,
    session: Mutex<Option<Session>>,
    tools: Mutex<Option<Vec<Value>>>,
}

struct Reply {
    session: Option<String>,
    body: Option<Value>,
}

impl Remote {
    pub fn new(url: Url, tokens: Arc<dyn TokenSource>) -> Result<Self, McpError> {
        Self::with_limits(url, tokens, Limits::default())
    }

    pub fn with_limits(
        url: Url,
        tokens: Arc<dyn TokenSource>,
        limits: Limits,
    ) -> Result<Self, McpError> {
        let http = reqwest::Client::builder()
            // A redirect would carry the bearer credential to another address.
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(limits.handshake)
            .user_agent(concat!("ax-admin/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| McpError::Protocol("cannot start the HTTP client".into()))?;
        Ok(Self {
            url,
            http,
            tokens,
            limits,
            session: Mutex::new(None),
            tools: Mutex::new(None),
        })
    }

    pub fn url(&self) -> &Url {
        &self.url
    }

    async fn post(
        &self,
        token: &str,
        session: Option<&Session>,
        body: &Value,
        timeout: Duration,
    ) -> Result<Reply, McpError> {
        let mut request = self
            .http
            .post(self.url.clone())
            .timeout(timeout)
            .bearer_auth(token)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body(body.to_string());
        if let Some(session) = session {
            request = request.header("mcp-protocol-version", &session.version);
            if let Some(id) = &session.id {
                request = request.header("mcp-session-id", id);
            }
        }
        let mut response = request.send().await.map_err(|e| {
            if e.is_timeout() {
                McpError::Timeout
            } else {
                McpError::Unreachable
            }
        })?;
        let status = response.status();
        let new_session = response
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        if status.is_redirection() {
            return Err(McpError::Redirect);
        }
        if response
            .content_length()
            .is_some_and(|n| n > self.limits.max_bytes as u64)
        {
            return Err(McpError::TooLarge);
        }
        let mut bytes = Vec::new();
        loop {
            let chunk = response.chunk().await.map_err(|e| {
                if e.is_timeout() {
                    McpError::Timeout
                } else {
                    McpError::Unreachable
                }
            })?;
            let Some(chunk) = chunk else { break };
            if chunk.len() > self.limits.max_bytes.saturating_sub(bytes.len()) {
                return Err(McpError::TooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        let json: Option<Value> = serde_json::from_slice(&bytes).ok();
        let detail = || {
            json.as_ref()
                .and_then(|v| v.get("error"))
                .and_then(Value::as_str)
                .unwrap_or("no reason given")
                .to_owned()
        };
        match status.as_u16() {
            401 => return Err(McpError::Rejected),
            403 => return Err(McpError::Forbidden(detail())),
            // The Ax server answers 404 with a JSON error for an unknown project
            // or session. Any other 404 is some other web server.
            404 if json.as_ref().is_some_and(|v| v.get("error").is_some()) => {
                return Err(McpError::UnknownProject)
            }
            429 => return Err(McpError::Busy),
            202 | 204 => {
                return Ok(Reply {
                    session: new_session,
                    body: None,
                })
            }
            code if !status.is_success() => return Err(McpError::NotMcp(code)),
            _ => {}
        }
        let body = json.ok_or(McpError::NotMcp(status.as_u16()))?;
        if body.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Err(McpError::NotMcp(status.as_u16()));
        }
        Ok(Reply {
            session: new_session,
            body: Some(body),
        })
    }

    fn result(reply: Reply) -> Result<Value, McpError> {
        let body = reply
            .body
            .ok_or_else(|| McpError::Protocol("empty answer".into()))?;
        if let Some(error) = body.get("error") {
            return Err(McpError::Rpc(
                error.get("code").and_then(Value::as_i64).unwrap_or(0),
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown error")
                    .to_owned(),
            ));
        }
        body.get("result")
            .cloned()
            .ok_or_else(|| McpError::Protocol("answer has no result".into()))
    }

    async fn initialize(&self, token: &str) -> Result<Session, McpError> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL_VERSIONS[0],
                "capabilities": {},
                "clientInfo": {"name": "ax-admin", "version": env!("CARGO_PKG_VERSION")}
            }
        });
        let reply = self.post(token, None, &body, self.limits.handshake).await?;
        let id = reply.session.clone();
        let result = Self::result(reply)?;
        let version = result
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !PROTOCOL_VERSIONS.contains(&version) {
            return Err(McpError::Protocol(format!(
                "unsupported MCP protocol version {version:?}"
            )));
        }
        let field = |name: &str| {
            result
                .pointer(&format!("/serverInfo/{name}"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned()
        };
        let session = Session {
            id,
            version: version.to_owned(),
            server_name: field("name"),
            server_version: field("version"),
        };
        // The server acknowledges this notification with 202. A server that
        // ignores it still works, so only authentication problems are fatal.
        let ready = json!({"jsonrpc": "2.0", "method": "notifications/initialized"});
        if let Err(e @ (McpError::Rejected | McpError::Forbidden(_))) = self
            .post(token, Some(&session), &ready, self.limits.handshake)
            .await
        {
            return Err(e);
        }
        Ok(session)
    }

    async fn session(&self, token: &str) -> Result<Session, McpError> {
        let mut guard = self.session.lock().await;
        if let Some(session) = guard.as_ref() {
            return Ok(session.clone());
        }
        let session = self.initialize(token).await?;
        *guard = Some(session.clone());
        Ok(session)
    }

    async fn forget_session(&self) {
        *self.session.lock().await = None;
    }

    /// One JSON-RPC request. An expired transport session is re-opened once,
    /// and a rejected OAuth token is refreshed once.
    async fn rpc(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<(Value, Session), McpError> {
        let mut retried_session = false;
        let mut retried_token = false;
        loop {
            let token = self
                .tokens
                .token(retried_token)
                .await
                .map_err(McpError::Credential)?;
            let outcome = match self.session(&token).await {
                Ok(session) => {
                    let body = json!({"jsonrpc": "2.0", "id": 2, "method": method, "params": params});
                    self.post(&token, Some(&session), &body, timeout)
                        .await
                        .and_then(Self::result)
                        .map(|value| (value, session))
                }
                Err(e) => Err(e),
            };
            match outcome {
                Err(McpError::UnknownProject) if !retried_session => {
                    // With a session header, 404 means the session expired.
                    let had_session = self.session.lock().await.is_some();
                    if !had_session {
                        return Err(McpError::UnknownProject);
                    }
                    retried_session = true;
                    self.forget_session().await;
                }
                Err(McpError::Rejected) if !retried_token && self.tokens.refreshable() => {
                    retried_token = true;
                    self.forget_session().await;
                }
                other => return other,
            }
        }
    }

    /// Server identity and the tools it offers to this credential.
    pub async fn info(&self) -> Result<RemoteInfo, McpError> {
        let (result, session) = self
            .rpc("tools/list", json!({}), self.limits.handshake)
            .await?;
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| McpError::Protocol("tool list is missing".into()))?;
        *self.tools.lock().await = Some(tools.clone());
        Ok(RemoteInfo {
            server_name: session.server_name,
            server_version: session.server_version,
            protocol_version: session.version,
            tools,
        })
    }

    async fn offers(&self, name: &str) -> Result<bool, McpError> {
        if self.tools.lock().await.is_none() {
            self.info().await?;
        }
        Ok(self
            .tools
            .lock()
            .await
            .as_ref()
            .is_some_and(|tools| tools.iter().any(|tool| tool["name"] == name)))
    }

    /// Calls a tool the server listed. A tool it did not list is refused here,
    /// without a request to the server.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value, McpError> {
        if !self.offers(name).await? {
            return Err(McpError::UnknownTool(name.to_owned()));
        }
        let timeout = if SLOW_ADMIN_TOOLS.contains(&name) {
            self.limits.admin_call
        } else {
            self.limits.call
        };
        self.rpc(
            "tools/call",
            json!({"name": name, "arguments": arguments}),
            timeout,
        )
        .await
        .map(|(value, _)| value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_admin_tools_get_ten_minutes_and_other_tools_do_not() {
        assert_eq!(ADMIN_CALL_TIMEOUT, Duration::from_secs(600));
        for tool in ["ax_index", "ax_sync", "ax_policy_index"] {
            assert_eq!(call_timeout(tool), ADMIN_CALL_TIMEOUT, "{tool}");
        }
        for tool in ["ax_status", "ax_explore", "ax_logs", "ax_files", "ax_remember"] {
            assert_eq!(call_timeout(tool), CALL_TIMEOUT, "{tool}");
        }
        assert!(CALL_TIMEOUT < ADMIN_CALL_TIMEOUT);
    }

    #[test]
    fn every_error_has_a_distinct_code_and_a_message() {
        let errors = [
            McpError::Unreachable,
            McpError::Timeout,
            McpError::Redirect,
            McpError::Rejected,
            McpError::Forbidden("x".into()),
            McpError::UnknownProject,
            McpError::NotMcp(502),
            McpError::TooLarge,
            McpError::Busy,
            McpError::Rpc(-1, "x".into()),
            McpError::UnknownTool("ax_x".into()),
            McpError::Credential("x".into()),
            McpError::Protocol("x".into()),
        ];
        let codes: std::collections::HashSet<_> = errors.iter().map(McpError::code).collect();
        assert_eq!(codes.len(), errors.len());
        assert!(errors.iter().all(|e| !e.message().is_empty()));
        assert_eq!(McpError::Rejected.message(), "The server rejected these credentials.");
    }
}
