//! Daemon pid lockfile — CG: mcp/daemon.ts tryAcquireDaemonLock, daemon-paths.ts.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::daemon_paths::{daemon_pid_path, primary_socket_path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonLockInfo {
    pub pid: u32,
    pub version: String,
    pub socket_path: String,
    pub started_at: u64,
}

pub enum AcquireResult {
    Acquired {
        pid_path: PathBuf,
        info: DaemonLockInfo,
    },
    Taken {
        existing: Option<DaemonLockInfo>,
        pid_path: PathBuf,
    },
}

pub fn read_lock_info(pid_path: &Path) -> Option<DaemonLockInfo> {
    let raw = fs::read_to_string(pid_path).ok()?;
    serde_json::from_str(raw.trim()).ok()
}

/// Publishes this process's lock unless a live daemon holds it.
///
/// Clearing a stale lock and publishing a new one run under the acquire guard
/// ([`guard_path`]). Without it, an acquirer that judged the lock stale (or
/// absent) could delete a lock another acquirer published in between, and both
/// would run as the daemon.
pub fn try_acquire_daemon_lock(project_root: &Path) -> std::io::Result<AcquireResult> {
    let pid_path = daemon_pid_path(project_root);
    if let Some(parent) = pid_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let _guard = lock_guard(&pid_path)?;
    clear_stale_unguarded(&pid_path, None);
    let info = DaemonLockInfo {
        pid: std::process::id(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        socket_path: primary_socket_path(project_root),
        started_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
    };
    let body = serde_json::to_string_pretty(&info)? + "\n";
    if publish_lock(&pid_path, body.as_bytes())? {
        Ok(AcquireResult::Acquired { pid_path, info })
    } else {
        let existing = read_lock_info(&pid_path);
        Ok(AcquireResult::Taken { existing, pid_path })
    }
}

/// A lock file that cannot be parsed is only treated as stale after this long:
/// before that it may belong to a daemon that is still starting.
const UNREADABLE_LOCK_GRACE: Duration = Duration::from_secs(5);

/// The file next to `pid_path` whose OS lock serializes acquirers and stale-lock
/// clearing. It is never deleted, so every process locks the same inode.
fn guard_path(pid_path: &Path) -> PathBuf {
    let name = pid_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "daemon.pid".into());
    pid_path.with_file_name(format!("{name}.guard"))
}

/// Holds the acquire guard of `pid_path` until the returned file is dropped.
/// The OS releases it when the holder dies, so a crash never blocks anyone.
fn lock_guard(pid_path: &Path) -> std::io::Result<fs::File> {
    use fs2::FileExt;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(guard_path(pid_path))?;
    file.lock_exclusive()?;
    Ok(file)
}

/// A temporary file next to `path`, unique per process and call.
fn temp_sibling(path: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "daemon.pid".into());
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{name}.{}.{n}.tmp", std::process::id()))
}

/// Creates `pid_path` with `body` unless it exists. Returns false when it exists.
///
/// The body is written to a temporary file first and published with a hard
/// link, which fails when the target exists, so no other process ever sees
/// the lock without its contents.
fn publish_lock(pid_path: &Path, body: &[u8]) -> std::io::Result<bool> {
    let tmp = temp_sibling(pid_path);
    let staged = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(body)?;
        file.sync_all()
    })();
    if let Err(e) = staged {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    let linked = fs::hard_link(&tmp, pid_path);
    let _ = fs::remove_file(&tmp);
    match linked {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        // A file system without hard links: create the file, then fill it.
        // A reader that sees it empty keeps it for UNREADABLE_LOCK_GRACE.
        Err(_) => match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(pid_path)
        {
            Ok(mut file) => {
                file.write_all(body)?;
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
            Err(e) => Err(e),
        },
    }
}

/// True when `path` was modified less than [`UNREADABLE_LOCK_GRACE`] ago (or its
/// age cannot be told).
fn is_recent(path: &Path) -> bool {
    let Ok(modified) = fs::metadata(path).and_then(|m| m.modified()) else {
        return false;
    };
    match SystemTime::now().duration_since(modified) {
        Ok(age) => age < UNREADABLE_LOCK_GRACE,
        Err(_) => true,
    }
}

/// Records the socket path in this process's lock. A lock that belongs to
/// another process is left alone.
pub fn rewrite_lock_socket_path(pid_path: &Path, socket_path: &str) -> std::io::Result<()> {
    let Some(mut info) = read_lock_info(pid_path) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "daemon lock is missing or unreadable",
        ));
    };
    if info.pid != std::process::id() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "daemon lock belongs to another process",
        ));
    }
    info.socket_path = socket_path.to_string();
    let body = serde_json::to_string_pretty(&info)? + "\n";
    ax_utils::atomic_write(pid_path, body.as_bytes())
}

