//! Shared MCP daemon — named pipe / Unix socket + TCP fallback (CG: mcp/daemon.ts).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use tokio::net::TcpListener;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;

use ax_context::directory::get_ax_dir;

use crate::daemon_conn::{connect_path, connect_tcp, DaemonSession};
use crate::daemon_lock::{
    clear_stale_daemon_lock, is_pid_alive, release_daemon_lock, rewrite_lock_socket_path,
    try_acquire_daemon_lock, AcquireResult,
};
use crate::daemon_paths::{daemon_pid_path, daemon_socket_candidates};
use crate::engine::McpEngine;
use crate::liveness_watchdog::install_main_thread_watchdog;
use crate::server::handle_request;
use crate::transport::{JsonRpcRequest, JsonRpcResponse, PARSE_ERROR};

pub const DAEMON_INFO_FILE: &str = "daemon.json";
pub const IDLE_TIMEOUT_ENV: &str = "AX_DAEMON_IDLE_TIMEOUT_MS";
pub const MAX_IDLE_ENV: &str = "AX_DAEMON_MAX_IDLE_MS";

const DEFAULT_IDLE_TIMEOUT_MS: u64 = 300_000;
const DEFAULT_MAX_IDLE_MS: u64 = 1_800_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonHello {
    #[serde(rename = "type")]
    pub kind: String,
    pub pid: u32,
    pub ax: String,
    pub project: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socket_path: Option<String>,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonInfo {
    pub pid: u32,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socket_path: Option<String>,
    pub version: String,
    pub project_root: String,
}

pub fn daemon_info_path(project_root: &Path) -> PathBuf {
    get_ax_dir(project_root).join(DAEMON_INFO_FILE)
}

pub fn write_daemon_info(
    project_root: &Path,
    port: u16,
    socket_path: Option<String>,
) -> std::io::Result<()> {
    let info = DaemonInfo {
        pid: std::process::id(),
        port,
        socket_path,
        version: env!("CARGO_PKG_VERSION").to_string(),
        project_root: project_root.to_string_lossy().replace('\\', "/"),
    };
    let content = serde_json::to_string_pretty(&info)?;
    std::fs::write(daemon_info_path(project_root), content)?;
    Ok(())
}

pub fn read_daemon_info(project_root: &Path) -> Option<DaemonInfo> {
    let content = std::fs::read_to_string(daemon_info_path(project_root)).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn remove_daemon_info(project_root: &Path) {
    let _ = std::fs::remove_file(daemon_info_path(project_root));
}

pub fn resolve_idle_timeout_ms() -> u64 {
    parse_env_ms(std::env::var(IDLE_TIMEOUT_ENV).ok(), DEFAULT_IDLE_TIMEOUT_MS)
}

pub fn resolve_max_idle_ms() -> u64 {
    parse_env_ms(std::env::var(MAX_IDLE_ENV).ok(), DEFAULT_MAX_IDLE_MS)
}

fn parse_env_ms(raw: Option<String>, default: u64) -> u64 {
    match raw {
        None => default,
        Some(s) if s.trim().is_empty() => default,
        Some(s) => {
            let parsed = s.parse::<f64>().unwrap_or(-1.0);
            if !parsed.is_finite() || parsed < 0.0 {
                default
            } else {
                parsed.floor() as u64
            }
        }
    }
}

pub struct DaemonLifecycle {
    clients: AtomicUsize,
    stopping: AtomicBool,
    last_activity: Mutex<Instant>,
    idle_timeout_ms: u64,
    max_idle_ms: u64,
    idle_task: RwLock<Option<JoinHandle<()>>>,
    project_root: PathBuf,
    pid_path: Option<PathBuf>,
}

impl DaemonLifecycle {
    pub fn new(project_root: PathBuf, pid_path: Option<PathBuf>) -> Arc<Self> {
        Arc::new(Self {
            clients: AtomicUsize::new(0),
            stopping: AtomicBool::new(false),
            last_activity: Mutex::new(Instant::now()),
            idle_timeout_ms: resolve_idle_timeout_ms(),
            max_idle_ms: resolve_max_idle_ms(),
            idle_task: RwLock::new(None),
            project_root,
            pid_path,
        })
    }

    pub fn client_count(&self) -> usize {
        self.clients.load(Ordering::SeqCst)
    }

    pub fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    pub async fn touch_activity(&self) {
        *self.last_activity.lock().await = Instant::now();
    }

    pub async fn on_client_connected(self: &Arc<Self>) {
        self.clients.fetch_add(1, Ordering::SeqCst);
        self.disarm_idle().await;
        self.touch_activity().await;
    }

    pub async fn on_client_disconnected(self: &Arc<Self>) {
        let prev = self.clients.fetch_sub(1, Ordering::SeqCst);
        if prev == 1 {
            self.arm_idle().await;
        }
    }

    pub async fn arm_idle(self: &Arc<Self>) {
        if self.is_stopping() || self.idle_timeout_ms == 0 {
            return;
        }
        self.disarm_idle().await;
        let me = Arc::clone(self);
        let timeout = self.idle_timeout_ms;
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(timeout)).await;
            if !me.is_stopping() && me.client_count() == 0 {
                me.shutdown("idle timeout").await;
                std::process::exit(0);
            }
        });
        *self.idle_task.write().await = Some(handle);
    }

    pub async fn disarm_idle(self: &Arc<Self>) {
        if let Some(handle) = self.idle_task.write().await.take() {
            handle.abort();
        }
    }

    pub fn spawn_watchers(self: &Arc<Self>) {
        let me = Arc::clone(self);
        if me.max_idle_ms > 0 {
            tokio::spawn(async move {
                let tick = Duration::from_millis(me.max_idle_ms.min(60_000));
                let mut interval = tokio::time::interval(tick);
                loop {
                    interval.tick().await;
                    if me.is_stopping() || me.client_count() == 0 {
                        continue;
                    }
                    let last = *me.last_activity.lock().await;
                    if last.elapsed() > Duration::from_millis(me.max_idle_ms) {
                        me.shutdown("inactivity backstop").await;
                        std::process::exit(0);
                    }
                }
            });
        }
    }

    pub async fn shutdown(self: &Arc<Self>, reason: &str) {
        if self.stopping.swap(true, Ordering::SeqCst) {
            return;
        }
        self.disarm_idle().await;
        tracing::info!(
            "ax daemon shutting down ({reason}; clients={})",
            self.client_count()
        );
        remove_daemon_info(&self.project_root);
        if let Some(pid_path) = &self.pid_path {
            release_daemon_lock(pid_path);
        }
    }
}

