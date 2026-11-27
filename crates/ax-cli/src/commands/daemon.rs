//! Daemon status/stop/restart — CG: daemon lifecycle CLI.

use ax_context::directory::{get_ax_dir, is_initialized};
use ax_mcp::daemon::{read_daemon_info, try_connect, DaemonInfo, DAEMON_INFO_FILE};
use ax_mcp::daemon_lock::is_pid_alive;
use ax_mcp::restart_daemon;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::commands::resolve_project_root;
use crate::ui::{info_line, ok_line};

#[derive(Clone, Copy)]
pub enum DaemonAction {
    Status,
    Stop,
    Restart,
}

pub async fn run(path: Option<String>, action: DaemonAction) -> Result<(), String> {
    let root = resolve_project_root(path);
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

async fn stop_only(root: &Path) -> Result<String, String> {
    stop_recorded_daemon(root, STOP_GRACE, KILL_WAIT).await
}

/// How long `ax daemon stop` waits after SIGTERM before it escalates to SIGKILL.
const STOP_GRACE: Duration = Duration::from_secs(5);
/// How long it waits for the process to disappear after SIGKILL.
const KILL_WAIT: Duration = Duration::from_secs(3);

/// Stops the daemon named in `.ax/daemon.json`.
///
/// The recorded pid is signalled only when it is still an ax daemon: it answered the daemon
/// socket with that pid, or its executable is the one the record names (or is called `ax`
/// for records without an executable). An ax daemon gets SIGTERM first and is waited for;
/// SIGKILL follows only after `grace`. It reports "stopped" only once the pid is gone and
/// returns an error when it outlives SIGKILL, so `ax db repair --stop-writers` aborts.
/// A record whose pid is dead or now belongs to another program is removed, and that
/// program is left running.
async fn stop_recorded_daemon(
    root: &Path,
    grace: Duration,
    kill_wait: Duration,
) -> Result<String, String> {
    use ax_mcp::daemon::connect_any;
    use ax_mcp::daemon_lock::kill_pid;

    let Some(info) = read_daemon_info(root) else {
        return Ok("no daemon running".into());
    };
    let pid = info.pid;
    if !process_running(pid) {
        clear_daemon_record(root);
        return Ok(format!(
            "no daemon running (removed the stale record of pid {pid})"
        ));
    }
    let hello_pid = connect_any(root).await.map(|(_, hello)| hello.pid);
    if !is_ax_daemon(pid, &info, hello_pid) {
        clear_daemon_record(root);
        return Ok(format!(
            "no daemon running (pid {pid} now belongs to another program; left it running and removed the stale record)"
        ));
    }
    send_terminate(pid);
    if !wait_for_exit(pid, grace).await {
        let _ = kill_pid(pid);
        if !wait_for_exit(pid, kill_wait).await {
            return Err(format!(
                "daemon pid {pid} is still running after SIGTERM and SIGKILL"
            ));
        }
    }
    clear_daemon_record(root);
    Ok(format!("stopped daemon pid {pid}"))
}

fn clear_daemon_record(root: &Path) {
    use ax_mcp::daemon::remove_daemon_info;
    use ax_mcp::daemon_lock::release_daemon_lock;
    use ax_mcp::daemon_paths::daemon_pid_path;
    remove_daemon_info(root);
    release_daemon_lock(&daemon_pid_path(root));
}

/// The pid exists and is not a zombie waiting to be reaped.
fn process_running(pid: u32) -> bool {
    let mut sys = sysinfo::System::new();
    let spid = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[spid]), true);
    sys.process(spid)
        .is_some_and(|p| !matches!(p.status(), sysinfo::ProcessStatus::Zombie))
}

/// Whether `pid` is the ax daemon described by `info`.
fn is_ax_daemon(pid: u32, info: &DaemonInfo, hello_pid: Option<u32>) -> bool {
    if hello_pid == Some(pid) {
        return true;
    }
    let mut sys = sysinfo::System::new();
    let spid = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[spid]), true);
    let Some(process) = sys.process(spid) else {
        return false;
    };
    let exe = process.exe().map(strip_deleted_suffix);
    match (&info.exe, exe) {
        (Some(recorded), Some(exe)) => same_file_path(&exe, Path::new(&recorded.path)),
        (Some(recorded), None) => Path::new(&recorded.path).file_name() == Some(process.name()),
        (None, Some(exe)) => is_ax_name(exe.file_stem()),
        (None, None) => is_ax_name(Path::new(process.name()).file_stem()),
    }
}

fn is_ax_name(stem: Option<&std::ffi::OsStr>) -> bool {
    stem.and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("ax"))
}

/// Linux appends ` (deleted)` to `/proc/<pid>/exe` once the binary was replaced on disk.
fn strip_deleted_suffix(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_suffix(" (deleted)") {
        Some(stripped) => PathBuf::from(stripped),
        None => path.to_path_buf(),
    }
}

fn same_file_path(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    canon(a) == canon(b)
}

