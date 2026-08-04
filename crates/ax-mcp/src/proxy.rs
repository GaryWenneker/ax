//! Stdio to daemon proxy — CG: mcp/proxy.ts.

use std::path::Path;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use ax_context::directory::get_ax_dir;

use crate::daemon::{read_daemon_info, remove_daemon_info, try_connect, wait_for_daemon};
use crate::daemon_lock::{is_pid_alive, kill_pid, read_lock_info, release_daemon_lock};
use crate::daemon_paths::daemon_pid_path;
use crate::liveness_watchdog::install_main_thread_watchdog;
use crate::ppid_watchdog::spawn_ppid_watchdog;

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

pub async fn run_stdio_proxy(project_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    spawn_ppid_watchdog(|| std::process::exit(0));
    let _liveness = install_main_thread_watchdog();

    let (session, hello) = try_connect(project_root)
        .await
        .ok_or("failed to connect to ax daemon")?;

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

    let (read_half, mut write_half) = session.into_split();

    let stdin_to_socket = tokio::spawn(async move {
        let mut stdin = BufReader::new(tokio::io::stdin());
        let mut line = String::new();
        while stdin.read_line(&mut line).await.unwrap_or(0) > 0 {
            if write_half.write_all(line.as_bytes()).await.is_err() {
                break;
            }
            if write_half.flush().await.is_err() {
                break;
            }
            line.clear();
        }
    });

    let socket_to_stdout = tokio::spawn(async move {
        let mut reader = BufReader::new(read_half);
        let mut stdout = tokio::io::stdout();
        let mut line = String::new();
        while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
            // Daemon hello handshake — not JSON-RPC; must not reach Cursor stdout.
            let trimmed = line.trim();
            if trimmed.starts_with("{\"type\":\"hello\"") {
                line.clear();
                continue;
            }
            // Verbose MCP traces (daemon side-channel): stderr only — never Cursor stdout.
            if let Some(text) = crate::verbose::parse_ax_log_line(trimmed) {
                eprintln!("{text}");
                line.clear();
                continue;
            }
            // Also mirror MCP logging notifications to stderr (Cursor Output often
            // surfaces process stderr more reliably than notification traffic).
            if trimmed.contains("\"notifications/message\"") && trimmed.contains("\"ax-mcp\"") {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if let Some(data) = v
                        .pointer("/params/data")
                        .and_then(|d| d.as_str())
                    {
                        eprintln!("{data}");
                    }
                }
            }
            if stdout.write_all(line.as_bytes()).await.is_err() {
                break;
            }
            if stdout.flush().await.is_err() {
                break;
            }
            line.clear();
        }
    });

    let _ = stdin_to_socket.await;
    let _ = socket_to_stdout.await;
    Ok(())
}

pub fn spawn_daemon_child(project_root: &Path) -> std::io::Result<std::process::Child> {
    let exe = std::env::current_exe()?;
    std::process::Command::new(exe)
        .arg("serve")
        .arg("--mcp")
        .arg("--daemon")
        .arg("--path")
        .arg(project_root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
}

pub async fn attach_or_spawn(project_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if try_connect(project_root).await.is_some() {
        return run_stdio_proxy(project_root).await;
    }
    let _ = read_daemon_info(project_root);
    spawn_daemon_child(project_root)?;
    if wait_for_daemon(project_root, 10_000).await.is_none() {
        return Err("daemon failed to start within 10s".into());
    }
    run_stdio_proxy(project_root).await
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