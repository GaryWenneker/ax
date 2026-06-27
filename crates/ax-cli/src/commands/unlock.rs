//! Remove stale `.ax/ax.lock` and stop orphaned ax processes.

use ax_context::directory::{get_ax_dir, is_initialized};

use crate::commands::resolve_path;
use crate::ui::{info_line, ok_line};

pub async fn run(path: Option<String>) -> Result<(), String> {
    let root = resolve_path(path);
    if !is_initialized(&root) {
        return Err(format!(
            "project not initialized in {} — run ax init first",
            root.display()
        ));
    }
    let lock_path = get_ax_dir(&root).join("ax.lock");
    let holder = read_lock_pid(&lock_path);

    let killed = kill_all_ax_processes(std::process::id())?;
    if killed > 0 {
        println!("{}", info_line(format!("Stopped {killed} ax process(es)")));
    }

    if holder.is_some() {
        ax_utils::clear_stale_lock(&lock_path);
    }

    if lock_path.exists() {
        std::fs::remove_file(&lock_path).map_err(|e| e.to_string())?;
    }

    println!("{}", ok_line("Lock cleared. You can run ax init or ax index again."));
    Ok(())
}

fn read_lock_pid(path: &std::path::Path) -> Option<u32> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn kill_all_ax_processes(self_pid: u32) -> Result<usize, String> {
    #[cfg(windows)]
    {
        kill_ax_windows(self_pid)
    }
    #[cfg(unix)]
    {
        kill_ax_unix(self_pid)
    }
}

#[cfg(windows)]
fn kill_ax_windows(self_pid: u32) -> Result<usize, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut killed = 0usize;
    let procs = std::process::Command::new("wmic")
        .args(["process", "where", "name='ax.exe'", "get", "ProcessId", "/format:csv"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&procs.stdout);
    for line in text.lines() {
        let pid: u32 = match line.split(',').nth(1).and_then(|s| s.trim().parse().ok()) {
            Some(p) if p != self_pid => p,
            _ => continue,
        };
        if std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            killed += 1;
        }
    }
    Ok(killed)
}

#[cfg(unix)]
fn kill_ax_unix(self_pid: u32) -> Result<usize, String> {
    let out = std::process::Command::new("pgrep")
        .arg("-x")
        .arg("ax")
        .output()
        .map_err(|e| e.to_string())?;
    let mut killed = 0usize;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let pid: u32 = match line.trim().parse() {
            Ok(p) if p != self_pid => p,
            _ => continue,
        };
        if std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            killed += 1;
        }
    }
    Ok(killed)
}
