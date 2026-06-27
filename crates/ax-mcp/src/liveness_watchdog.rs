//! Main-thread liveness watchdog — CG: mcp/liveness-watchdog.ts (#850).
//!
//! Spawns a child process that kills the parent if heartbeats stop (wedged main thread).

use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sysinfo::{Pid, ProcessesToUpdate, System};

pub const DEFAULT_WATCHDOG_TIMEOUT_MS: u64 = 60_000;
pub const NO_WATCHDOG_ENV: &str = "AX_NO_WATCHDOG";
pub const WATCHDOG_TIMEOUT_ENV: &str = "AX_WATCHDOG_TIMEOUT_MS";

pub struct WatchdogHandle {
    stop: Arc<AtomicBool>,
    child: Option<Child>,
    heartbeat: Option<JoinHandle<()>>,
}

impl WatchdogHandle {
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.heartbeat.take() {
            let _ = h.join();
        }
        if let Some(mut child) = self.child.take() {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.flush();
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for WatchdogHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn is_env_truthy(raw: Option<String>) -> bool {
    match raw {
        None => false,
        Some(s) => matches!(
            s.trim().to_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
    }
}

pub fn parse_watchdog_timeout_ms(raw: Option<String>) -> u64 {
    match raw {
        None => DEFAULT_WATCHDOG_TIMEOUT_MS,
        Some(s) if s.trim().is_empty() => DEFAULT_WATCHDOG_TIMEOUT_MS,
        Some(s) => {
            let parsed = s.parse::<f64>().unwrap_or(-1.0);
            if !parsed.is_finite() || parsed <= 0.0 {
                DEFAULT_WATCHDOG_TIMEOUT_MS
            } else {
                parsed.floor() as u64
            }
        }
    }
}

pub fn derive_check_interval_ms(timeout_ms: u64) -> u64 {
    let fifth = (timeout_ms / 5).max(50);
    fifth.min(2000)
}

fn kill_process(pid: u32) {
    if pid <= 1 {
        return;
    }
    let pid = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    if let Some(p) = sys.process(pid) {
        p.kill();
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Child entry: read stdin heartbeats; kill parent when they stop.
pub fn run_watchdog_child(parent_pid: u32, timeout_ms: u64) {
    use std::io::Read;
    use std::sync::atomic::AtomicU64;

    let last_beat = Arc::new(AtomicU64::new(now_ms()));
    let beat_ref = Arc::clone(&last_beat);

    thread::spawn(move || {
        let mut buf = [0u8; 64];
        let mut stdin = std::io::stdin().lock();
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => std::process::exit(0),
                Ok(_) => beat_ref.store(now_ms(), Ordering::SeqCst),
                Err(_) => std::process::exit(0),
            }
        }
    });

    let poll = Duration::from_millis(derive_check_interval_ms(timeout_ms).max(50));
    loop {
        thread::sleep(poll);
        let elapsed = now_ms() - last_beat.load(Ordering::SeqCst);
        if elapsed >= timeout_ms {
            let secs = timeout_ms / 1000;
            eprintln!(
                "[ax] Main thread unresponsive for ~{secs}s — killing wedged process. Disable with AX_NO_WATCHDOG=1."
            );
            kill_process(parent_pid);
            std::process::exit(0);
        }
    }
}

fn spawn_watchdog_child(timeout_ms: u64) -> Option<(Child, ChildStdin)> {
    let exe = std::env::current_exe().ok()?;
    let parent_pid = std::process::id();
    let mut child = Command::new(exe)
        .arg("watchdog-child")
        .arg(parent_pid.to_string())
        .arg(timeout_ms.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .current_dir(std::env::temp_dir())
        .spawn()
        .ok()?;
    let stdin = child.stdin.take()?;
    Some((child, stdin))
}

/// Install liveness watchdog for a long-lived MCP/daemon process.
pub fn install_main_thread_watchdog() -> Option<WatchdogHandle> {
    if is_env_truthy(std::env::var(NO_WATCHDOG_ENV).ok()) {
        return None;
    }

    let timeout_ms = parse_watchdog_timeout_ms(std::env::var(WATCHDOG_TIMEOUT_ENV).ok());
    let check_ms = derive_check_interval_ms(timeout_ms);

    let (child, stdin) = spawn_watchdog_child(timeout_ms)?;
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop);

    let heartbeat = thread::spawn(move || {
        let mut stdin = stdin;
        let interval = Duration::from_millis(check_ms);
        while !stop_flag.load(Ordering::SeqCst) {
            let start = Instant::now();
            let _ = stdin.write_all(b"\n");
            let _ = stdin.flush();
            while start.elapsed() < interval && !stop_flag.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(50));
            }
        }
    });

    Some(WatchdogHandle {
        stop,
        child: Some(child),
        heartbeat: Some(heartbeat),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_timeout_defaults() {
        assert_eq!(parse_watchdog_timeout_ms(None), DEFAULT_WATCHDOG_TIMEOUT_MS);
        assert_eq!(parse_watchdog_timeout_ms(Some("0".into())), DEFAULT_WATCHDOG_TIMEOUT_MS);
        assert_eq!(parse_watchdog_timeout_ms(Some("5000".into())), 5000);
    }

    #[test]
    fn derive_check_interval() {
        assert_eq!(derive_check_interval_ms(60_000), 2000);
        assert_eq!(derive_check_interval_ms(100), 50);
    }

    #[test]
    fn no_watchdog_env() {
        assert!(!is_env_truthy(None));
        assert!(is_env_truthy(Some("1".into())));
        assert!(is_env_truthy(Some("TRUE".into())));
        assert!(!is_env_truthy(Some("0".into())));
    }
}