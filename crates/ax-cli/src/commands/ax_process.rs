//! Find and stop ax processes without external process-listing tools.
//!
//! Process lookup and signals go through `sysinfo`, so these helpers work where `pgrep`,
//! `wmic` or `kill` are missing (slim containers, current Windows 11).

use std::path::{Path, PathBuf};
use std::time::Duration;

fn lookup<T>(pid: u32, f: impl FnOnce(&sysinfo::Process) -> T) -> Option<T> {
    let mut sys = sysinfo::System::new().ok()?;
    let spid = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[spid]), true);
    sys.process(spid).map(f)
}

/// The pid exists and is not a zombie waiting to be reaped.
pub(crate) fn process_running(pid: u32) -> bool {
    lookup(pid, |p| {
        !matches!(p.status(), sysinfo::ProcessStatus::Zombie)
    })
    .unwrap_or(false)
}

/// Whether a file stem names the ax binary (`ax` or `ax.exe`).
pub(crate) fn is_ax_name(stem: Option<&std::ffi::OsStr>) -> bool {
    stem.and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("ax"))
}

/// Linux appends ` (deleted)` to `/proc/<pid>/exe` once the binary was replaced on disk.
pub(crate) fn strip_deleted_suffix(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_suffix(" (deleted)") {
        Some(stripped) => PathBuf::from(stripped),
        None => path.to_path_buf(),
    }
}

fn process_is_ax(p: &sysinfo::Process) -> bool {
    match p.exe().map(strip_deleted_suffix) {
        Some(exe) => is_ax_name(exe.file_stem()),
        None => is_ax_name(Path::new(p.name()).file_stem()),
    }
}

/// Whether `pid` is a running ax process (its executable is called `ax`).
pub(crate) fn is_ax_process(pid: u32) -> bool {
    lookup(pid, |p| {
        !matches!(p.status(), sysinfo::ProcessStatus::Zombie) && process_is_ax(p)
    })
    .unwrap_or(false)
}

/// Every running ax process except `self_pid`.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn other_ax_pids(self_pid: u32) -> Vec<u32> {
    let Ok(mut sys) = sysinfo::System::new() else {
        return Vec::new();
    };
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let mut pids: Vec<u32> = sys
        .processes()
        .iter()
        .filter(|(_, p)| process_is_ax(p))
        .map(|(pid, _)| pid.as_u32())
        .filter(|&pid| pid != self_pid)
        .collect();
    pids.sort_unstable();
    pids
}

/// Asks the process to exit: SIGTERM on Unix, `taskkill` without `/F` on Windows.
pub(crate) fn send_terminate(pid: u32) {
    #[cfg(unix)]
    {
        let _ = lookup(pid, |p| p.kill_with(sysinfo::Signal::Term));
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
}

/// Ends the process at once (SIGKILL on Unix, TerminateProcess on Windows).
pub(crate) fn force_kill(pid: u32) -> bool {
    lookup(pid, |p| p.kill()).unwrap_or(false)
}

/// Polls until `pid` is gone or `limit` passes; true when it is gone.
pub(crate) async fn wait_for_exit(pid: u32, limit: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + limit;
    loop {
        if !process_running(pid) {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Stops `pid` gracefully, then forcefully after `grace`; errors when it outlives both.
pub(crate) async fn stop_pid(pid: u32, grace: Duration, kill_wait: Duration) -> Result<(), String> {
    send_terminate(pid);
    if wait_for_exit(pid, grace).await {
        return Ok(());
    }
    force_kill(pid);
    if wait_for_exit(pid, kill_wait).await {
        Ok(())
    } else {
        Err(format!(
            "pid {pid} is still running after SIGTERM and SIGKILL"
        ))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn own_process_is_not_listed() {
        assert!(!other_ax_pids(std::process::id()).contains(&std::process::id()));
    }

    #[test]
    fn non_ax_process_is_not_ax() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = child.id();
        let is_ax = is_ax_process(pid);
        let listed = other_ax_pids(0).contains(&pid);
        let _ = child.kill();
        let _ = child.wait();
        assert!(!is_ax && !listed);
    }
}
