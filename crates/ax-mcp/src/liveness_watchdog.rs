//! Main-thread liveness watchdog — CG: mcp/liveness-watchdog.ts (#850).
//!
//! Spawns a child process that kills the parent if heartbeats stop.
//!
//! A heartbeat thread writes the beats, but only while a task on the tokio runtime
//! keeps ticking: a runtime that can no longer run tasks stops the beats, and the
//! child kills the process after the timeout.

use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessesToUpdate, System};

pub const DEFAULT_WATCHDOG_TIMEOUT_MS: u64 = 60_000;
pub const NO_WATCHDOG_ENV: &str = "AX_NO_WATCHDOG";
pub const WATCHDOG_TIMEOUT_ENV: &str = "AX_WATCHDOG_TIMEOUT_MS";

pub struct WatchdogHandle {
    stop: Arc<AtomicBool>,
    ticker: Option<tokio::task::JoinHandle<()>>,
    child: Option<Child>,
    heartbeat: Option<JoinHandle<()>>,
}

impl WatchdogHandle {
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.ticker.take() {
            t.abort();
        }
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

fn beat_age_ms(last_beat_ms: u64, now_ms: u64) -> u64 {
    now_ms.saturating_sub(last_beat_ms)
}

/// Last heartbeat time on the monotonic clock, as milliseconds since `start`.
///
/// A wall-clock jump (system sleep, NTP or VM time correction) does not change the
/// age of a beat: only time this process was running counts.
struct BeatClock {
    start: Instant,
    last_ms: std::sync::atomic::AtomicU64,
}

impl BeatClock {
    fn new(start: Instant) -> Self {
        Self {
            start,
            last_ms: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn millis_at(&self, at: Instant) -> u64 {
        at.saturating_duration_since(self.start).as_millis() as u64
    }

    fn record_at(&self, at: Instant) {
        self.last_ms.store(self.millis_at(at), Ordering::SeqCst);
    }

    fn age_at(&self, at: Instant) -> u64 {
        beat_age_ms(self.last_ms.load(Ordering::SeqCst), self.millis_at(at))
    }
}

/// True when one poll sleep took far longer than asked, so the beat age says
/// nothing about the parent.
fn was_suspended(slept: Duration, poll: Duration) -> bool {
    slept > poll * 5 + Duration::from_secs(1)
}

/// Child entry: read stdin heartbeats; kill parent when they stop.
pub fn run_watchdog_child(parent_pid: u32, timeout_ms: u64) {
    use std::io::Read;

    let clock = Arc::new(BeatClock::new(Instant::now()));
    let beat_ref = Arc::clone(&clock);

    thread::spawn(move || {
        let mut buf = [0u8; 64];
        let mut stdin = std::io::stdin().lock();
        loop {
            match stdin.read(&mut buf) {
                Ok(0) => std::process::exit(0),
                Ok(_) => beat_ref.record_at(Instant::now()),
                Err(_) => std::process::exit(0),
            }
        }
    });

    let poll = Duration::from_millis(derive_check_interval_ms(timeout_ms).max(50));
    loop {
        let before = Instant::now();
        thread::sleep(poll);
        let now = Instant::now();
        if was_suspended(now.saturating_duration_since(before), poll) {
            // The watchdog itself did not run (system sleep on a clock that counts
            // it, or a stopped process): give the parent a full timeout to beat again.
            clock.record_at(now);
            continue;
        }
        let elapsed = clock.age_at(now);
        if elapsed >= timeout_ms {
            let secs = timeout_ms / 1000;
            eprintln!(
                "{}",
                ax_usage::format_ax_tagged(format!(
                    "Main thread unresponsive for ~{secs}s — killing wedged process. Disable with AX_NO_WATCHDOG=1."
                ))
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

/// Writes one beat per interval to the watchdog child until `stop` is set.
///
/// With `progress`, a beat is sent only when the counter moved since the last
/// beat, so the beats stop when whatever bumps the counter is stuck.
fn spawn_heartbeat<W: Write + Send + 'static>(
    mut out: W,
    interval: Duration,
    stop_flag: Arc<AtomicBool>,
    progress: Option<Arc<AtomicU64>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut last_seen: Option<u64> = None;
        while !stop_flag.load(Ordering::SeqCst) {
            let start = Instant::now();
            let tick = progress.as_ref().map(|p| p.load(Ordering::SeqCst));
            if tick.is_none() || tick != last_seen {
                last_seen = tick;
                let _ = out.write_all(b"\n");
                let _ = out.flush();
            }
            while start.elapsed() < interval && !stop_flag.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(10));
            }
        }
    })
}

/// Bumps `progress` from a task on the tokio runtime, so the heartbeat stops when
/// the runtime can no longer run tasks.
async fn tick_runtime(progress: Arc<AtomicU64>, period: Duration) {
    loop {
        progress.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(period).await;
    }
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

    let interval = Duration::from_millis(check_ms);
    let (ticker, progress) = match tokio::runtime::Handle::try_current() {
        Ok(runtime) => {
            let progress = Arc::new(AtomicU64::new(0));
            let task = runtime.spawn(tick_runtime(Arc::clone(&progress), interval / 2));
            (Some(task), Some(progress))
        }
        Err(_) => (None, None),
    };
    let heartbeat = spawn_heartbeat(stdin, interval, Arc::clone(&stop), progress);

    Some(WatchdogHandle {
        stop,
        ticker,
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
        assert_eq!(
            parse_watchdog_timeout_ms(Some("0".into())),
            DEFAULT_WATCHDOG_TIMEOUT_MS
        );
        assert_eq!(parse_watchdog_timeout_ms(Some("5000".into())), 5000);
    }

    #[test]
    fn derive_check_interval() {
        assert_eq!(derive_check_interval_ms(60_000), 2000);
        assert_eq!(derive_check_interval_ms(100), 50);
    }

    #[test]
    fn beat_age_never_underflows_when_the_clock_steps_back() {
        assert_eq!(beat_age_ms(10_000, 4_000), 0);
        assert_eq!(beat_age_ms(4_000, 10_000), 6_000);
    }

    #[test]
    fn beat_clock_measures_age_on_the_monotonic_clock() {
        let start = Instant::now();
        let clock = BeatClock::new(start);
        clock.record_at(start + Duration::from_millis(5_000));
        assert_eq!(clock.age_at(start + Duration::from_millis(5_000)), 0);
        assert_eq!(clock.age_at(start + Duration::from_millis(7_500)), 2_500);
        // A check that observes an earlier instant than the beat never underflows.
        assert_eq!(clock.age_at(start + Duration::from_millis(1_000)), 0);
        assert_eq!(clock.age_at(start), 0);
    }

    #[test]
    fn a_poll_that_overslept_counts_as_a_suspension() {
        let poll = Duration::from_millis(2_000);
        assert!(!was_suspended(Duration::from_millis(2_050), poll));
        assert!(!was_suspended(Duration::from_millis(9_000), poll));
        assert!(was_suspended(Duration::from_secs(600), poll));
    }

    /// Collects what the heartbeat writes, as the watchdog child would read it.
    #[derive(Clone, Default)]
    struct Beats(Arc<std::sync::Mutex<Vec<u8>>>);

    impl Write for Beats {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Beats {
        fn count(&self) -> usize {
            self.0.lock().unwrap().len()
        }
    }

    #[test]
    fn heartbeat_stops_when_the_watched_runtime_makes_no_progress() {
        let beats = Beats::default();
        let stop = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicU64::new(0));
        let handle = spawn_heartbeat(
            beats.clone(),
            Duration::from_millis(20),
            Arc::clone(&stop),
            Some(Arc::clone(&progress)),
        );
        // The runtime ticks: every tick allows a beat.
        for tick in 1..=3 {
            progress.store(tick, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(60));
        }
        let while_alive = beats.count();
        assert!(
            while_alive >= 2,
            "beats while the runtime ticks: {while_alive}"
        );
        // The runtime is wedged: no tick, so no beat reaches the child.
        thread::sleep(Duration::from_millis(80));
        let settled = beats.count();
        thread::sleep(Duration::from_millis(200));
        assert_eq!(
            beats.count(),
            settled,
            "beats sent while the runtime was stuck"
        );
        stop.store(true, Ordering::SeqCst);
        handle.join().unwrap();
    }

    #[test]
    fn heartbeat_without_a_runtime_beats_every_interval() {
        let beats = Beats::default();
        let stop = Arc::new(AtomicBool::new(false));
        let handle = spawn_heartbeat(
            beats.clone(),
            Duration::from_millis(20),
            Arc::clone(&stop),
            None,
        );
        thread::sleep(Duration::from_millis(120));
        stop.store(true, Ordering::SeqCst);
        handle.join().unwrap();
        assert!(beats.count() >= 3, "beats: {}", beats.count());
    }

    #[test]
    fn the_runtime_ticker_stops_when_the_runtime_is_blocked() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        let progress = Arc::new(AtomicU64::new(0));
        let task = runtime.spawn(tick_runtime(
            Arc::clone(&progress),
            Duration::from_millis(10),
        ));
        runtime.block_on(async { tokio::time::sleep(Duration::from_millis(60)).await });
        let alive = progress.load(Ordering::SeqCst);
        assert!(alive >= 2, "ticks while the runtime runs: {alive}");
        // Nothing drives the current-thread runtime now, like a wedged runtime.
        thread::sleep(Duration::from_millis(80));
        assert_eq!(progress.load(Ordering::SeqCst), alive);
        task.abort();
    }

    #[test]
    fn no_watchdog_env() {
        assert!(!is_env_truthy(None));
        assert!(is_env_truthy(Some("1".into())));
        assert!(is_env_truthy(Some("TRUE".into())));
        assert!(!is_env_truthy(Some("0".into())));
    }
}
