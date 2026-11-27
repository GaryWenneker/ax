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
    clear_stale_daemon_lock, is_daemon_pid, release_daemon_lock_if_owned, rewrite_lock_socket_path,
    try_acquire_daemon_lock, AcquireResult,
};
use crate::daemon_paths::{daemon_pid_path, daemon_socket_candidates};
use crate::engine::McpEngine;
use crate::exe_identity::ExeIdentity;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe: Option<ExeIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonInfo {
    pub pid: u32,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub socket_path: Option<String>,
    pub version: String,
    pub project_root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe: Option<ExeIdentity>,
    /// Secret a TCP client sends as its first line. Only TCP daemons have one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// When the daemon wrote this file, in milliseconds since the epoch. A process
    /// with the same pid that started later is not this daemon.
    #[serde(default, rename = "startedAt", skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
}

/// The binary this daemon was started from, taken once at startup.
fn daemon_exe() -> Option<ExeIdentity> {
    static EXE: std::sync::OnceLock<Option<ExeIdentity>> = std::sync::OnceLock::new();
    EXE.get_or_init(ExeIdentity::current).clone()
}

pub fn daemon_info_path(project_root: &Path) -> PathBuf {
    get_ax_dir(project_root).join(DAEMON_INFO_FILE)
}

pub fn write_daemon_info(
    project_root: &Path,
    port: u16,
    socket_path: Option<String>,
    token: Option<String>,
) -> std::io::Result<()> {
    let info = DaemonInfo {
        pid: std::process::id(),
        port,
        socket_path,
        version: env!("CARGO_PKG_VERSION").to_string(),
        project_root: project_root.to_string_lossy().replace('\\', "/"),
        exe: daemon_exe(),
        token,
        started_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_millis() as u64),
    };
    let content = serde_json::to_string_pretty(&info)?;
    write_private(&daemon_info_path(project_root), content.as_bytes())
}

/// Writes `path` readable by its owner only (it may hold the TCP token):
/// a temporary file created with that mode is renamed over it.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        #[cfg(windows)]
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// A new random secret for a TCP daemon.
fn new_daemon_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// Longest first line a TCP client may send before it is authenticated.
const TOKEN_LINE_LIMIT: u64 = 256;
/// How long a TCP client has to send its token.
const TOKEN_TIMEOUT: Duration = Duration::from_secs(5);

