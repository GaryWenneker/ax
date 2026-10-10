use super::{
    auth::{Principal, TokenVerifier, CONTEXT_SCOPE, WRITE_SCOPE},
    config::{Project, RemoteConfig},
};
use crate::{engine::McpEngine, server::handle_request, transport::JsonRpcRequest};
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Semaphore};

const SESSION_TTL: Duration = Duration::from_secs(1800);
const MAX_SESSIONS: usize = 128;
const MAX_HANDLES: usize = 4096;
const VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26"];
const CONTEXT_TOOLS: &[&str] = &[
    "ax_preflight",
    "ax_rules",
    "ax_skill",
    "ax_guard",
    "ax_explore",
    "ax_search",
    "ax_node",
    "ax_callers",
    "ax_callees",
    "ax_impact",
    "ax_path",
    "ax_cycles",
    "ax_api",
    "ax_context",
    "ax_session",
    "ax_durable",
    "ax_affected",
    "ax_insights",
    "ax_report",
    "ax_status",
    "ax_recall",
    "ax_history",
    "ax_expand",
    "ax_stash",
    "ax_cache_status",
    "ax_policy_capture",
];
const WRITE_TOOLS: &[&str] = &["ax_remember"];

struct Session {
    principal: String,
    project: String,
    version: String,
    last_seen: Instant,
    engine: Arc<Mutex<SessionEngine>>,
}
struct SessionEngine {
    engine: McpEngine,
    chats: HashSet<String>,
    caches: HashSet<String>,
    current_chat: Option<String>,
}
#[derive(Clone)]
pub struct RemoteState {
    config: Arc<RemoteConfig>,
    verifier: TokenVerifier,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    concurrency: Arc<Semaphore>,
}

impl RemoteState {
    pub fn new(config: Arc<RemoteConfig>, verifier: TokenVerifier) -> Self {
        Self {
            config,
            verifier,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            concurrency: Arc::new(Semaphore::new(16)),
        }
    }
}

pub fn router(state: RemoteState) -> Router {
    Router::new()
        .route("/health", get(|| async { Json(json!({"ready":true})) }))
        .route("/.well-known/oauth-protected-resource", get(metadata))
        .route(
            "/projects/{project}/mcp",
            axum::routing::post(post).delete(delete),
        )
        .layer(DefaultBodyLimit::max(128 * 1024))
        .with_state(state)
}

async fn metadata(State(state): State<RemoteState>) -> Response {
    no_store(Json(json!({"resource":state.config.public_url, "authorization_servers":[state.config.oauth.issuer], "scopes_supported":[CONTEXT_SCOPE, WRITE_SCOPE], "bearer_methods_supported":["header"]})).into_response())
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    response
}
fn error(status: StatusCode, message: &str) -> Response {
    no_store((status, Json(json!({"error":message}))).into_response())
}
fn challenge(state: &RemoteState, status: StatusCode, scope: &str) -> Response {
    let mut response = error(
        status,
        if status == StatusCode::UNAUTHORIZED {
            "Authentication required"
        } else {
            "Access denied"
        },
    );
    let value = format!(
        "Bearer resource_metadata=\"{}\", scope=\"{}\", error=\"{}\"",
        state.config.metadata_url(),
        scope,
        if status == StatusCode::UNAUTHORIZED {
            "invalid_token"
        } else {
            "insufficient_scope"
        }
    );
    if let Ok(header) = HeaderValue::from_str(&value) {
        response.headers_mut().insert("www-authenticate", header);
    }
    response
}
fn rpc(id: Option<Value>, result: Result<Value, (&str, i32)>) -> Response {
    let body = match result {
        Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
        Err((message, code)) => {
            json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
        }
    };
    no_store(Json(body).into_response())
}