/// Removes `pid_path` when no live daemon holds it, under the acquire guard.
pub fn clear_stale_daemon_lock(pid_path: &Path, expected_pid: Option<u32>) {
    if !pid_path.exists() {
        return;
    }
    match lock_guard(pid_path) {
        Ok(_guard) => clear_stale_unguarded(pid_path, expected_pid),
        Err(e) => tracing::warn!("daemon lock guard unavailable; stale lock kept: {e}"),
    }
}

/// [`clear_stale_daemon_lock`] for a caller that holds the acquire guard.
fn clear_stale_unguarded(pid_path: &Path, expected_pid: Option<u32>) {
    if let Some(existing) = read_lock_info(pid_path) {
        if expected_pid.is_some_and(|p| p != existing.pid) {
            return;
        }
        if is_daemon_pid(existing.pid, Some(existing.started_at)) {
            return;
        }
    } else if let Some(pid) = expected_pid {
        if is_pid_alive(pid) {
            return;
        }
    } else if is_recent(pid_path) {
        return;
    }
    let _ = fs::remove_file(pid_path);
}

/// Removes the lock whatever process it belongs to (`ax daemon stop`).
pub fn release_daemon_lock(pid_path: &Path) {
    let _ = fs::remove_file(pid_path);
}

/// Removes the lock only when it belongs to `pid`.
pub fn release_daemon_lock_if_owned(pid_path: &Path, pid: u32) {
    if read_lock_info(pid_path).is_some_and(|info| info.pid == pid) {
        let _ = fs::remove_file(pid_path);
    }
}

pub fn is_pid_alive(pid: u32) -> bool {
    use sysinfo::{Pid, System};
    let Ok(mut sys) = System::new() else {
        return false;
    };
    sys.refresh_processes(
        sysinfo::ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
        true,
    );
    sys.process(Pid::from_u32(pid)).is_some_and(is_running)
}

/// False for a process that has exited but was not reaped yet (a zombie). Linux
/// still lists it; it runs nothing, holds no lock and cannot be stopped.
fn is_running(process: &sysinfo::Process) -> bool {
    !matches!(
        process.status(),
        sysinfo::ProcessStatus::Zombie | sysinfo::ProcessStatus::Dead
    )
}

/// Whole seconds a process start time (which sysinfo reports in seconds) may lie
/// after the time the daemon wrote into its file and still be that daemon.
const START_SLACK_SECS: u64 = 2;

/// True when `pid` is alive and can be the ax daemon that wrote a file stamped
/// `started_at_ms` (milliseconds since the epoch; `None` or 0 when unknown).
///
/// A pid alone is not enough: after a crash or reboot the pid in `daemon.json`
/// or `daemon.pid` can belong to any other process. The process must run an
/// `ax` binary (or this binary) and must have started before the file was written.
pub fn is_daemon_pid(pid: u32, started_at_ms: Option<u64>) -> bool {
    use sysinfo::{Pid, ProcessRefreshKind, System, UpdateKind};
    // No process table: nothing can be shown to be the daemon.
    let Ok(mut sys) = System::new() else {
        return false;
    };
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::Always),
    );
    let Some(process) = sys.process(Pid::from_u32(pid)).filter(|p| is_running(p)) else {
        return false;
    };
    let exe_stem = process
        .exe()
        .and_then(|p| p.file_stem())
        .or_else(|| Path::new(process.name()).file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    could_be_daemon(&exe_stem, process.start_time(), started_at_ms)
}

/// The decision behind [`is_daemon_pid`], given the process's executable stem and
/// start time (seconds since the epoch).
fn could_be_daemon(exe_stem: &str, process_start_secs: u64, started_at_ms: Option<u64>) -> bool {
    let own_stem = crate::exe_identity::current_exe_path()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()));
    let is_ax = exe_stem.eq_ignore_ascii_case("ax")
        || own_stem.is_some_and(|own| own.eq_ignore_ascii_case(exe_stem));
    let started_in_time = match started_at_ms.filter(|ms| *ms > 0) {
        Some(ms) => process_start_secs <= ms / 1000 + START_SLACK_SECS,
        None => true,
    };
    is_ax && started_in_time
}