/// Reads the first line of a TCP session and checks it against `token`.
async fn token_matches(session: &mut DaemonSession, token: &str) -> bool {
    let mut line = String::new();
    let read = tokio::time::timeout(
        TOKEN_TIMEOUT,
        session.read_line_limited(&mut line, TOKEN_LINE_LIMIT),
    )
    .await;
    if !matches!(read, Ok(Ok(n)) if n > 0) {
        return false;
    }
    let given = line.trim_end_matches(['\r', '\n']).as_bytes();
    let expected = token.as_bytes();
    // Compares every byte, so the time taken does not reveal a prefix.
    given.len() == expected.len()
        && given
            .iter()
            .zip(expected)
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

pub fn read_daemon_info(project_root: &Path) -> Option<DaemonInfo> {
    let content = std::fs::read_to_string(daemon_info_path(project_root)).ok()?;
    serde_json::from_str(&content).ok()
}

/// Removes `daemon.json` whatever process wrote it (`ax daemon stop`).
pub fn remove_daemon_info(project_root: &Path) {
    let _ = std::fs::remove_file(daemon_info_path(project_root));
}

/// Removes `daemon.json` only when it describes the daemon with `pid`.
pub fn remove_daemon_info_if_owned(project_root: &Path, pid: u32) {
    if read_daemon_info(project_root).is_some_and(|info| info.pid == pid) {
        remove_daemon_info(project_root);
    }
}

pub fn resolve_idle_timeout_ms() -> u64 {
    parse_env_ms(
        std::env::var(IDLE_TIMEOUT_ENV).ok(),
        DEFAULT_IDLE_TIMEOUT_MS,
    )
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
        self.client_accepted();
        self.client_started().await;
    }

    /// Counts a client the moment it is accepted, so an idle timer that fires
    /// before the client's task runs sees it.
    fn client_accepted(&self) {
        self.clients.fetch_add(1, Ordering::SeqCst);
    }

    async fn client_started(self: &Arc<Self>) {
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
            if me.idle_timer_fired().await {
                std::process::exit(0);
            }
        });
        *self.idle_task.write().await = Some(handle);
    }

    /// Runs on the idle task when its timer ends. Returns true when it shut down.
    async fn idle_timer_fired(self: &Arc<Self>) -> bool {
        // This is the idle task: take its handle out without aborting it, so the
        // shutdown below does not cancel the task that runs it.
        drop(self.idle_task.write().await.take());
        self.idle_expired().await
    }

    /// The idle timer fired: shuts the daemon down when no client is connected.
    /// Returns true when it shut down.
    async fn idle_expired(self: &Arc<Self>) -> bool {
        if self.is_stopping() || self.client_count() > 0 {
            return false;
        }
        self.shutdown("idle timeout").await;
        true
    }

    pub async fn disarm_idle(self: &Arc<Self>) {
        if let Some(handle) = self.idle_task.write().await.take() {
            handle.abort();
        }
    }

    pub fn spawn_watchers(self: &Arc<Self>) {
        self.spawn_exe_watch();
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

    /// Old code must not keep serving after its binary was replaced; proxies reconnect
    /// and start a daemon from their own binary.
    fn spawn_exe_watch(self: &Arc<Self>) {
        let interval_ms = crate::exe_identity::exe_check_interval_ms();
        let Some(exe) = daemon_exe().filter(|_| interval_ms > 0) else {
            return;
        };
        let me = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(interval_ms));
            loop {
                interval.tick().await;
                let probe = exe.clone();
                let replaced = tokio::task::spawn_blocking(move || probe.replaced_on_disk())
                    .await
                    .unwrap_or(false);
                if !me.is_stopping() && replaced {
                    me.shutdown("binary replaced").await;
                    std::process::exit(0);
                }
            }
        });
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
        // A newer daemon may own these files by now; only this daemon's go.
        let me = std::process::id();
        remove_daemon_info_if_owned(&self.project_root, me);
        ax_core::mark_clean_shutdown(&self.project_root);
        if let Some(pid_path) = &self.pid_path {
            release_daemon_lock_if_owned(pid_path, me);
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
        exe: daemon_exe(),
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
    write_or_close(session, line.as_bytes()).await?;
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
                    if is_daemon_pid(info.pid, Some(info.started_at)) {
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
    // Only a socket that cannot be bound falls back to TCP. Once the socket
    // daemon serves clients it never hands over to a second daemon.
    match bind_socket(&project_root) {
        Ok(socket) => run_socket_daemon(socket, project_root, pid_path).await,
        Err(e) => {
            tracing::warn!("socket daemon unavailable ({e}); falling back to TCP");
            run_tcp_daemon(project_root, pid_path).await
        }
    }
}

#[cfg(unix)]
struct BoundSocket {
    listener: tokio::net::UnixListener,
    path: String,
}

#[cfg(windows)]
struct BoundSocket {
    server: tokio::net::windows::named_pipe::NamedPipeServer,
    path: String,
}

#[cfg(not(any(windows, unix)))]
struct BoundSocket;

#[cfg(unix)]
fn bind_socket(project_root: &Path) -> Result<BoundSocket, Box<dyn std::error::Error>> {
    use tokio::net::UnixListener;
    for path in daemon_socket_candidates(project_root) {
        let _ = std::fs::remove_file(&path);
        match UnixListener::bind(&path) {
            Ok(listener) => return Ok(BoundSocket { listener, path }),
            Err(e) => tracing::warn!("socket bind failed for {path}: {e}"),
        }
    }
    Err("no socket path could be bound".into())
}

#[cfg(windows)]
fn bind_socket(project_root: &Path) -> Result<BoundSocket, Box<dyn std::error::Error>> {
    use tokio::net::windows::named_pipe::ServerOptions;
    let path = daemon_socket_candidates(project_root)
        .first()
        .cloned()
        .ok_or("no pipe candidates")?;
    let server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(&path)?;
    Ok(BoundSocket { server, path })
}

#[cfg(not(any(windows, unix)))]
fn bind_socket(_project_root: &Path) -> Result<BoundSocket, Box<dyn std::error::Error>> {
    Err("no socket transport on this platform".into())
}

#[cfg(not(any(windows, unix)))]
async fn run_socket_daemon(
    _socket: BoundSocket,
    _project_root: PathBuf,
    _pid_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    Err("no socket transport on this platform".into())
}

#[cfg(windows)]
async fn run_socket_daemon(
    socket: BoundSocket,
    project_root: PathBuf,
    pid_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::net::windows::named_pipe::ServerOptions;

    let BoundSocket {
        mut server,
        path: pipe_name,
    } = socket;
    write_daemon_info(&project_root, 0, Some(pipe_name.clone()), None)?;
    let _ = rewrite_lock_socket_path(&pid_path, &pipe_name);
    let _liveness = install_main_thread_watchdog();
    tracing::info!(
        "ax daemon on pipe {} pid {} idle {}ms",
        pipe_name,
        std::process::id(),
        resolve_idle_timeout_ms()
    );

    let lifecycle = start_daemon_core(project_root.clone(), pid_path.clone()).await?;
    let engine = Arc::new(Mutex::new(McpEngine::with_project_root(
        project_root.clone(),
    )));

    while !lifecycle.is_stopping() {
        // A failed connect or a pipe instance that cannot be created is
        // retried after a pause; it never ends the daemon.
        if let Err(e) = server.connect().await {
            tracing::warn!("daemon pipe connect failed: {e}; retrying");
            tokio::time::sleep(ACCEPT_RETRY).await;
            match ServerOptions::new().create(&pipe_name) {
                Ok(fresh) => server = fresh,
                Err(e) => tracing::warn!("daemon pipe create failed: {e}"),
            }
            continue;
        }
        let next = loop {
            match ServerOptions::new().create(&pipe_name) {
                Ok(next) => break next,
                Err(e) => {
                    tracing::warn!("daemon pipe create failed: {e}; retrying");
                    tokio::time::sleep(ACCEPT_RETRY).await;
                }
            }
        };
        let connected = std::mem::replace(&mut server, next);
        let engine = engine.clone();
        let root = project_root.clone();
        let lc = Arc::clone(&lifecycle);
        let sp = pipe_name.clone();
        spawn_client(&lifecycle, async move {
            let session = DaemonSession::from_io(connected);
            serve_session(session, engine, &root, 0, Some(sp), None, lc).await
        });
    }
    Ok(())
}

#[cfg(unix)]
async fn run_socket_daemon(
    socket: BoundSocket,
    project_root: PathBuf,
    pid_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let BoundSocket {
        listener,
        path: socket_path,
    } = socket;
    write_daemon_info(&project_root, 0, Some(socket_path.clone()), None)?;
    let _ = rewrite_lock_socket_path(&pid_path, &socket_path);
    let _liveness = install_main_thread_watchdog();
    tracing::info!(
        "ax daemon on {} pid {} idle {}ms",
        socket_path,
        std::process::id(),
        resolve_idle_timeout_ms()
    );

    let lifecycle = start_daemon_core(project_root.clone(), pid_path.clone()).await?;
    let engine = Arc::new(Mutex::new(McpEngine::with_project_root(
        project_root.clone(),
    )));

    accept_clients(
        || lifecycle.is_stopping(),
        || listener.accept(),
        |(stream, _)| {
            let engine = engine.clone();
            let root = project_root.clone();
            let lc = Arc::clone(&lifecycle);
            let sp = socket_path.clone();
            spawn_client(&lifecycle, async move {
                let session = DaemonSession::from_unix(stream);
                serve_session(session, engine, &root, 0, Some(sp), None, lc).await
            });
        },
    )
    .await;
    Ok(())
}

async fn run_tcp_daemon(
    project_root: PathBuf,
    pid_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let token = new_daemon_token();
    write_daemon_info(&project_root, port, None, Some(token.clone()))?;
    let _liveness = install_main_thread_watchdog();
    tracing::info!(
        "ax daemon on 127.0.0.1:{} pid {} idle {}ms",
        port,
        std::process::id(),
        resolve_idle_timeout_ms()
    );

    let lifecycle = start_daemon_core(project_root.clone(), pid_path.clone()).await?;
    let engine = Arc::new(Mutex::new(McpEngine::with_project_root(
        project_root.clone(),
    )));

    accept_clients(
        || lifecycle.is_stopping(),
        || listener.accept(),
        |(stream, _)| {
            let engine = engine.clone();
            let root = project_root.clone();
            let lc = Arc::clone(&lifecycle);
            let token = token.clone();
            spawn_client(&lifecycle, async move {
                let session = DaemonSession::from_tcp(stream);
                serve_session(session, engine, &root, port, None, Some(token), lc).await
            });
        },
    )
    .await;
    Ok(())
}

/// Serves one accepted client on its own task and keeps the client count.
fn spawn_client<F>(lifecycle: &Arc<DaemonLifecycle>, serve: F)
where
    F: std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>>
        + Send
        + 'static,
{
    lifecycle.client_accepted();
    let lc = Arc::clone(lifecycle);
    tokio::spawn(async move {
        lc.client_started().await;
        let result = serve.await;
        lc.on_client_disconnected().await;
        if let Err(e) = result {
            tracing::warn!("daemon client: {}", e);
        }
    });
}

/// Pause before accepting again after a failed accept.
const ACCEPT_RETRY: Duration = Duration::from_millis(250);

/// Accepts clients until the daemon stops and hands each one to `serve`. A
/// failed accept (for example when the process is out of file descriptors)
/// is logged and retried after a short pause: it never ends the daemon, so a
/// daemon that serves clients is never replaced by a second one.
async fn accept_clients<C, Fut>(
    is_stopping: impl Fn() -> bool,
    mut accept: impl FnMut() -> Fut,
    mut serve: impl FnMut(C),
) where
    Fut: std::future::Future<Output = std::io::Result<C>>,
{
    while !is_stopping() {
        match accept().await {
            Ok(client) => serve(client),
            Err(e) => {
                tracing::warn!("daemon accept failed: {e}; retrying");
                tokio::time::sleep(ACCEPT_RETRY).await;
            }
        }
    }
}

async fn start_daemon_core(
    project_root: PathBuf,
    pid_path: PathBuf,
) -> Result<Arc<DaemonLifecycle>, Box<dyn std::error::Error>> {
    McpEngine::start_background_services(&project_root);
    let lifecycle = DaemonLifecycle::new(project_root.clone(), Some(pid_path));
    lifecycle.spawn_watchers();
    lifecycle.arm_idle().await;
    let force = ax_core::take_unclean_shutdown(&project_root);
    let check_root = project_root.clone();
    tokio::spawn(async move {
        ax_core::run_database_check(&check_root, force).await;
    });

    let lc_signal = Arc::clone(&lifecycle);
    let root_signal = project_root.clone();
    tokio::spawn(async move {
        let reason = stop_signal().await;
        lc_signal.shutdown(reason).await;
        remove_daemon_info_if_owned(&root_signal, std::process::id());
        std::process::exit(0);
    });

    Ok(lifecycle)
}

/// Waits for SIGINT or (Unix) SIGTERM, the signal `ax daemon stop` sends first.
async fn stop_signal() -> &'static str {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                Ok(()) = tokio::signal::ctrl_c() => return "SIGINT",
                Some(()) = term.recv() => return "SIGTERM",
                else => {}
            }
        }
    }
    if tokio::signal::ctrl_c().await.is_err() {
        std::future::pending::<()>().await;
    }
    "SIGINT"
}