async fn authorize(
    state: &RemoteState,
    headers: &HeaderMap,
    project: &str,
) -> Result<(Principal, Project), Box<Response>> {
    if let Some(origin) = headers.get("origin") {
        let origin = origin
            .to_str()
            .map_err(|_| Box::new(error(StatusCode::FORBIDDEN, "Invalid origin")))?;
        if origin != state.config.public_url
            && !state
                .config
                .allowed_origins
                .iter()
                .any(|o| o.trim_end_matches('/') == origin)
        {
            return Err(Box::new(error(
                StatusCode::FORBIDDEN,
                "Origin is not allowed",
            )));
        }
    }
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(|| Box::new(challenge(state, StatusCode::UNAUTHORIZED, CONTEXT_SCOPE)))?;
    let principal = state
        .verifier
        .verify(token)
        .await
        .map_err(|_| Box::new(challenge(state, StatusCode::UNAUTHORIZED, CONTEXT_SCOPE)))?;
    let project = state
        .config
        .projects
        .iter()
        .find(|p| p.id == project)
        .ok_or_else(|| Box::new(error(StatusCode::NOT_FOUND, "Unknown project")))?
        .clone();
    if !principal.permits(&project) {
        return Err(Box::new(challenge(
            state,
            StatusCode::FORBIDDEN,
            CONTEXT_SCOPE,
        )));
    }
    Ok((principal, project))
}

pub fn tool_permitted(name: &str, args: &Value, write: bool) -> bool {
    if name == "ax_policy_capture" && args.get("action").and_then(Value::as_str) == Some("save") {
        return write;
    }
    CONTEXT_TOOLS.contains(&name) || (write && WRITE_TOOLS.contains(&name))
}