/// What [`stop_daemon_pid`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopOutcome {
    /// No process with that pid runs.
    NotRunning,
    /// A process runs with that pid, but it is not the ax daemon: it was left alone.
    NotADaemon,
    /// The daemon exited after the polite stop request.
    Stopped,
    /// The daemon ignored the stop request and was force-killed.
    Killed,
    /// The daemon still runs after the force-kill.
    StillRunning,
}

impl StopOutcome {
    /// The daemon with that pid no longer runs.
    pub fn is_gone(self) -> bool {
        matches!(self, Self::NotRunning | Self::Stopped | Self::Killed)
    }
}

/// How long [`stop_daemon_pid`] waits after SIGTERM before it force-kills.
pub const DAEMON_STOP_GRACE: Duration = Duration::from_secs(5);

/// Stops the ax daemon with `pid`: checks that the pid is that daemon
/// ([`is_daemon_pid`]), asks it to stop (SIGTERM, so it removes its files and
/// records a clean shutdown), waits up to `grace`, then force-kills it.
pub fn stop_daemon_pid(pid: u32, started_at_ms: Option<u64>, grace: Duration) -> StopOutcome {
    stop_pid_with(pid, grace, |pid| is_daemon_pid(pid, started_at_ms))
}

fn stop_pid_with(pid: u32, grace: Duration, is_ours: impl Fn(u32) -> bool) -> StopOutcome {
    if !is_pid_alive(pid) {
        return StopOutcome::NotRunning;
    }
    if !is_ours(pid) {
        return StopOutcome::NotADaemon;
    }
    if pid == std::process::id() {
        return StopOutcome::NotADaemon;
    }
    let _ = term_pid(pid);
    if wait_until_gone(pid, grace) {
        return StopOutcome::Stopped;
    }
    let _ = kill_pid(pid);
    if wait_until_gone(pid, Duration::from_secs(2)) {
        StopOutcome::Killed
    } else {
        StopOutcome::StillRunning
    }
}