/// How long one write to a client may take before the connection is closed.
const CLIENT_WRITE_TIMEOUT: Duration = Duration::from_secs(60);

/// Writes to a client, closing the connection (an error) when the client has
/// not taken the bytes within [`CLIENT_WRITE_TIMEOUT`].
async fn write_or_close(session: &mut DaemonSession, data: &[u8]) -> std::io::Result<()> {
    match tokio::time::timeout(CLIENT_WRITE_TIMEOUT, session.write_bytes(data)).await {
        Ok(result) => result,
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "client stopped reading; connection closed",
        )),
    }
}

async fn serve_session(
    mut session: DaemonSession,
    engine: Arc<Mutex<McpEngine>>,
    project_root: &Path,
    port: u16,
    socket_path: Option<String>,
    token: Option<String>,
    lifecycle: Arc<DaemonLifecycle>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // A TCP port can be reached by any local process, and by web pages. Such
    // a client must send the token from daemon.json as its first line; any
    // other first line (an HTTP request, for example) closes the connection
    // before anything is answered or run.
    if let Some(token) = &token {
        if !token_matches(&mut session, token).await {
            return Err("TCP client did not send the daemon token; connection closed".into());
        }
    }
    static NEXT_CONNECTION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let connection = NEXT_CONNECTION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    send_hello(&mut session, project_root, port, socket_path).await?;
    let result = serve_requests(&mut session, &engine, project_root, &lifecycle, connection).await;
    // Ends the connection's policy session however the connection ended.
    engine.lock().await.policy_sessions().end(connection);
    result
}