async fn post(
    State(state): State<RemoteState>,
    Path(project_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (principal, project) = match authorize(&state, &headers, &project_id).await {
        Ok(v) => v,
        Err(e) => return *e,
    };
    let _permit = match state.concurrency.clone().try_acquire_owned() {
        Ok(p) => p,
        Err(_) => return error(StatusCode::TOO_MANY_REQUESTS, "Server is busy"),
    };
    if headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .map(str::trim)
        != Some("application/json")
    {
        return error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Content-Type must be application/json",
        );
    }
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !accept.contains("application/json") || !accept.contains("text/event-stream") {
        return error(
            StatusCode::NOT_ACCEPTABLE,
            "Accept must include application/json and text/event-stream",
        );
    }
    let mut request: JsonRpcRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(_) => return rpc(None, Err(("Invalid JSON-RPC request", -32700))),
    };
    if request.jsonrpc != "2.0"
        || request
            .id
            .as_ref()
            .is_some_and(|id| !(id.is_string() || id.is_number() || id.is_null()))
    {
        return rpc(None, Err(("Invalid JSON-RPC request", -32600)));
    }
    if request.method == "initialize" {
        if request.id.is_none() {
            return error(StatusCode::BAD_REQUEST, "initialize needs an ID");
        }
        let params = request.params.clone().unwrap_or(Value::Null);
        let version = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .filter(|v| VERSIONS.contains(v))
            .unwrap_or(VERSIONS[0])
            .to_string();
        let mut engine = McpEngine::for_remote(project.root.clone());
        let outcome = handle_request(&mut engine, "initialize", params).await;
        let mut result = match outcome.result {
            Ok(r) => r,
            Err(_) => return error(StatusCode::INTERNAL_SERVER_ERROR, "Cannot initialize MCP"),
        };
        result["protocolVersion"] = json!(version);
        result["instructions"] = json!(format!("{}\nRemote project: {}. This server permits context tools and explicitly granted memory/policy writes only. Index, sync, ship, LSP and shell-running operations are unavailable remotely. Use the session returned by preflight; a new preflight without it starts a new chat.", result["instructions"].as_str().unwrap_or(""), project.id));
        let session_id = uuid::Uuid::new_v4().to_string();
        let mut sessions = state.sessions.lock().await;
        sessions.retain(|_, s| s.last_seen.elapsed() < SESSION_TTL);
        if sessions.len() >= MAX_SESSIONS {
            return error(
                StatusCode::TOO_MANY_REQUESTS,
                "Too many active MCP sessions",
            );
        }
        sessions.insert(
            session_id.clone(),
            Session {
                principal: principal.id,
                project: project.id,
                version: version.to_string(),
                last_seen: Instant::now(),
                engine: Arc::new(Mutex::new(SessionEngine {
                    engine,
                    chats: HashSet::new(),
                    caches: HashSet::new(),
                    current_chat: None,
                })),
            },
        );
        let mut response = rpc(request.id, Ok(result));
        response.headers_mut().insert(
            "mcp-session-id",
            HeaderValue::from_str(&session_id).expect("UUID is a valid header"),
        );
        return response;
    }
    let session_id = match headers.get("mcp-session-id").and_then(|v| v.to_str().ok()) {
        Some(id) => id,
        None => return error(StatusCode::BAD_REQUEST, "Mcp-Session-Id is required"),
    };
    let engine = {
        let mut sessions = state.sessions.lock().await;
        sessions.retain(|_, s| s.last_seen.elapsed() < SESSION_TTL);
        let session = match sessions.get_mut(session_id) {
            Some(s) => s,
            None => return error(StatusCode::NOT_FOUND, "Unknown or expired MCP session"),
        };
        if session.principal != principal.id || session.project != project.id {
            return error(
                StatusCode::FORBIDDEN,
                "Session belongs to another connection",
            );
        }
        if headers
            .get("mcp-protocol-version")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("2025-03-26")
            != session.version
        {
            return error(
                StatusCode::BAD_REQUEST,
                "MCP protocol version does not match the initialized session",
            );
        }
        session.last_seen = Instant::now();
        session.engine.clone()
    };
    if request.id.is_none() {
        return if request.method == "notifications/initialized" {
            no_store(StatusCode::ACCEPTED.into_response())
        } else {
            error(StatusCode::BAD_REQUEST, "Unsupported notification")
        };
    }
    if request.method == "ping" {
        return rpc(request.id, Ok(json!({})));
    }
    if !matches!(request.method.as_str(), "tools/list" | "tools/call") {
        return rpc(request.id, Err(("Method not found", -32601)));
    }
    let write = principal.can_write(&project);
    let mut engine = match tokio::time::timeout(Duration::from_secs(30), engine.lock()).await {
        Ok(e) => e,
        Err(_) => return error(StatusCode::TOO_MANY_REQUESTS, "Connection is busy"),
    };
    if request.method == "tools/call" {
        if engine.chats.len() >= MAX_HANDLES || engine.caches.len() >= MAX_HANDLES {
            return error(
                StatusCode::TOO_MANY_REQUESTS,
                "Reconnect to reset the connection handle limit",
            );
        }
        let params = request.params.get_or_insert_with(|| json!({}));
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !tool_permitted(&name, &args, write) {
            if WRITE_TOOLS.contains(&name.as_str()) || name == "ax_policy_capture" {
                let auth = format!("Bearer resource_metadata=\"{}\", scope=\"{} {}\", error=\"insufficient_scope\"",
                    state.config.metadata_url(), CONTEXT_SCOPE, WRITE_SCOPE);
                return rpc(
                    request.id,
                    Ok(json!({
                        "content":[{"type":"text","text":"This tool needs ax:write and an explicit project write grant."}],
                        "isError":true,
                        "_meta":{"mcp/www_authenticate":[auth]}
                    })),
                );
            }
            return error(StatusCode::FORBIDDEN, "Tool is unavailable remotely");
        }
        if name == "ax_policy_capture" && args.get("action").and_then(Value::as_str) == Some("save")
        {
            let fm = &args["rule"]["frontmatter"];
            if fm
                .get("scope")
                .and_then(Value::as_str)
                .is_some_and(|scope| scope != "project")
                || fm.get("rootId").is_some_and(|id| !id.is_null())
                || fm.get("root_id").is_some_and(|id| !id.is_null())
            {
                return error(
                    StatusCode::FORBIDDEN,
                    "Remote policy writes must remain project-local",
                );
            }
        }
        if !args.is_object() {
            return rpc(
                request.id,
                Err(("Tool arguments must be an object", -32602)),
            );
        }
        if args.get("projectPath").is_some()
            || args.get("project_path").is_some()
            || args.get("__axSession").is_some()
        {
            return rpc(
                request.id,
                Err((
                    "Project and policy session are selected by the authenticated connection",
                    -32602,
                )),
            );
        }
        // Validate every public identity before normalization. Invalid IDs must not
        // silently turn into a newly minted chat or overwrite the selected project.
        for key in [
            "session",
            "sessionId",
            "conversation",
            "conversationId",
            "chatId",
        ] {
            if let Some(value) = args.get(key) {
                if ax_usage::session_from_args(&json!({"session":value})).is_none() {
                    return rpc(
                        request.id,
                        Err(("Invalid explicit chat identifier", -32602)),
                    );
                }
            }
        }
        if crate::request_context::validate(&project.root, &args).is_err() {
            return rpc(request.id, Err(("Invalid tool context arguments", -32602)));
        }
        let supplied_ids: HashSet<_> = [
            "session",
            "sessionId",
            "conversation",
            "conversationId",
            "chatId",
        ]
        .into_iter()
        .filter_map(|key| args.get(key).and_then(Value::as_str))
        .collect();
        if supplied_ids.len() > 1 {
            return rpc(request.id, Err(("Conflicting chat identifiers", -32602)));
        }
        let mut args = crate::request_context::public_args(args);
        // Local windows cannot influence a remote conversation's identity.
        args.as_object_mut()
            .expect("object checked")
            .remove("window_id");
        if name == "ax_expand" {
            let id = args.get("id").and_then(Value::as_str).unwrap_or("");
            if !engine.caches.contains(id) {
                return error(StatusCode::FORBIDDEN, "Cache belongs to another connection");
            }
        }
        let supplied = crate::remote::server::chat_argument(&args);
        let chat = if let Some(id) = supplied {
            if !engine.chats.contains(&id) {
                return error(StatusCode::FORBIDDEN, "Chat belongs to another connection");
            }
            id
        } else if name == "ax_preflight" || engine.current_chat.is_none() {
            let id = crate::chat_session::mint_session();
            engine.chats.insert(id.clone());
            id
        } else {
            engine.current_chat.clone().expect("chat checked")
        };
        for key in [
            "session",
            "sessionId",
            "conversation",
            "conversationId",
            "chatId",
        ] {
            args.as_object_mut().expect("object checked").remove(key);
        }
        args["session"] = json!(chat);
        engine.current_chat = Some(chat);
        params["arguments"] = args;
    }
    let durable_branch = request.params.as_ref().and_then(|params| {
        if params["name"] == "ax_durable" {
            params
                .pointer("/arguments/action")
                .and_then(Value::as_str)
                .filter(|action| matches!(*action, "fork" | "handoff"))
                .map(str::to_owned)
        } else {
            None
        }
    });
    let outcome = match tokio::time::timeout(
        Duration::from_secs(120),
        handle_request(
            &mut engine.engine,
            &request.method,
            request.params.unwrap_or(Value::Null),
        ),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(_) => return error(StatusCode::GATEWAY_TIMEOUT, "Ax tool timed out"),
    };
    let mut value = match outcome.result {
        Ok(v) => v,
        Err(_) => return rpc(request.id, Err(("Ax request failed", -32603))),
    };
    if request.method == "tools/list" {
        if let Some(tools) = value.get_mut("tools").and_then(Value::as_array_mut) {
            tools.retain(|t| tool_permitted(t["name"].as_str().unwrap_or(""), &Value::Null, write));
            for tool in tools {
                let name = tool["name"].as_str().unwrap_or("").to_owned();
                let requires_write =
                    WRITE_TOOLS.contains(&name.as_str()) || (write && name == "ax_policy_capture");
                let policy_capture = name == "ax_policy_capture";
                let scopes = if requires_write {
                    vec![CONTEXT_SCOPE, WRITE_SCOPE]
                } else {
                    vec![CONTEXT_SCOPE]
                };
                tool["securitySchemes"] = json!([{"type":"oauth2", "scopes":scopes}]);
                tool["_meta"]["securitySchemes"] = tool["securitySchemes"].clone();
                let read_only = !requires_write
                    && !matches!(
                        name.as_str(),
                        "ax_session" | "ax_durable" | "ax_stash" | "ax_policy_capture"
                    );
                tool["annotations"] =
                    json!({"readOnlyHint":read_only,"destructiveHint":false,"openWorldHint":false});
                if let Some(props) = tool
                    .pointer_mut("/inputSchema/properties")
                    .and_then(Value::as_object_mut)
                {
                    props.remove("projectPath");
                    props.remove("project_path");
                    props.remove("window_id");
                    if !write && policy_capture {
                        // Discovery reflects the same action-level write ACL as invocation.
                        if let Some(actions) = props
                            .get_mut("action")
                            .and_then(|a| a.get_mut("enum"))
                            .and_then(Value::as_array_mut)
                        {
                            actions.retain(|action| action != "save");
                        }
                    }
                    props.insert("session".into(), json!({"type":"string","description":"Server-issued chat ID returned by preflight; omit on a new chat"}));
                }
            }
        }
    }
    if value.get("isError").and_then(Value::as_bool) != Some(true) {
        if let Some(action) = durable_branch {
            let prefix = format!("<ax_durable_{action} parent=");
            let child = value
                .pointer("/content/0/text")
                .and_then(Value::as_str)
                .filter(|text| text.starts_with(&prefix))
                .and_then(|text| text.lines().next())
                .and_then(|header| header.split(" child=").nth(1))
                .map(|rest| rest.split([' ', '>']).next().unwrap_or(""))
                .filter(|id| ax_usage::session_from_args(&json!({"session":id})).is_some())
                .map(str::to_owned);
            if let Some(child) = child {
                // The durable handler mints this ID; caller-supplied transcript text
                // can never register a foreign conversation.
                value["structuredContent"]["session"] = json!(child);
            }
        }
    }
    if let Some(chat) = value
        .pointer("/structuredContent/session")
        .and_then(Value::as_str)
    {
        // Only server-produced fork/handoff IDs become owned by this connection.
        engine.chats.insert(chat.to_owned());
    }
    if value.get("isError").and_then(Value::as_bool) != Some(true) {
        for pointer in [
            "/structuredContent/contextCacheId",
            "/structuredContent/contextCacheHit",
            "/structuredContent/id",
        ] {
            if let Some(id) = value.pointer(pointer).and_then(Value::as_str) {
                if id.starts_with("cc_") {
                    engine.caches.insert(id.to_owned());
                }
            }
        }
    }
    rpc(request.id, Ok(value))
}