fn build_hello(project_root: &Path, port: u16, socket_path: Option<String>) -> DaemonHello {
    DaemonHello {
        kind: "hello".to_string(),
        pid: std::process::id(),
        ax: env!("CARGO_PKG_VERSION").to_string(),
        project: project_root.to_string_lossy().replace('\\', "/"),
        socket_path,
        port,
    }
}

async fn send_hello(
    session: &mut DaemonSession,
    project_root: &Path,
    port: u16,
    socket_path: Option<String>,
) -> std::io::Result<()> {
    let hello = build_hello(project_root, port, socket_path);
    let line = serde_json::to_string(&hello)? + "\n";
    session.write_bytes(line.as_bytes()).await?;
    Ok(())
}

fn acquire_daemon_lock_or_exit(project_root: &Path) -> PathBuf {
    const MAX_RETRIES: u32 = 3;
    const RETRY_MS: u64 = 200;
    for attempt in 0..MAX_RETRIES {
        clear_stale_daemon_lock(&daemon_pid_path(project_root), None);
        match try_acquire_daemon_lock(project_root) {
            Ok(AcquireResult::Acquired { pid_path, .. }) => return pid_path,
            Ok(AcquireResult::Taken { existing, pid_path }) => {
                if let Some(info) = existing {
                    if is_pid_alive(info.pid) {
                        tracing::info!(
                            "another ax daemon (pid {}) holds the lock; exiting",
                            info.pid
                        );
                        std::process::exit(0);
                    }
                    clear_stale_daemon_lock(&pid_path, Some(info.pid));
                } else {
                    clear_stale_daemon_lock(&pid_path, None);
                }
            }
            Err(e) => tracing::warn!("daemon lock acquire failed: {e}"),
        }
        if attempt + 1 < MAX_RETRIES {
            std::thread::sleep(std::time::Duration::from_millis(RETRY_MS));
        }
    }
    tracing::error!("could not acquire daemon lock; exiting");
    std::process::exit(0);
}

pub async fn run_daemon(project_root: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let pid_path = acquire_daemon_lock_or_exit(&project_root);
    if run_socket_daemon(project_root.clone(), pid_path.clone()).await.is_err() {
        tracing::warn!("socket daemon unavailable; falling back to TCP");
        run_tcp_daemon(project_root, pid_path).await?;
    }
    Ok(())
}

