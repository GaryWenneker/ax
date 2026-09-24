//! Stdio to daemon proxy — CG: mcp/proxy.ts.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use tokio::io::BufReader;

use ax_context::directory::get_ax_dir;

use crate::daemon::{
    connect_any, read_daemon_info, remove_daemon_info, try_connect, wait_for_any_daemon,
    wait_for_daemon, DaemonHello,
};
use crate::daemon_conn::DaemonSession;
use crate::daemon_lock::{is_pid_alive, kill_pid, read_lock_info, release_daemon_lock};
use crate::daemon_paths::daemon_pid_path;
use crate::exe_identity::{current_exe_path, decide, Attach, ExeIdentity};
use crate::liveness_watchdog::install_main_thread_watchdog;
use crate::ppid_watchdog::spawn_ppid_watchdog;
use crate::proxy_pump::pump;

/// Result of bouncing the shared per-project MCP daemon.
#[derive(Debug, Clone, Serialize)]
pub struct DaemonRestartReport {
    pub ok: bool,
    pub stopped_pid: Option<u32>,
    pub started_pid: Option<u32>,
    pub cleared_ax_lock: bool,
    pub connected: bool,
    pub hint: String,
}

/// How long a proxy keeps trying to reach a daemon after losing its connection.
const RECONNECT_DEADLINE: Duration = Duration::from_secs(15);

/// Reconnect attempts before this one only connect: a restarting daemon usually comes
/// back on its own, and spawning too early races the process that is restarting it.
const FIRST_SPAWNING_ATTEMPT: u32 = 3;

/// Serves the MCP client on stdio through the daemon until the client closes stdin.
/// Exits the process with status 1 when the daemon is gone and does not come back.
pub async fn run_stdio_proxy(project_root: &Path, session: DaemonSession) {
    spawn_ppid_watchdog(|| std::process::exit(0));
    let _liveness = install_main_thread_watchdog();

    let outcome = pump(
        BufReader::new(tokio::io::stdin()),
        tokio::io::stdout(),
        session,
        |attempt| connect_or_spawn(project_root, attempt >= FIRST_SPAWNING_ATTEMPT),
        RECONNECT_DEADLINE,
    )
    .await;
    if let Err(reason) = outcome {
        eprintln!("ax: {reason}");
        std::process::exit(1);
    }
}

/// A session with a daemon this proxy may use, restarting or spawning one when allowed.
async fn connect_or_spawn(project_root: &Path, may_spawn: bool) -> Option<DaemonSession> {
    if let Some((session, hello)) = connect_any(project_root).await {
        let mine = tokio::task::spawn_blocking(ExeIdentity::current)
            .await
            .ok()
            .flatten();
        match decide(
            mine.as_ref(),
            env!("CARGO_PKG_VERSION"),
            hello.exe.as_ref(),
            &hello.ax,
        ) {
            Attach::Same => {
                log_attached(&hello);
                return Some(session);
            }
            Attach::Newer => {
                tracing::warn!(
                    "ax daemon pid {} runs a newer build (v{}); attaching to it",
                    hello.pid,
                    hello.ax
                );
                return Some(session);
            }
            Attach::RestartOnMine if may_spawn => {
                drop(session);
                tracing::info!("restarting ax daemon pid {} on this newer build", hello.pid);
                if let Err(e) = restart_daemon(project_root).await {
                    tracing::warn!("could not restart the ax daemon: {e}");
                    return None;
                }
                return connect_any(project_root).await.map(|(s, _)| s);
            }
            Attach::RestartOnMine => return None,
        }
    }
    if !may_spawn {
        return None;
    }
    if let Err(e) = spawn_daemon_child(project_root) {
        tracing::warn!("could not start an ax daemon: {e}");
        return None;
    }
    wait_for_any_daemon(project_root, 10_000)
        .await
        .map(|(session, _)| session)
}

fn log_attached(hello: &DaemonHello) {
    if let Some(path) = &hello.socket_path {
        tracing::info!(
            "attached to ax daemon pid {} socket {} v{}",
            hello.pid,
            path,
            hello.ax
        );
    } else {
        tracing::info!(
            "attached to ax daemon pid {} port {} v{}",
            hello.pid,
            hello.port,
            hello.ax
        );
    }
}

/// Starts a daemon for the project in the background and returns its pid.
pub fn spawn_daemon_child(project_root: &Path) -> std::io::Result<u32> {
    let exe = current_exe_path().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "cannot locate the ax binary")
    })?;
    let mut child = std::process::Command::new(exe)
        .arg("serve")
        .arg("--mcp")
        .arg("--daemon")
        .arg("--path")
        .arg(project_root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let pid = child.id();
    // A daemon that exits before this process does stays a zombie until it is waited on.
    if let Err(e) = std::thread::Builder::new()
        .name("ax-daemon-reaper".into())
        .spawn(move || child.wait())
    {
        tracing::warn!("ax daemon pid {pid} started, but will not be reaped: {e}");
    }
    Ok(pid)
}

/// Serves stdio through the project daemon. `Err` only when no daemon could be reached at
/// all, so the caller can fall back to an in-process server.
pub async fn attach_or_spawn(project_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let session = connect_or_spawn(project_root, true)
        .await
        .ok_or("no ax daemon could be reached or started within 10s")?;
    run_stdio_proxy(project_root, session).await;
    Ok(())
}

/// Stop the shared MCP daemon (if any), clear stale locks, and start a fresh daemon.
///
/// Safe for Command Center: does **not** kill unrelated `ax.exe` processes (unlike `ax unlock`).
pub async fn restart_daemon(project_root: &Path) -> Result<DaemonRestartReport, String> {
    let mut stopped_pid = None;
    if let Some(info) = read_daemon_info(project_root) {
        if is_pid_alive(info.pid) {
            let _ = kill_pid(info.pid);
            stopped_pid = Some(info.pid);
        }
    }
    let pid_path = daemon_pid_path(project_root);
    if let Some(lock) = read_lock_info(&pid_path) {
        if stopped_pid != Some(lock.pid) && is_pid_alive(lock.pid) {
            let _ = kill_pid(lock.pid);
            if stopped_pid.is_none() {
                stopped_pid = Some(lock.pid);
            }
        }
    }
    remove_daemon_info(project_root);
    release_daemon_lock(&pid_path);

    let lock_path = get_ax_dir(project_root).join("ax.lock");
    let had_lock = lock_path.exists();
    ax_utils::clear_stale_lock(&lock_path);
    let cleared_ax_lock = had_lock && !lock_path.exists();

    // Give the OS a beat to release named pipes / sockets after kill.
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    spawn_daemon_child(project_root).map_err(|e| format!("failed to spawn MCP daemon: {e}"))?;
    if wait_for_daemon(project_root, 12_000).await.is_none() {
        return Err("MCP daemon failed to start within 12s".into());
    }
    let info = read_daemon_info(project_root);
    let connected = try_connect(project_root).await.is_some();
    Ok(DaemonRestartReport {
        ok: connected,
        stopped_pid,
        started_pid: info.map(|i| i.pid),
        cleared_ax_lock,
        connected,
        hint: "Shared MCP daemon restarted. If Cursor or Takumi still show DEGRADED, run MCP: Restart Servers (or reload the window). Prefer the daemon over parallel embedded MCP processes on the same .ax/ax.db.".into(),
    })
}