fn chat_argument(args: &Value) -> Option<String> {
    ax_usage::session_from_args(args)
}

async fn delete(
    State(state): State<RemoteState>,
    Path(project_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let (principal, project) = match authorize(&state, &headers, &project_id).await {
        Ok(v) => v,
        Err(e) => return *e,
    };
    let id = headers
        .get("mcp-session-id")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let mut sessions = state.sessions.lock().await;
    let Some(session) = sessions.get(id) else {
        return error(StatusCode::NOT_FOUND, "Unknown MCP session");
    };
    if session.principal != principal.id || session.project != project.id {
        return error(
            StatusCode::FORBIDDEN,
            "Session belongs to another connection",
        );
    }
    sessions.remove(id);
    no_store(StatusCode::NO_CONTENT.into_response())
}

pub async fn bind(port: u16) -> Result<tokio::net::TcpListener, String> {
    tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| format!("Cannot bind remote MCP listener: {e}"))
}

pub async fn serve(
    mut config: RemoteConfig,
    port: u16,
    socket: Option<PathBuf>,
) -> Result<(), String> {
    config.validate()?;
    let config = Arc::new(config);
    let verifier = TokenVerifier::discover(config.clone()).await?;
    let listener = bind(port).await?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    let bridge = start_socket_bridge(socket.as_deref(), addr).await?;
    println!(
        "{}",
        json!({"port":addr.port(),"local_url":format!("http://{addr}"),"public_url":config.public_url,"socket":socket,"projects":config.projects.iter().map(|p| json!({"id":p.id,"url":format!("{}/projects/{}/mcp",config.public_url,p.id)})).collect::<Vec<_>>() })
    );
    let result = axum::serve(listener, router(RemoteState::new(config, verifier)))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|e| e.to_string());
    if let Some(task) = bridge {
        task.abort();
    }
    if let Some(path) = socket {
        let _ = tokio::fs::remove_file(path).await;
    }
    result
}