async fn run_socket_daemon(project_root: PathBuf, pid_path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    {
        return run_windows_pipe_daemon(project_root, pid_path).await;
    }
    #[cfg(unix)]
    {
        return run_unix_socket_daemon(project_root, pid_path).await;
    }
    #[cfg(not(any(windows, unix)))]
    {
        Err("no socket transport on this platform".into())
    }
}

#[cfg(windows)]
async fn run_windows_pipe_daemon(project_root: PathBuf, pid_path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::net::windows::named_pipe::{ServerOptions};

    let candidates = daemon_socket_candidates(&project_root);
    let pipe_name = candidates
        .first()
        .cloned()
        .ok_or("no pipe candidates")?;

    write_daemon_info(&project_root, 0, Some(pipe_name.clone()))?; let _ = rewrite_lock_socket_path(&pid_path, &pipe_name);
    let _liveness = install_main_thread_watchdog();
    tracing::info!(
        "ax daemon on pipe {} pid {} idle {}ms",
        pipe_name,
        std::process::id(),
        resolve_idle_timeout_ms()
    );

    let lifecycle = start_daemon_core(project_root.clone(), pid_path.clone()).await?;
    let engine = Arc::new(Mutex::new(McpEngine::with_project_root(project_root.clone())));

    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(&pipe_name)?;

    loop {
        if lifecycle.is_stopping() {
            break;
        }
        server.connect().await?;
        let connected = server;
        server = ServerOptions::new().create(&pipe_name)?;
        let engine = engine.clone();
        let root = project_root.clone();
        let lc = Arc::clone(&lifecycle);
        let sp = pipe_name.clone();
        tokio::spawn(async move {
            lc.on_client_connected().await;
            let session = DaemonSession::from_io(connected);
            let result =
                serve_session(session, engine, &root, 0, Some(sp), lc.clone()).await;
            lc.on_client_disconnected().await;
            if let Err(e) = result {
                tracing::warn!("daemon client: {}", e);
            }
        });
    }
    Ok(())
}

#[cfg(unix)]
async fn run_unix_socket_daemon(project_root: PathBuf, pid_path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::net::UnixListener;

    let candidates = daemon_socket_candidates(&project_root);
    let mut bound_path: Option<String> = None;
    let mut listener: Option<UnixListener> = None;

    for path in candidates {
        let _ = std::fs::remove_file(&path);
        match UnixListener::bind(&path) {
            Ok(l) => {
                bound_path = Some(path);
                listener = Some(l);
                break;
            }
            Err(e) => {
                tracing::warn!("socket bind failed for {path}: {e}");
            }
        }
    }

    let listener = listener.ok_or("no socket path could be bound")?;
    let socket_path = bound_path.unwrap_or_default();

    write_daemon_info(&project_root, 0, Some(socket_path.clone()))?; let _ = rewrite_lock_socket_path(&pid_path, &socket_path);
    let _liveness = install_main_thread_watchdog();
    tracing::info!(
        "ax daemon on {} pid {} idle {}ms",
        socket_path,
        std::process::id(),
        resolve_idle_timeout_ms()
    );

    let lifecycle = start_daemon_core(project_root.clone(), pid_path.clone()).await?;
    let engine = Arc::new(Mutex::new(McpEngine::with_project_root(project_root.clone())));

    loop {
        if lifecycle.is_stopping() {
            break;
        }
        let (stream, _) = listener.accept().await?;
        let engine = engine.clone();
        let root = project_root.clone();
        let lc = Arc::clone(&lifecycle);
        let sp = socket_path.clone();
        tokio::spawn(async move {
            lc.on_client_connected().await;
            let session = DaemonSession::from_unix(stream);
            let result = serve_session(session, engine, &root, 0, Some(sp), lc.clone()).await;
            lc.on_client_disconnected().await;
            if let Err(e) = result {
                tracing::warn!("daemon client: {}", e);
            }
        });
    }
    Ok(())
}

async fn run_tcp_daemon(project_root: PathBuf, pid_path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    write_daemon_info(&project_root, port, None)?;
    let _liveness = install_main_thread_watchdog();
    tracing::info!(
        "ax daemon on 127.0.0.1:{} pid {} idle {}ms",
        port,
        std::process::id(),
        resolve_idle_timeout_ms()
    );

    let lifecycle = start_daemon_core(project_root.clone(), pid_path.clone()).await?;
    let engine = Arc::new(Mutex::new(McpEngine::with_project_root(project_root.clone())));

    loop {
        if lifecycle.is_stopping() {
            break;
        }
        let (stream, _) = listener.accept().await?;
        let engine = engine.clone();
        let root = project_root.clone();
        let lc = Arc::clone(&lifecycle);
        tokio::spawn(async move {
            lc.on_client_connected().await;
            let session = DaemonSession::from_tcp(stream);
            let result = serve_session(session, engine, &root, port, None, lc.clone()).await;
            lc.on_client_disconnected().await;
            if let Err(e) = result {
                tracing::warn!("daemon client: {}", e);
            }
        });
    }
    Ok(())
}

