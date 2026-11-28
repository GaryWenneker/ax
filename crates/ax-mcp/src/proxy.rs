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
use crate::daemon_lock::{
    clear_stale_daemon_lock, is_daemon_pid, read_lock_info, release_daemon_lock, stop_daemon_pid,
    StopOutcome, DAEMON_STOP_GRACE,
};
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
        match attach_decision(&hello).await {
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
                if let Err(e) =
                    restart_serialized(project_root, RestartWhen::DaemonOlderThanMe).await
                {
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
/// Safe for Command Center: does **not** kill unrelated `ax.exe` processes (unlike `ax unlock`),
/// and only stops a pid that is the project's ax daemon.
pub async fn restart_daemon(project_root: &Path) -> Result<DaemonRestartReport, String> {
    restart_serialized(project_root, RestartWhen::Always).await
}

/// Whether a restart is still wanted once the restart lock is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestartWhen {
    /// An explicit restart (`ax daemon restart`, Command Center).
    Always,
    /// A proxy on a newer build: skip when the daemon now runs this build or a newer one,
    /// because another proxy restarted it first.
    DaemonOlderThanMe,
}

/// File in `.ax` that one restart holds for its whole stop-spawn-wait sequence.
const RESTART_LOCK_FILE: &str = "daemon-restart.lock";

/// How long a restart waits for another process's restart to finish.
const RESTART_LOCK_WAIT: Duration = Duration::from_secs(40);

/// Takes the exclusive restart lock, waiting up to `timeout`. The lock is released when
/// the returned file is dropped, or by the kernel when the process dies. The file is
/// never deleted, so every process locks the same inode.
fn acquire_restart_lock(project_root: &Path, timeout: Duration) -> std::io::Result<std::fs::File> {
    use fs2::FileExt;
    let ax_dir = get_ax_dir(project_root);
    std::fs::create_dir_all(&ax_dir)?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(ax_dir.join(RESTART_LOCK_FILE))?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(file),
            Err(e) if std::time::Instant::now() >= deadline => return Err(e),
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

/// How this proxy relates to the daemon that sent `hello`.
async fn attach_decision(hello: &DaemonHello) -> Attach {
    let mine = tokio::task::spawn_blocking(ExeIdentity::current)
        .await
        .ok()
        .flatten();
    decide(
        mine.as_ref(),
        env!("CARGO_PKG_VERSION"),
        hello.exe.as_ref(),
        &hello.ax,
    )
}

/// Restarts the daemon while holding the restart lock, so restarts from several
/// proxies or commands never stop each other's new daemon.
async fn restart_serialized(
    project_root: &Path,
    when: RestartWhen,
) -> Result<DaemonRestartReport, String> {
    let lock_root = project_root.to_path_buf();
    let _restart_lock =
        tokio::task::spawn_blocking(move || acquire_restart_lock(&lock_root, RESTART_LOCK_WAIT))
            .await
            .map_err(|e| format!("restart lock task failed: {e}"))?
            .map_err(|e| format!("another ax daemon restart is still running: {e}"))?;

    if when == RestartWhen::DaemonOlderThanMe {
        if let Some((_session, hello)) = connect_any(project_root).await {
            if attach_decision(&hello).await != Attach::RestartOnMine {
                return Ok(DaemonRestartReport {
                    ok: true,
                    stopped_pid: None,
                    started_pid: Some(hello.pid),
                    cleared_ax_lock: false,
                    connected: true,
                    hint: "The ax daemon already runs this build; it was not restarted.".into(),
                });
            }
        }
    }

    let pid_path = daemon_pid_path(project_root);
    let mut targets: Vec<(u32, Option<u64>)> = Vec::new();
    if let Some(info) = read_daemon_info(project_root) {
        targets.push((info.pid, info.started_at));
    }
    if let Some(lock) = read_lock_info(&pid_path) {
        if targets.iter().all(|(pid, _)| *pid != lock.pid) {
            targets.push((lock.pid, Some(lock.started_at)));
        }
    }
    let mut stopped_pid = None;
    let mut gone = Vec::new();
    for (pid, started_at) in targets {
        let outcome = tokio::task::spawn_blocking(move || {
            stop_daemon_pid(pid, started_at, DAEMON_STOP_GRACE)
        })
        .await
        .map_err(|e| format!("stop task failed: {e}"))?;
        match outcome {
            StopOutcome::Stopped | StopOutcome::Killed => {
                stopped_pid.get_or_insert(pid);
                gone.push(pid);
            }
            StopOutcome::NotRunning => gone.push(pid),
            StopOutcome::NotADaemon => {
                tracing::warn!("pid {pid} in the daemon files is not an ax daemon; left running");
                gone.push(pid);
            }
            StopOutcome::StillRunning => {
                return Err(format!("ax daemon pid {pid} did not stop"));
            }
        }
    }
    // Files of the daemon just stopped (or of one that is gone) go; a daemon that
    // started meanwhile keeps its own.
    if read_daemon_info(project_root)
        .is_some_and(|info| gone.contains(&info.pid) || !is_daemon_pid(info.pid, info.started_at))
    {
        remove_daemon_info(project_root);
    }
    if let Some(lock) = read_lock_info(&pid_path) {
        if gone.contains(&lock.pid) || !is_daemon_pid(lock.pid, Some(lock.started_at)) {
            release_daemon_lock(&pid_path);
        }
    } else {
        clear_stale_daemon_lock(&pid_path, None);
    }

    let lock_path = get_ax_dir(project_root).join("ax.lock");
    let cleared_ax_lock = ax_utils::clear_stale_lock(&lock_path);

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
        hint: "Shared MCP daemon restarted. If your IDE still shows DEGRADED, run MCP: Restart Servers (or reload the window). Prefer the daemon over parallel embedded MCP processes on the same .ax/ax.db.".into(),
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::daemon::{daemon_info_path, DaemonInfo};
    use crate::daemon_conn::DaemonSession;
    use tokio::net::TcpListener;

    /// A process that stands in for a daemon's pid; reaped when it exits.
    fn stand_in_pid() -> (u32, std::sync::Arc<std::sync::Mutex<std::process::Child>>) {
        let child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = child.id();
        (pid, std::sync::Arc::new(std::sync::Mutex::new(child)))
    }

    /// A fake daemon on TCP that answers every connection with a hello naming
    /// `pid` and this test binary as its executable (the caller's own build).
    async fn fake_daemon_on_my_build(root: &Path, pid: u32) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let info = DaemonInfo {
            pid,
            port,
            socket_path: None,
            version: env!("CARGO_PKG_VERSION").into(),
            project_root: root.to_string_lossy().into_owned(),
            exe: ExeIdentity::current(),
            token: None,
            started_at: None,
        };
        std::fs::write(
            daemon_info_path(root),
            serde_json::to_string(&info).unwrap(),
        )
        .unwrap();
        let hello = DaemonHello {
            kind: "hello".into(),
            pid,
            ax: env!("CARGO_PKG_VERSION").into(),
            project: root.to_string_lossy().into_owned(),
            socket_path: None,
            port,
            exe: ExeIdentity::current(),
        };
        let line = serde_json::to_string(&hello).unwrap() + "\n";
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let mut session = DaemonSession::from_tcp(stream);
                let _ = session.write_bytes(line.as_bytes()).await;
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        });
    }

    fn project() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        std::fs::create_dir_all(get_ax_dir(&root)).unwrap();
        (dir, root)
    }

    #[tokio::test]
    async fn a_proxy_restart_is_skipped_when_another_proxy_already_restarted_on_this_build() {
        let (_dir, root) = project();
        let (pid, child) = stand_in_pid();
        fake_daemon_on_my_build(&root, pid).await;
        let report = restart_serialized(&root, RestartWhen::DaemonOlderThanMe)
            .await
            .expect("the restart was not skipped");
        assert_eq!(report.stopped_pid, None);
        assert_eq!(report.started_pid, Some(pid));
        assert!(
            is_pid_alive_now(pid),
            "the daemon that already ran this build was stopped"
        );
        let _ = child.lock().unwrap().kill();
        let _ = child.lock().unwrap().wait();
    }

    #[tokio::test]
    async fn a_restart_waits_while_another_restart_holds_the_restart_lock() {
        let (_dir, root) = project();
        let (pid, child) = stand_in_pid();
        fake_daemon_on_my_build(&root, pid).await;
        let held = acquire_restart_lock(&root, Duration::from_secs(1)).unwrap();
        let task_root = root.clone();
        let restart = tokio::spawn(async move {
            restart_serialized(&task_root, RestartWhen::DaemonOlderThanMe).await
        });
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(
            !restart.is_finished(),
            "a restart ran while another one held the lock"
        );
        drop(held);
        let report = tokio::time::timeout(Duration::from_secs(5), restart)
            .await
            .expect("the restart did not continue after the lock was released")
            .unwrap()
            .unwrap();
        assert_eq!(report.stopped_pid, None);
        let _ = child.lock().unwrap().kill();
        let _ = child.lock().unwrap().wait();
    }

    fn is_pid_alive_now(pid: u32) -> bool {
        crate::daemon_lock::is_pid_alive(pid)
    }
}
