//! Memory monitoring.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;

/// Monitors process RSS and fires a callback when threshold is exceeded.
pub struct MemoryMonitor {
    threshold_bytes: u64,
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl MemoryMonitor {
    pub fn new(threshold_bytes: u64) -> Self {
        Self {
            threshold_bytes,
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    pub fn start<F>(&mut self, callback: F)
    where
        F: Fn(u64) + Send + Sync + 'static,
    {
        if self.running.load(Ordering::SeqCst) {
            return;
        }
        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let threshold = self.threshold_bytes;
        self.handle = Some(tokio::spawn(async move {
            while running.load(Ordering::SeqCst) {
                let rss = current_rss_bytes();
                if rss >= threshold {
                    callback(rss);
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }));
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

fn current_rss_bytes() -> u64 {
    #[cfg(target_os = "windows")]
    {
        // Approximate via Windows API not available without extra deps; return 0
        0
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<u64>() {
                            return kb * 1024;
                        }
                    }
                }
            }
        }
        0
    }
}
