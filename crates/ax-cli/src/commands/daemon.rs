//! Daemon status/stop/restart — CG: daemon lifecycle CLI.

use ax_context::directory::{get_ax_dir, is_initialized};
use ax_mcp::daemon::{read_daemon_info, try_connect, DAEMON_INFO_FILE};
use ax_mcp::daemon_lock::is_pid_alive;
use ax_mcp::restart_daemon;

use crate::commands::resolve_path;
use crate::ui::{info_line, ok_line};

#[derive(Clone, Copy)]
pub enum DaemonAction {
    Status,
    Stop,
    Restart,
}

pub async fn run(path: Option<String>, action: DaemonAction) -> Result<(), String> {
    let root = resolve_path(path);
    if !is_initialized(&root) {
        return Err(format!(
            "project not initialized in {} — run ax init first",
            root.display()
        ));
    }
    match action {
        DaemonAction::Status => {
            let info_path = get_ax_dir(&root).join(DAEMON_INFO_FILE);
            if let Some(info) = read_daemon_info(&root) {
                let alive = is_pid_alive(info.pid);
                let connected = try_connect(&root).await.is_some();
                let socket = info.socket_path.as_deref().unwrap_or("(none)");
                println!(
                    "daemon pid {} port {} socket {} version {} alive {} connected {}",
                    info.pid, info.port, socket, info.version, alive, connected
                );
            } else {
                println!("no daemon info at {}", info_path.display());
            }
            Ok(())
        }
        DaemonAction::Stop => {
            let report = stop_only(&root).await?;
            println!("{}", ok_line(report));
            Ok(())
        }
        DaemonAction::Restart => {
            let report = restart_daemon(&root).await?;
            if let Some(old) = report.stopped_pid {
                println!(
                    "{}",
                    info_line(format!("Stopped previous daemon pid {old}"))
                );
            }
            if report.cleared_ax_lock {
                println!("{}", info_line("Cleared stale .ax/ax.lock"));
            }
            if let Some(pid) = report.started_pid {
                println!(
                    "{}",
                    ok_line(format!(
                        "MCP daemon ready pid {pid} connected={}",
                        report.connected
                    ))
                );
            } else {
                println!("{}", ok_line("MCP daemon restarted"));
            }
            println!("{}", info_line(report.hint));
            Ok(())
        }
    }
}

async fn stop_only(root: &std::path::Path) -> Result<String, String> {
    use ax_mcp::daemon::remove_daemon_info_if_owned;
    use ax_mcp::daemon_lock::{
        release_daemon_lock_if_owned, stop_daemon_pid, StopOutcome, DAEMON_STOP_GRACE,
    };
    use ax_mcp::daemon_paths::daemon_pid_path;

    let Some(info) = read_daemon_info(root) else {
        return Ok("no daemon running".into());
    };
    let (pid, started_at) = (info.pid, info.started_at);
    let outcome =
        tokio::task::spawn_blocking(move || stop_daemon_pid(pid, started_at, DAEMON_STOP_GRACE))
            .await
            .map_err(|e| format!("stop task failed: {e}"))?;
    if outcome == StopOutcome::StillRunning {
        return Err(format!("ax daemon pid {pid} did not stop"));
    }
    // The daemon removes its own files on a clean stop; these are left after a
    // force-kill, or describe a daemon that is gone.
    remove_daemon_info_if_owned(root, pid);
    release_daemon_lock_if_owned(&daemon_pid_path(root), pid);
    Ok(match outcome {
        StopOutcome::Stopped => format!("stopped daemon pid {pid}"),
        StopOutcome::Killed => format!("daemon pid {pid} did not stop in time and was killed"),
        StopOutcome::NotADaemon => {
            format!("pid {pid} is not an ax daemon; left it running and removed the stale files")
        }
        _ => format!("daemon pid {pid} was not running; removed its files"),
    })
}
