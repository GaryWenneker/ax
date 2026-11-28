//! Portable process RSS sampling for Neural benchmarks (macOS + Linux).

use serde_json::{json, Value};

#[cfg(test)]
thread_local! {
    /// Samples taken on this thread. Per thread so parallel tests that each
    /// run a harness only see their own samples.
    static PROCESS_RSS_SAMPLE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

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
    PROCESS_RSS_SAMPLE_CALLS.with(|calls| calls.set(calls.get() + 1));
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
        // proc_pidinfo answers in-process; no child process is spawned.
        let mut info = std::mem::MaybeUninit::<libc::proc_taskinfo>::zeroed();
        let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
        let pid = std::process::id() as libc::c_int;
        // SAFETY: `info` is a writable buffer of exactly `size` bytes for a
        // PROC_PIDTASKINFO reply, and it is read only when the call filled it.
        let written = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTASKINFO,
                0,
                info.as_mut_ptr().cast::<libc::c_void>(),
                size,
            )
        };
        if written != size {
            return None;
        }
        // SAFETY: the call wrote the whole struct (checked above).
        let info = unsafe { info.assume_init() };
        Some(info.pti_resident_size)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

/// Returns and clears the number of samples taken on the calling thread.
#[cfg(test)]
pub(crate) fn take_process_rss_sample_calls() -> usize {
    PROCESS_RSS_SAMPLE_CALLS.with(|calls| calls.replace(0))
}

/// Clears the sample count of the calling thread.
#[cfg(test)]
pub(crate) fn reset_process_rss_sample_calls() {
    PROCESS_RSS_SAMPLE_CALLS.with(|calls| calls.set(0));
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
    fn sample_counter_ignores_samples_taken_on_other_threads() {
        reset_process_rss_sample_calls();
        std::thread::spawn(|| {
            let _ = process_rss_bytes();
        })
        .join()
        .unwrap();
        assert_eq!(take_process_rss_sample_calls(), 0);
        let _ = process_rss_bytes();
        assert_eq!(take_process_rss_sample_calls(), 1);
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
