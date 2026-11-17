//! Portable process RSS sampling for Neural benchmarks (macOS + Linux).

use serde_json::{json, Value};

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(test)]
use std::sync::Mutex;

#[cfg(test)]
static PROCESS_RSS_SAMPLE_CALLS: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
static PROCESS_RSS_SAMPLE_LOCK: Mutex<()> = Mutex::new(());

/// Builds the harness `memory` JSON from an already-sampled RSS value (no extra I/O).
pub fn memory_json(bytes: Option<u64>) -> Value {
    match bytes.filter(|b| *b > 0) {
        Some(bytes) => json!({
            "kind": "measured",
            "rssBytes": bytes,
            "rssMiB": (bytes as f64) / (1024.0 * 1024.0)
        }),
        None => json!({
            "kind": "not_measured",
            "reason": if cfg!(windows) {
                "Windows RSS sampling not implemented (no portable reader without extra deps)"
            } else {
                "RSS reader returned 0 or unsupported platform"
            }
        }),
    }
}

/// Sample process RSS once and format the harness `memory` block.
pub fn memory_report() -> serde_json::Value {
    memory_json(process_rss_bytes())
}

/// Returns current RSS in bytes when readable; `None` when the platform reader fails.
pub fn process_rss_bytes() -> Option<u64> {
    #[cfg(test)]
    {
        let _g = PROCESS_RSS_SAMPLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        PROCESS_RSS_SAMPLE_CALLS.fetch_add(1, Ordering::Relaxed);
    }
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                let kb = rest.split_whitespace().next()?.parse::<u64>().ok()?;
                return Some(kb.saturating_mul(1024));
            }
        }
        None
    }
    #[cfg(target_os = "macos")]
    {
        let pid = std::process::id().to_string();
        let output = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &pid])
            .output()
            .ok()?;
        let text = String::from_utf8(output.stdout).ok()?;
        let kb = text.trim().parse::<u64>().ok()?;
        Some(kb.saturating_mul(1024))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

#[cfg(test)]
pub(crate) fn take_process_rss_sample_calls() -> usize {
    let _g = PROCESS_RSS_SAMPLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    PROCESS_RSS_SAMPLE_CALLS.swap(0, Ordering::Relaxed)
}

#[cfg(test)]
pub(crate) fn reset_process_rss_sample_calls() {
    let _g = PROCESS_RSS_SAMPLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    PROCESS_RSS_SAMPLE_CALLS.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_json_kind_follows_sample() {
        let measured = memory_json(Some(1024));
        assert_eq!(measured["kind"], "measured");
        assert_eq!(measured["rssBytes"], 1024);

        let zero = memory_json(Some(0));
        assert_eq!(zero["kind"], "not_measured");

        let missing = memory_json(None);
        assert_eq!(missing["kind"], "not_measured");
    }

    #[test]
    fn rss_reader_returns_positive_on_this_host() {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            let bytes = process_rss_bytes().expect("rss");
            assert!(bytes > 0, "expected positive RSS, got {bytes}");
        }
    }
}