#[cfg(unix)]
async fn start_socket_bridge(
    path: Option<&std::path::Path>,
    addr: std::net::SocketAddr,
) -> Result<Option<tokio::task::JoinHandle<()>>, String> {
    use std::os::unix::fs::PermissionsExt;
    let Some(path) = path else { return Ok(None) };
    // Never remove somebody else's socket or an existing live listener.
    if path.exists() {
        return Err(
            "Socket path already exists; stop its owner or remove a stale socket explicitly".into(),
        );
    }
    let listener = tokio::net::UnixListener::bind(path).map_err(|e| e.to_string())?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;
    Ok(Some(tokio::spawn(async move {
        let limit = Arc::new(Semaphore::new(32));
        while let Ok((mut stream, _)) = listener.accept().await {
            let Ok(permit) = limit.clone().try_acquire_owned() else {
                continue;
            };
            tokio::spawn(async move {
                let _permit = permit;
                if let Ok(mut tcp) = tokio::net::TcpStream::connect(addr).await {
                    let _ = tokio::time::timeout(
                        Duration::from_secs(300),
                        tokio::io::copy_bidirectional(&mut stream, &mut tcp),
                    )
                    .await;
                }
            });
        }
    })))
}
#[cfg(not(unix))]
async fn start_socket_bridge(
    path: Option<&std::path::Path>,
    _addr: std::net::SocketAddr,
) -> Result<Option<tokio::task::JoinHandle<()>>, String> {
    if path.is_some() {
        Err("Unix socket upstream is available on macOS/Linux; use TCP on Windows".into())
    } else {
        Ok(None)
    }
}