async fn serve_requests(
    session: &mut DaemonSession,
    engine: &Mutex<McpEngine>,
    project_root: &Path,
    lifecycle: &DaemonLifecycle,
    connection: u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
                write_or_close(session, out.as_bytes()).await?;
                line.clear();
                continue;
            }
        };

        // The engine is shared by every connection. Its lock covers the
        // request only and is released before anything is written back, so
        // a client that stops reading cannot stall the other clients.
        let outcome = {
            let mut eng = engine.lock().await;
            eng.policy_sessions().set_active(connection);
            handle_request(
                &mut eng,
                &req.method,
                req.params.unwrap_or(serde_json::Value::Null),
            )
            .await
        };
        if crate::transport::is_notification(&req.id) {
            line.clear();
            continue;
        }
        let id = req.id.clone().unwrap_or(serde_json::Value::Null);
        let response = match outcome.result {
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
                    code: outcome.error_code,
                    message: msg,
                    data: None,
                }),
            },
        };
        let out = serde_json::to_string(&response)? + "\n";
        write_or_close(session, out.as_bytes()).await?;
        // Verbose traces: MCP logging notifications (Cursor Output via stdout) +
        // ax_log side-channel (proxy → stderr) + local log file.
        if !outcome.verbose_lines.is_empty() {
            crate::verbose::append_verbose_log_file(&outcome.verbose_lines, Some(project_root));
            for text in &outcome.verbose_lines {
                let notif = crate::verbose::format_mcp_log_notification(text);
                write_or_close(session, notif.as_bytes()).await?;
                let log_line = crate::verbose::format_ax_log_line(text);
                write_or_close(session, log_line.as_bytes()).await?;
            }
        }
        line.clear();
    }
    Ok(())
}

