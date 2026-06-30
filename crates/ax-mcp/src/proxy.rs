//! Stdio to daemon proxy — CG: mcp/proxy.ts.

use std::path::Path;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::daemon::{read_daemon_info, try_connect, wait_for_daemon};
use crate::liveness_watchdog::install_main_thread_watchdog;
use crate::ppid_watchdog::spawn_ppid_watchdog;

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