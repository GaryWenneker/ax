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
            // Restart helper stops cleanly; reuse it then kill the fresh daemon? No —
            // keep stop semantics: only stop, do not start.
            let report = stop_only(&root).await?;
            println!("{}", ok_line(report));
            Ok(())
        }
        DaemonAction::Restart => {
            let report = restart_daemon(&root).await?;
            if let Some(old) = report.stopped_pid {
                println!("{}", info_line(format!("Stopped previous daemon pid {old}")));
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
    use ax_mcp::daemon::remove_daemon_info;
    use ax_mcp::daemon_lock::{kill_pid, release_daemon_lock};
    use ax_mcp::daemon_paths::daemon_pid_path;

    if let Some(info) = read_daemon_info(root) {
        if is_pid_alive(info.pid) {
            let _ = kill_pid(info.pid);
        }
        remove_daemon_info(root);
        release_daemon_lock(&daemon_pid_path(root));
        Ok(format!("stopped daemon pid {}", info.pid))
    } else {
        Ok("no daemon running".into())
    }
}
