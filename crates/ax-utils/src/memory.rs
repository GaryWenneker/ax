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

/// Current process RSS in bytes when the platform provides a reader; otherwise 0.
pub fn current_rss_bytes() -> u64 {
    #[cfg(target_os = "linux")]
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
    #[cfg(target_os = "macos")]
    {
        let pid = std::process::id().to_string();
        if let Ok(output) = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &pid])
            .output()
        {
            if let Ok(text) = String::from_utf8(output.stdout) {
                if let Ok(kb) = text.trim().parse::<u64>() {
                    return kb.saturating_mul(1024);
                }
            }
        }
        0
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        0
    }
}