/// Connects to the project's daemon only when it runs this exact ax version.
pub async fn try_connect(project_root: &Path) -> Option<(DaemonSession, DaemonHello)> {
    let info = read_daemon_info(project_root)?;
    if info.version != env!("CARGO_PKG_VERSION") {
        return None;
    }
    connect_any(project_root)
        .await
        .filter(|(_, hello)| hello.ax == env!("CARGO_PKG_VERSION"))
}

/// Connects to whatever daemon serves the project, whatever its version.
pub async fn connect_any(project_root: &Path) -> Option<(DaemonSession, DaemonHello)> {
    let info = read_daemon_info(project_root)?;

    let socket_candidates: Vec<String> = if let Some(path) = info.socket_path.clone() {
        vec![path]
    } else if info.token.is_some() {
        // A TCP daemon: a socket at a default path is not this daemon's and
        // may belong to another user.
        Vec::new()
    } else {
        daemon_socket_candidates(project_root)
    };

    for path in socket_candidates {
        if let Some(mut session) = connect_path(&path).await {
            if let Some(hello) = read_hello(&mut session).await {
                return Some((session, hello));
            }
        }
    }

    if info.port > 0 {
        if let Some(mut session) = connect_tcp(info.port).await {
            if let Some(token) = &info.token {
                let line = format!("{token}\n");
                if session.write_bytes(line.as_bytes()).await.is_err() {
                    return None;
                }
            }
            if let Some(hello) = read_hello(&mut session).await {
                return Some((session, hello));
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

/// Waits up to `timeout_ms` for any daemon to serve the project, whatever its version.
pub async fn wait_for_any_daemon(
    project_root: &Path,
    timeout_ms: u64,
) -> Option<(DaemonSession, DaemonHello)> {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    while tokio::time::Instant::now() < deadline {
        if let Some(found) = connect_any(project_root).await {
            return Some(found);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pid that is not this process (the lock and daemon.json of another daemon).
    const OTHER_PID: u32 = 1;

    fn write_files_of(root: &Path, pid: u32) -> PathBuf {
        let info = DaemonInfo {
            pid,
            port: 0,
            socket_path: Some("sock".into()),
            version: "test".into(),
            project_root: root.to_string_lossy().into_owned(),
            exe: None,
            token: None,
            started_at: None,
        };
        std::fs::write(
            daemon_info_path(root),
            serde_json::to_string(&info).unwrap(),
        )
        .unwrap();
        let pid_path = daemon_pid_path(root);
        let lock = crate::daemon_lock::DaemonLockInfo {
            pid,
            version: "test".into(),
            socket_path: "sock".into(),
            started_at: 0,
        };
        std::fs::write(&pid_path, serde_json::to_string(&lock).unwrap()).unwrap();
        pid_path
    }

    fn lifecycle_with_idle(root: &Path, idle_timeout_ms: u64) -> Arc<DaemonLifecycle> {
        Arc::new(DaemonLifecycle {
            clients: AtomicUsize::new(0),
            stopping: AtomicBool::new(false),
            last_activity: Mutex::new(Instant::now()),
            idle_timeout_ms,
            max_idle_ms: 0,
            idle_task: RwLock::new(None),
            project_root: root.to_path_buf(),
            pid_path: Some(daemon_pid_path(root)),
        })
    }

    fn ax_dir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join(".ax")).unwrap();
        (dir, root)
    }

    #[tokio::test]
    async fn the_idle_timer_does_not_stop_a_daemon_with_a_client() {
        let (_dir, root) = ax_dir();
        let pid_path = write_files_of(&root, std::process::id());
        let lifecycle = lifecycle_with_idle(&root, 60_000);
        lifecycle.on_client_connected().await;
        assert!(!lifecycle.idle_expired().await);
        assert!(!lifecycle.is_stopping());
        assert!(pid_path.exists() && read_daemon_info(&root).is_some());
    }

    #[tokio::test]
    async fn the_idle_timer_stops_a_daemon_without_clients() {
        let (_dir, root) = ax_dir();
        let pid_path = write_files_of(&root, std::process::id());
        let lifecycle = lifecycle_with_idle(&root, 60_000);
        assert!(lifecycle.idle_expired().await);
        assert!(lifecycle.is_stopping());
        assert!(!pid_path.exists() && read_daemon_info(&root).is_none());
        // A second expiry after shutdown does nothing.
        assert!(!lifecycle.idle_expired().await);
    }

    #[tokio::test]
    async fn a_connecting_client_disarms_the_idle_timer_and_the_last_disconnect_rearms_it() {
        let (_dir, root) = ax_dir();
        let lifecycle = lifecycle_with_idle(&root, 60_000);
        lifecycle.arm_idle().await;
        assert!(lifecycle.idle_task.read().await.is_some());
        lifecycle.on_client_connected().await;
        lifecycle.on_client_connected().await;
        assert!(lifecycle.idle_task.read().await.is_none());
        lifecycle.on_client_disconnected().await;
        assert!(
            lifecycle.idle_task.read().await.is_none(),
            "armed while a client is still connected"
        );
        lifecycle.on_client_disconnected().await;
        assert!(lifecycle.idle_task.read().await.is_some());
        assert_eq!(lifecycle.client_count(), 0);
        lifecycle.disarm_idle().await;
    }

    #[tokio::test]
    async fn an_accepted_client_counts_before_its_task_runs() {
        let (_dir, root) = ax_dir();
        let pid_path = write_files_of(&root, std::process::id());
        let lifecycle = lifecycle_with_idle(&root, 60_000);
        // The idle timer fires right after accept, before the client's task ran.
        spawn_client(&lifecycle, std::future::pending());
        assert_eq!(lifecycle.client_count(), 1);
        assert!(!lifecycle.idle_expired().await, "stopped with a client accepted");
        assert!(pid_path.exists());
    }

    #[tokio::test]
    async fn a_shutdown_started_by_the_idle_timer_is_not_cancelled_by_itself() {
        let (_dir, root) = ax_dir();
        write_files_of(&root, std::process::id());
        let lifecycle = lifecycle_with_idle(&root, 60_000);
        let finished = Arc::new(AtomicBool::new(false));
        let start = Arc::new(tokio::sync::Notify::new());
        let (lc, done, go) = (lifecycle.clone(), finished.clone(), start.clone());
        let task = tokio::spawn(async move {
            go.notified().await;
            assert!(lc.idle_timer_fired().await);
            // Any await after the shutdown must still run.
            tokio::task::yield_now().await;
            done.store(true, Ordering::SeqCst);
        });
        *lifecycle.idle_task.write().await = Some(task);
        start.notify_one();
        for _ in 0..200 {
            if finished.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            finished.load(Ordering::SeqCst),
            "the idle task aborted itself during shutdown"
        );
    }

    #[tokio::test]
    async fn an_idle_timeout_of_zero_never_arms() {
        let (_dir, root) = ax_dir();
        let lifecycle = lifecycle_with_idle(&root, 0);
        lifecycle.arm_idle().await;
        assert!(lifecycle.idle_task.read().await.is_none());
    }

    #[tokio::test]
    async fn shutdown_leaves_the_files_of_another_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join(".ax")).unwrap();
        let pid_path = write_files_of(&root, OTHER_PID);
        let lifecycle = DaemonLifecycle::new(root.clone(), Some(pid_path.clone()));
        lifecycle.shutdown("test").await;
        assert_eq!(read_daemon_info(&root).map(|i| i.pid), Some(OTHER_PID));
        assert_eq!(
            crate::daemon_lock::read_lock_info(&pid_path).map(|i| i.pid),
            Some(OTHER_PID)
        );
    }

    #[tokio::test]
    async fn shutdown_removes_its_own_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(root.join(".ax")).unwrap();
        let pid_path = write_files_of(&root, std::process::id());
        let lifecycle = DaemonLifecycle::new(root.clone(), Some(pid_path.clone()));
        lifecycle.shutdown("test").await;
        assert!(read_daemon_info(&root).is_none());
        assert!(!pid_path.exists());
    }

    #[test]
    fn parse_idle_timeout_env() {
        assert_eq!(parse_env_ms(Some("0".to_string()), 100), 0);
        assert_eq!(parse_env_ms(Some("invalid".to_string()), 100), 100);
    }
    #[tokio::test]
    async fn a_failed_accept_does_not_end_the_accept_loop() {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let served = std::sync::Mutex::new(Vec::new());
        accept_clients(
            || served.lock().unwrap().len() >= 2,
            || {
                let n = calls.fetch_add(1, Ordering::SeqCst);
                async move {
                    if n == 0 {
                        // What accept reports when the process is out of file descriptors.
                        Err(std::io::Error::from_raw_os_error(24))
                    } else {
                        Ok(n)
                    }
                }
            },
            |client| served.lock().unwrap().push(client),
        )
        .await;
        assert_eq!(*served.lock().unwrap(), vec![1, 2]);
    }
    async fn read_json_line(session: &mut DaemonSession) -> serde_json::Value {
        let mut line = String::new();
        session.read_line(&mut line).await.unwrap();
        serde_json::from_str(line.trim()).unwrap()
    }

    #[tokio::test]
    async fn a_failed_call_is_an_internal_error_and_an_unknown_method_is_not_found() {
        let _env = crate::test_env::shared().await;
        let dir = tempfile::tempdir().unwrap();
        let (client, server) = tokio::io::duplex(1024 * 1024);
        let root = dir.path().to_path_buf();
        let engine = Arc::new(Mutex::new(McpEngine::with_project_root(root.clone())));
        let lifecycle = DaemonLifecycle::new(root.clone(), None);
        tokio::spawn(async move {
            let session = DaemonSession::from_io(server);
            let _ = serve_session(session, engine, &root, 0, None, None, lifecycle).await;
        });
        let mut client = DaemonSession::from_io(client);
        assert_eq!(read_json_line(&mut client).await["type"], "hello");
        // The project was never indexed, so the call fails inside the tool.
        client
            .write_bytes(
                b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\
                  \"params\":{\"name\":\"ax_search\",\"arguments\":{\"query\":\"x\"}}}\n",
            )
            .await
            .unwrap();
        let failed = read_json_line(&mut client).await;
        assert_eq!(
            failed["error"]["code"],
            crate::transport::INTERNAL_ERROR,
            "{failed}"
        );
        client
            .write_bytes(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"no/such\"}\n")
            .await
            .unwrap();
        let unknown = read_json_line(&mut client).await;
        assert_eq!(
            unknown["error"]["code"],
            crate::transport::METHOD_NOT_FOUND,
            "{unknown}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_line_longer_than_the_limit_closes_the_connection() {
        let _env = crate::test_env::shared().await;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let engine = Arc::new(Mutex::new(McpEngine::with_project_root(root.clone())));
        let lifecycle = DaemonLifecycle::new(root.clone(), None);
        let (client, server) = tokio::io::duplex(1024 * 1024);
        let served = tokio::spawn(async move {
            serve_session(
                DaemonSession::from_io(server),
                engine,
                &root,
                0,
                None,
                None,
                lifecycle,
            )
            .await
        });
        let mut client = DaemonSession::from_io(client);
        assert_eq!(read_json_line(&mut client).await["type"], "hello");
        // Bytes without a newline, one more than the limit.
        let chunk = vec![b'a'; 1024 * 1024];
        let mut left = crate::daemon_conn::MAX_LINE_BYTES + 1;
        while left > 0 {
            let n = left.min(chunk.len() as u64) as usize;
            if client.write_bytes(&chunk[..n]).await.is_err() {
                break;
            }
            left -= n as u64;
        }
        let ended = tokio::time::timeout(Duration::from_secs(20), served).await;
        let result = ended.expect("the daemon kept reading an endless line").unwrap();
        assert!(result.is_err(), "an over-long line is an error");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_client_that_stops_reading_does_not_block_other_clients() {
        let _env = crate::test_env::shared().await;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let engine = Arc::new(Mutex::new(McpEngine::with_project_root(root.clone())));
        let lifecycle = DaemonLifecycle::new(root.clone(), None);
        let serve = |io: tokio::io::DuplexStream| {
            let (engine, root, lc) = (engine.clone(), root.clone(), lifecycle.clone());
            tokio::spawn(async move {
                let _ = serve_session(DaemonSession::from_io(io), engine, &root, 0, None, None, lc)
                    .await;
            })
        };
        let list = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n";

        // Window A reads the hello, then stops reading its responses.
        let (a_client, a_server) = tokio::io::duplex(8 * 1024);
        serve(a_server);
        let mut a = DaemonSession::from_io(a_client);
        assert_eq!(read_json_line(&mut a).await["type"], "hello");
        for _ in 0..4 {
            a.write_bytes(list).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Window B must still get its answer.
        let (b_client, b_server) = tokio::io::duplex(1024 * 1024);
        serve(b_server);
        let mut b = DaemonSession::from_io(b_client);
        assert_eq!(read_json_line(&mut b).await["type"], "hello");
        b.write_bytes(list).await.unwrap();
        let answer = tokio::time::timeout(Duration::from_secs(10), read_json_line(&mut b))
            .await
            .expect("window B was blocked by window A's stalled socket");
        assert!(answer["result"]["tools"].is_array(), "{answer}");
        drop(a);
    }
    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn spawn_tcp_style_session(io: tokio::io::DuplexStream, root: PathBuf) {
        let engine = Arc::new(Mutex::new(McpEngine::with_project_root(root.clone())));
        let lifecycle = DaemonLifecycle::new(root.clone(), None);
        tokio::spawn(async move {
            let session = DaemonSession::from_io(io);
            let _ = serve_session(
                session,
                engine,
                &root,
                4242,
                None,
                Some(TOKEN.to_string()),
                lifecycle,
            )
            .await;
        });
    }

    #[tokio::test]
    async fn a_tcp_session_without_the_token_is_closed_before_anything_runs() {
        let _env = crate::test_env::shared().await;
        let dir = tempfile::tempdir().unwrap();
        let (client, server) = tokio::io::duplex(1024 * 1024);
        spawn_tcp_style_session(server, dir.path().to_path_buf());
        let mut client = DaemonSession::from_io(client);
        // What a web page's fetch() to the port sends: HTTP headers, then a
        // JSON-RPC line as the body.
        client
            .write_bytes(
                b"POST / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n\
                  {\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n",
            )
            .await
            .unwrap();
        let mut all = String::new();
        let read = tokio::time::timeout(Duration::from_secs(10), async {
            let mut line = String::new();
            while client.read_line(&mut line).await.unwrap_or(0) > 0 {
                all.push_str(&line);
                line.clear();
            }
        })
        .await;
        assert!(read.is_ok(), "the connection was not closed");
        assert!(all.is_empty(), "an unauthenticated client got: {all}");
    }

    #[tokio::test]
    async fn a_tcp_session_with_the_token_is_served() {
        let _env = crate::test_env::shared().await;
        let dir = tempfile::tempdir().unwrap();
        let (client, server) = tokio::io::duplex(1024 * 1024);
        spawn_tcp_style_session(server, dir.path().to_path_buf());
        let mut client = DaemonSession::from_io(client);
        client
            .write_bytes(format!("{TOKEN}\n").as_bytes())
            .await
            .unwrap();
        assert_eq!(read_json_line(&mut client).await["type"], "hello");
        client
            .write_bytes(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n")
            .await
            .unwrap();
        assert!(read_json_line(&mut client).await["result"]["tools"].is_array());
    }

    #[tokio::test]
    async fn connect_any_sends_the_token_to_a_tcp_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(get_ax_dir(&root)).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        write_daemon_info(&root, port, None, Some(TOKEN.to_string())).unwrap();
        let hello_root = root.clone();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut session = DaemonSession::from_tcp(stream);
            let mut first = String::new();
            let got =
                tokio::time::timeout(Duration::from_secs(2), session.read_line(&mut first)).await;
            if got.is_ok() && first.trim() == TOKEN {
                let _ = send_hello(&mut session, &hello_root, port, None).await;
            }
        });
        let connected = connect_any(&root).await;
        assert!(connected.is_some(), "the client did not send the token");
    }

    #[cfg(unix)]
    #[test]
    fn daemon_info_with_a_token_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(get_ax_dir(dir.path())).unwrap();
        write_daemon_info(dir.path(), 1, None, Some(TOKEN.to_string())).unwrap();
        let mode = std::fs::metadata(daemon_info_path(dir.path()))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "daemon.json mode {mode:o}");
        assert_eq!(
            read_daemon_info(dir.path()).unwrap().token.as_deref(),
            Some(TOKEN)
        );
    }
}
