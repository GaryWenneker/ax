//! Daemon status/stop — CG: daemon lifecycle CLI.

use std::process::Command;

use ax_context::directory::{get_ax_dir, is_initialized};
use ax_mcp::daemon_lock::release_daemon_lock;
use ax_mcp::daemon_paths::daemon_pid_path;
use ax_mcp::daemon::{read_daemon_info, remove_daemon_info, try_connect, DAEMON_INFO_FILE};

use crate::commands::resolve_path;
use crate::ui::ok_line;

#[derive(Clone, Copy)]
pub enum DaemonAction {
    Status,
    Stop,
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
            if let Some(info) = read_daemon_info(&root) {
                kill_pid(info.pid);
                                remove_daemon_info(&root);
                release_daemon_lock(&daemon_pid_path(&root));
                println!("{}", ok_line(format!("stopped daemon pid {}", info.pid)));
            } else {
                println!("no daemon running");
            }
            Ok(())
        }
    }
}

fn is_pid_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid)])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
    #[cfg(unix)]
    {
        std::path::Path::new(&format!("/proc/{}", pid)).exists()
    }
}

fn kill_pid(pid: u32) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .status();
    }
    #[cfg(unix)]
    {
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }
}