#[cfg(all(test, unix))]
mod socket_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn remote_socket_refuses_existing_paths_without_removing_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("upstream.sock");
        std::fs::write(&path, "existing owner's file").unwrap();
        assert!(
            start_socket_bridge(Some(&path), "127.0.0.1:1".parse().unwrap())
                .await
                .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "existing owner's file"
        );
    }

    #[tokio::test]
    async fn remote_socket_forwards_to_actual_dynamic_port() {
        // Managed local runtimes can forbid AF_UNIX. CI enables this explicitly
        // on macOS/Linux, where a failure must fail the job.
        if std::env::var("AX_TEST_UNIX_SOCKETS").as_deref() != Ok("1") {
            eprintln!("Unix socket forwarding check requires AX_TEST_UNIX_SOCKETS=1");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("upstream.sock");
        let listener = bind(0).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let upstream = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route("/health", get(|| async { "ready" })),
            )
            .await
            .unwrap();
        });
        let bridge = start_socket_bridge(Some(&path), addr)
            .await
            .unwrap()
            .unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let mut socket = tokio::net::UnixStream::connect(&path).await.unwrap();
        socket
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        let mut response = String::new();
        tokio::time::timeout(Duration::from_secs(5), socket.read_to_string(&mut response))
            .await
            .unwrap()
            .unwrap();
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.ends_with("ready"), "{response}");
        bridge.abort();
        upstream.abort();
    }
}