fn wait_until_gone(pid: u32, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if !is_pid_alive(pid) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Ask a process to stop (Unix SIGTERM, Windows `taskkill` without `/F`).
fn term_pid(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

/// Force-stop a process by PID (Windows `taskkill` / Unix `kill -9`).
pub fn kill_pid(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lock_root() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join(".ax")).unwrap();
        (dir, root)
    }

    #[test]
    fn a_lock_still_being_written_is_not_cleared_as_stale() {
        let (_dir, root) = lock_root();
        let pid_path = daemon_pid_path(&root);
        fs::create_dir_all(pid_path.parent().unwrap()).unwrap();
        fs::write(&pid_path, "").unwrap();
        clear_stale_daemon_lock(&pid_path, None);
        assert!(pid_path.exists(), "a fresh empty lock was deleted as stale");
        match try_acquire_daemon_lock(&root).unwrap() {
            AcquireResult::Taken { existing: None, .. } => {}
            _ => panic!("a fresh empty lock must stay taken"),
        }
    }

    #[test]
    fn an_old_unreadable_lock_is_cleared() {
        let (_dir, root) = lock_root();
        let pid_path = daemon_pid_path(&root);
        fs::create_dir_all(pid_path.parent().unwrap()).unwrap();
        fs::write(&pid_path, "").unwrap();
        let old = SystemTime::now() - Duration::from_secs(60);
        fs::File::options()
            .write(true)
            .open(&pid_path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        match try_acquire_daemon_lock(&root).unwrap() {
            AcquireResult::Acquired { info, .. } => assert_eq!(info.pid, std::process::id()),
            AcquireResult::Taken { .. } => panic!("an old unreadable lock kept the daemon out"),
        }
        assert_eq!(read_lock_info(&pid_path).unwrap().pid, std::process::id());
    }

    #[test]
    fn a_lock_of_a_dead_pid_is_cleared_and_a_live_one_is_kept() {
        let (_dir, root) = lock_root();
        let pid_path = daemon_pid_path(&root);
        let mut child = std::process::Command::new(if cfg!(windows) { "cmd" } else { "true" })
            .args(if cfg!(windows) {
                vec!["/C", "exit"]
            } else {
                vec![]
            })
            .spawn()
            .unwrap();
        let dead = child.id();
        child.wait().unwrap();
        let write = |pid: u32| {
            let info = DaemonLockInfo {
                pid,
                version: "test".into(),
                socket_path: "sock".into(),
                started_at: 0,
            };
            fs::write(&pid_path, serde_json::to_string(&info).unwrap()).unwrap();
        };
        write(dead);
        clear_stale_daemon_lock(&pid_path, None);
        assert!(!pid_path.exists(), "the lock of a dead daemon was kept");
        write(std::process::id());
        clear_stale_daemon_lock(&pid_path, None);
        assert!(pid_path.exists(), "the lock of a live daemon was cleared");
    }

    #[test]
    fn release_and_rewrite_only_touch_this_process_lock() {
        let (_dir, root) = lock_root();
        let pid_path = daemon_pid_path(&root);
        let other = DaemonLockInfo {
            pid: 1,
            version: "test".into(),
            socket_path: "theirs".into(),
            started_at: 0,
        };
        fs::write(&pid_path, serde_json::to_string(&other).unwrap()).unwrap();
        assert!(rewrite_lock_socket_path(&pid_path, "mine").is_err());
        release_daemon_lock_if_owned(&pid_path, std::process::id());
        let kept = read_lock_info(&pid_path).expect("lock kept");
        assert_eq!((kept.pid, kept.socket_path.as_str()), (1, "theirs"));

        fs::remove_file(&pid_path).unwrap();
        let AcquireResult::Acquired { pid_path, .. } = try_acquire_daemon_lock(&root).unwrap()
        else {
            panic!("expected acquired");
        };
        rewrite_lock_socket_path(&pid_path, "mine").unwrap();
        assert_eq!(read_lock_info(&pid_path).unwrap().socket_path, "mine");
        release_daemon_lock_if_owned(&pid_path, std::process::id());
        assert!(!pid_path.exists());
        let leftovers: Vec<_> = fs::read_dir(pid_path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temporary files left: {leftovers:?}");
    }

    #[test]
    fn concurrent_acquirers_get_exactly_one_lock() {
        for _ in 0..40 {
            let (_dir, root) = lock_root();
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
            let threads: Vec<_> = (0..8)
                .map(|_| {
                    let root = root.clone();
                    let barrier = std::sync::Arc::clone(&barrier);
                    std::thread::spawn(move || {
                        barrier.wait();
                        match try_acquire_daemon_lock(&root).unwrap() {
                            AcquireResult::Acquired { .. } => (1, 0),
                            AcquireResult::Taken { existing, .. } => {
                                (0, usize::from(existing.is_none()))
                            }
                        }
                    })
                })
                .collect();
            let (acquired, unreadable) = threads
                .into_iter()
                .map(|t| t.join().unwrap())
                .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
            assert_eq!(acquired, 1, "daemons that acquired the same lock");
            assert_eq!(unreadable, 0, "a taken lock was seen without its contents");
        }
    }

    /// An acquirer waits while another one holds the acquire guard, and then
    /// sees the lock that one published instead of clearing it.
    #[test]
    fn an_acquirer_waits_for_the_guard_and_keeps_the_lock_published_under_it() {
        use fs2::FileExt;
        let (_dir, root) = lock_root();
        let pid_path = daemon_pid_path(&root);
        fs::create_dir_all(pid_path.parent().unwrap()).unwrap();
        let guard = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(guard_path(&pid_path))
            .unwrap();
        guard.lock_exclusive().unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        let acquirer_root = root.clone();
        let acquirer = std::thread::spawn(move || {
            let result = try_acquire_daemon_lock(&acquirer_root).unwrap();
            tx.send(()).unwrap();
            result
        });
        assert!(
            rx.recv_timeout(Duration::from_millis(300)).is_err(),
            "an acquirer ran while another held the guard"
        );
        // What the guard holder publishes before it lets go must survive.
        let theirs = DaemonLockInfo {
            pid: std::process::id(),
            version: "test".into(),
            socket_path: "theirs".into(),
            started_at: now_ms(),
        };
        fs::write(&pid_path, serde_json::to_string(&theirs).unwrap()).unwrap();
        FileExt::unlock(&guard).unwrap();
        match acquirer.join().unwrap() {
            AcquireResult::Taken {
                existing: Some(info),
                ..
            } => assert_eq!(info.socket_path, "theirs"),
            _ => panic!("the acquirer replaced a lock published under the guard"),
        }
    }

    /// A process that exited but was not reaped yet is not running, so it holds
    /// no lock and needs no stopping.
    #[cfg(unix)]
    #[test]
    fn a_zombie_process_is_not_alive_and_not_a_daemon() {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        // Not waited on: once it exits it stays a zombie until `wait` below.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !is_zombie(pid) {
            assert!(
                std::time::Instant::now() < deadline,
                "child never became a zombie"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!is_pid_alive(pid), "a zombie counted as a live process");
        assert!(!is_daemon_pid(pid, None), "a zombie counted as the daemon");
        let started = std::time::Instant::now();
        assert_eq!(
            stop_pid_with(pid, Duration::from_secs(5), |_| true),
            StopOutcome::NotRunning
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        child.wait().unwrap();
    }

    /// True when `ps` reports `pid` as a zombie (state `Z`).
    #[cfg(unix)]
    fn is_zombie(pid: u32) -> bool {
        std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim_start()
                    .starts_with('Z')
            })
            .unwrap_or(false)
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// A child process that is reaped as soon as it exits, so it is not seen as a
    /// live zombie.
    #[cfg(unix)]
    fn reaped_child(script: &str) -> u32 {
        let mut child = std::process::Command::new("sh")
            .args(["-c", script])
            .spawn()
            .unwrap();
        let pid = child.id();
        std::thread::spawn(move || child.wait());
        // Give the shell time to install its traps.
        std::thread::sleep(Duration::from_millis(300));
        pid
    }

    #[test]
    fn a_live_pid_that_started_after_the_lock_is_not_the_daemon() {
        let me = std::process::id();
        assert!(
            is_daemon_pid(me, Some(now_ms())),
            "this process, lock written now"
        );
        let a_day_ago = now_ms() - 24 * 3600 * 1000;
        assert!(
            !is_daemon_pid(me, Some(a_day_ago)),
            "a pid that started after its lock was written is a recycled pid"
        );
    }

    #[test]
    fn only_an_ax_binary_can_be_the_daemon() {
        let now = 2_000_000;
        assert!(could_be_daemon("ax", now, Some(now * 1000)));
        assert!(could_be_daemon("AX", now, None));
        assert!(!could_be_daemon("Code Helper", now, Some(now * 1000)));
        assert!(!could_be_daemon("sleep", now, None));
        assert!(!could_be_daemon("ax", now + 60, Some(now * 1000)));
        assert!(
            could_be_daemon("ax", now + 1, Some(now * 1000)),
            "start time is rounded to seconds"
        );
    }

    #[cfg(unix)]
    #[test]
    fn stop_leaves_a_process_that_is_not_the_daemon_alone() {
        let pid = reaped_child("sleep 30");
        assert_eq!(
            stop_daemon_pid(pid, Some(now_ms()), Duration::from_millis(200)),
            StopOutcome::NotADaemon
        );
        assert!(is_pid_alive(pid), "a process that is not ax was stopped");
        let _ = kill_pid(pid);
    }

    #[cfg(unix)]
    #[test]
    fn stop_asks_politely_first_and_then_force_kills() {
        let polite = reaped_child("trap 'exit 0' TERM; while true; do sleep 0.05; done");
        assert_eq!(
            stop_pid_with(polite, Duration::from_secs(5), |_| true),
            StopOutcome::Stopped
        );
        let stubborn = reaped_child("trap '' TERM; while true; do sleep 0.05; done");
        assert_eq!(
            stop_pid_with(stubborn, Duration::from_millis(300), |_| true),
            StopOutcome::Killed
        );
        assert!(!is_pid_alive(stubborn));
    }

    #[test]
    fn a_lock_whose_pid_was_recycled_is_cleared() {
        let (_dir, root) = lock_root();
        let pid_path = daemon_pid_path(&root);
        let info = DaemonLockInfo {
            pid: std::process::id(),
            version: "test".into(),
            socket_path: "sock".into(),
            started_at: now_ms() - 24 * 3600 * 1000,
        };
        fs::write(&pid_path, serde_json::to_string(&info).unwrap()).unwrap();
        clear_stale_daemon_lock(&pid_path, None);
        assert!(
            !pid_path.exists(),
            "a lock of a recycled pid kept new daemons out"
        );
    }

    #[test]
    fn acquire_and_release_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("ax-lock-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join(".ax")).unwrap();
        let root = tmp.clone();
        let r = try_acquire_daemon_lock(&root).unwrap();
        match r {
            AcquireResult::Acquired { pid_path, .. } => {
                release_daemon_lock(&pid_path);
            }
            AcquireResult::Taken { .. } => panic!("expected acquired"),
        }
        let _ = fs::remove_dir_all(&tmp);
    }
}