async fn start_daemon_core(project_root: PathBuf, pid_path: PathBuf) -> Result<Arc<DaemonLifecycle>, Box<dyn std::error::Error>> {
    let lifecycle = DaemonLifecycle::new(project_root.clone(), Some(pid_path));
    lifecycle.spawn_watchers();
    lifecycle.arm_idle().await;

    let lc_signal = Arc::clone(&lifecycle);
    let root_signal = project_root.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            lc_signal.shutdown("SIGINT").await;
            remove_daemon_info(&root_signal);
            std::process::exit(0);
        }
    });

    Ok(lifecycle)
}

async fn serve_session(
    mut session: DaemonSession,
    engine: Arc<Mutex<McpEngine>>,
    project_root: &Path,
    port: u16,
    socket_path: Option<String>,
    lifecycle: Arc<DaemonLifecycle>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    send_hello(&mut session, project_root, port, socket_path).await?;
    let mut line = String::new();

    while session.read_line(&mut line).await? > 0 {
        lifecycle.touch_activity().await;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }
        let req: JsonRpcRequest = match serde_json::from_str(trimmed) {
            Ok(r) => r,
            Err(e) => {
                let err = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: None,
                    result: None,
                    error: Some(crate::transport::JsonRpcError {
                        code: PARSE_ERROR,
                        message: e.to_string(),
                        data: None,
                    }),
                };
                let out = serde_json::to_string(&err)? + "\n";
                session.write_bytes(out.as_bytes()).await?;
                line.clear();
                continue;
            }
        };

        let id = req.id.clone().unwrap_or(serde_json::Value::Null);
        let mut eng = engine.lock().await;
        let result = handle_request(
            &mut *eng,
            &req.method,
            req.params.unwrap_or(serde_json::Value::Null),
        )
        .await;
        let response = match result {
            Ok(value) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: Some(id),
                result: Some(value),
                error: None,
            },
            Err(msg) => JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: Some(id),
                result: None,
                error: Some(crate::transport::JsonRpcError {
                    code: crate::transport::METHOD_NOT_FOUND,
                    message: msg,
                    data: None,
                }),
            },
        };
        let out = serde_json::to_string(&response)? + "\n";
        session.write_bytes(out.as_bytes()).await?;
        line.clear();
    }
    Ok(())
}

pub async fn try_connect(project_root: &Path) -> Option<(DaemonSession, DaemonHello)> {
    let info = read_daemon_info(project_root)?;
    if info.version != env!("CARGO_PKG_VERSION") {
        return None;
    }

    let socket_candidates: Vec<String> = if let Some(path) = info.socket_path.clone() {
        vec![path]
    } else {
        daemon_socket_candidates(project_root)
    };

    for path in socket_candidates {
        if let Some(mut session) = connect_path(&path).await {
            if let Some(hello) = read_hello(&mut session).await {
                if hello.ax == env!("CARGO_PKG_VERSION") {
                    return Some((session, hello));
                }
            }
        }
    }

    if info.port > 0 {
        if let Some(mut session) = connect_tcp(info.port).await {
            if let Some(hello) = read_hello(&mut session).await {
                if hello.ax == env!("CARGO_PKG_VERSION") {
                    return Some((session, hello));
                }
            }
        }
    }

    None
}

async fn read_hello(session: &mut DaemonSession) -> Option<DaemonHello> {
    let mut hello_line = String::new();
    if session.read_line(&mut hello_line).await.is_err() {
        return None;
    }
    serde_json::from_str(hello_line.trim()).ok()
}

pub async fn wait_for_daemon(project_root: &Path, timeout_ms: u64) -> Option<DaemonInfo> {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    while tokio::time::Instant::now() < deadline {
        if read_daemon_info(project_root).is_some() && try_connect(project_root).await.is_some() {
            return read_daemon_info(project_root);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_idle_timeout_env() {
        assert_eq!(parse_env_ms(Some("0".to_string()), 100), 0);
        assert_eq!(parse_env_ms(Some("invalid".to_string()), 100), 100);
    }
}