/// Asks the process to exit: SIGTERM on Unix, `taskkill` without `/F` on Windows.
fn send_terminate(pid: u32) {
    #[cfg(unix)]
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status();
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

/// Polls until `pid` is gone or `limit` passes; true when it is gone.
async fn wait_for_exit(pid: u32, limit: Duration) -> bool {
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use ax_mcp::daemon::daemon_info_path;
    use ax_mcp::exe_identity::ExeIdentity;

    /// Starts `script` under `sh` in the background, detached from this test process so
    /// init reaps it, and returns its pid.
    fn spawn_orphan(script: &str) -> u32 {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("({script}) >/dev/null 2>&1 & echo $!"))
            .output()
            .expect("spawn sh");
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .expect("pid from sh")
    }

    fn write_info(root: &Path, pid: u32, exe: Option<ExeIdentity>) {
        std::fs::create_dir_all(root.join(".ax")).unwrap();
        let info = ax_mcp::daemon::DaemonInfo {
            pid,
            port: 0,
            socket_path: None,
            version: env!("CARGO_PKG_VERSION").to_string(),
            project_root: root.to_string_lossy().into_owned(),
            exe,
        };
        std::fs::write(
            daemon_info_path(root),
            serde_json::to_string(&info).unwrap(),
        )
        .unwrap();
        assert!(read_daemon_info(root).is_some(), "daemon info must parse");
    }

    fn exe_of(pid: u32) -> PathBuf {
        let mut sys = sysinfo::System::new();
        let spid = sysinfo::Pid::from_u32(pid);
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[spid]), true);
        sys.process(spid)
            .and_then(|p| p.exe())
            .map(Path::to_path_buf)
            .expect("exe of test process")
    }

    /// Waits until `pid` runs `sleep`: the orphan starts as `sh` and only then execs.
    async fn wait_running(pid: u32) {
        for _ in 0..250 {
            let mut sys = sysinfo::System::new();
            let spid = sysinfo::Pid::from_u32(pid);
            sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[spid]), true);
            let is_sleep = sys
                .process(spid)
                .and_then(|p| p.exe())
                .and_then(|exe| exe.file_name())
                .is_some_and(|name| name == "sleep");
            if is_sleep {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("pid {pid} never started sleep");
    }

    #[tokio::test]
    async fn stale_pid_of_unrelated_process_is_not_killed() {
        let tmp = tempfile::tempdir().unwrap();
        let pid = spawn_orphan("exec sleep 30");
        wait_running(pid).await;
        // A record without exe identity: the name check must reject `sleep`.
        write_info(tmp.path(), pid, None);
        let out = stop_recorded_daemon(tmp.path(), Duration::from_millis(200), KILL_WAIT).await;
        let alive = process_running(pid);
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status();
        assert!(alive, "an unrelated process must survive `ax daemon stop`");
        let out = out.expect("stale record is not an error");
        assert!(
            !out.starts_with("stopped"),
            "must not claim it stopped a daemon: {out}"
        );
        assert!(
            read_daemon_info(tmp.path()).is_none(),
            "stale record removed"
        );
    }

    #[tokio::test]
    async fn stale_pid_with_other_exe_identity_is_not_killed() {
        let tmp = tempfile::tempdir().unwrap();
        let pid = spawn_orphan("exec sleep 30");
        wait_running(pid).await;
        // The record names the ax binary; the pid now runs `sleep`.
        let ax_exe = ExeIdentity::current();
        assert!(ax_exe.is_some());
        write_info(tmp.path(), pid, ax_exe);
        let out = stop_recorded_daemon(tmp.path(), Duration::from_millis(200), KILL_WAIT).await;
        let alive = process_running(pid);
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status();
        assert!(alive, "an unrelated process must survive `ax daemon stop`");
        assert!(out.is_ok());
    }

    #[tokio::test]
    async fn identified_daemon_is_terminated_and_waited_for() {
        let tmp = tempfile::tempdir().unwrap();
        let pid = spawn_orphan("exec sleep 30");
        wait_running(pid).await;
        write_info(tmp.path(), pid, ExeIdentity::of(&exe_of(pid)));
        let out = stop_recorded_daemon(tmp.path(), Duration::from_secs(5), KILL_WAIT)
            .await
            .expect("stop succeeds");
        assert_eq!(out, format!("stopped daemon pid {pid}"));
        assert!(
            !process_running(pid),
            "stop returns only after the pid is gone"
        );
        assert!(read_daemon_info(tmp.path()).is_none());
    }

    #[tokio::test]
    async fn daemon_ignoring_sigterm_is_killed_after_grace() {
        let tmp = tempfile::tempdir().unwrap();
        let pid = spawn_orphan("trap '' TERM; exec sleep 30");
        wait_running(pid).await;
        write_info(tmp.path(), pid, ExeIdentity::of(&exe_of(pid)));
        let started = std::time::Instant::now();
        let out = stop_recorded_daemon(tmp.path(), Duration::from_millis(300), KILL_WAIT)
            .await
            .expect("stop succeeds");
        assert_eq!(out, format!("stopped daemon pid {pid}"));
        assert!(!process_running(pid));
        assert!(
            started.elapsed() >= Duration::from_millis(300),
            "SIGTERM came first"
        );
    }

    #[tokio::test]
    async fn dead_pid_is_reported_as_no_daemon() {
        let tmp = tempfile::tempdir().unwrap();
        let pid = spawn_orphan("exit 0");
        for _ in 0..100 {
            if !process_running(pid) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        write_info(tmp.path(), pid, ExeIdentity::current());
        let out = stop_recorded_daemon(tmp.path(), Duration::from_millis(200), KILL_WAIT)
            .await
            .unwrap();
        assert!(!out.starts_with("stopped"), "{out}");
        assert!(read_daemon_info(tmp.path()).is_none());
    }
}
