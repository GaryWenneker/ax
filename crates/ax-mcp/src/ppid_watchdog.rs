//! PPID watchdog — CG: mcp/ppid-watchdog.ts (#277, #692).

use std::time::Duration;

use sysinfo::{Pid, ProcessesToUpdate, System};

pub const DEFAULT_PPID_POLL_MS: u64 = 5000;
pub const HOST_PPID_ENV: &str = "AX_HOST_PPID";
pub const PPID_POLL_ENV: &str = "AX_PPID_POLL_MS";

pub struct SupervisionState {
    pub original_ppid: u32,
    pub current_ppid: u32,
    pub host_ppid: Option<u32>,
    pub is_windows: bool,
}

/// Returns a shutdown reason when supervision is lost, or `None` while supervised.
pub fn supervision_lost_reason(state: &SupervisionState) -> Option<String> {
    if state.current_ppid != state.original_ppid {
        return Some(format!("ppid {} -> {}", state.original_ppid, state.current_ppid));
    }
    if state.is_windows && state.original_ppid > 1 && !is_process_alive(state.original_ppid) {
        return Some(format!("parent pid {} exited", state.original_ppid));
    }
    if let Some(host) = state.host_ppid {
        if !is_process_alive(host) {
            return Some(format!("host pid {} exited", host));
        }
    }
    None
}

pub fn parse_ppid_poll_ms(raw: Option<String>) -> u64 {
    match raw {
        None => DEFAULT_PPID_POLL_MS,
        Some(s) if s.is_empty() => DEFAULT_PPID_POLL_MS,
        Some(s) => {
            let parsed = s.parse::<f64>().unwrap_or(-1.0);
            if !parsed.is_finite() || parsed < 0.0 {
                DEFAULT_PPID_POLL_MS
            } else {
                parsed.floor() as u64
            }
        }
    }
}

pub fn parse_host_ppid(raw: Option<String>) -> Option<u32> {
    match raw {
        None => None,
        Some(s) if s.is_empty() => None,
        Some(s) => {
            let parsed = s.parse::<i64>().unwrap_or(0);
            if parsed <= 1 { None } else { Some(parsed as u32) }
        }
    }
}

pub fn parent_pid() -> u32 {
    let pid = Pid::from_u32(std::process::id());
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    sys.process(pid)
        .and_then(|p| p.parent())
        .map(|p| p.as_u32())
        .unwrap_or(0)
}

pub fn is_process_alive(pid: u32) -> bool {
    if pid <= 1 {
        return false;
    }
    let pid = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    sys.process(pid).is_some()
}

/// Spawn async poll loop; calls `on_death` when parent/host supervision is lost.
pub fn spawn_ppid_watchdog<F: Fn() + Send + Sync + 'static>(on_death: F) {
    let poll_ms = parse_ppid_poll_ms(std::env::var(PPID_POLL_ENV).ok());
    if poll_ms == 0 {
        return;
    }
    let original_ppid = parent_pid();
    let host_ppid = parse_host_ppid(std::env::var(HOST_PPID_ENV).ok());
    let is_windows = std::env::consts::OS == "windows";

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(poll_ms));
        loop {
            interval.tick().await;
            let state = SupervisionState {
                original_ppid,
                current_ppid: parent_pid(),
                host_ppid,
                is_windows,
            };
            if let Some(reason) = supervision_lost_reason(&state) {
                tracing::info!("Parent process exited ({}); shutting down", reason);
                on_death();
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_ppid_change_signals_loss() {
        let reason = supervision_lost_reason(&SupervisionState {
            original_ppid: 100,
            current_ppid: 1,
            host_ppid: None,
            is_windows: false,
        });
        assert!(reason.is_some());
    }

    #[test]
    fn windows_parent_death_signals_loss() {
        let reason = supervision_lost_reason(&SupervisionState {
            original_ppid: 4242,
            current_ppid: 4242,
            host_ppid: None,
            is_windows: true,
        });
        // 4242 unlikely alive in CI — if alive test is flaky; use host path instead
        if !is_process_alive(4242) {
            assert!(reason.is_some());
        }
    }

    #[test]
    fn host_ppid_death_signals_loss() {
        let reason = supervision_lost_reason(&SupervisionState {
            original_ppid: 100,
            current_ppid: 100,
            host_ppid: Some(4242),
            is_windows: false,
        });
        if !is_process_alive(4242) {
            assert_eq!(reason, Some("host pid 4242 exited".to_string()));
        }
    }

    #[test]
    fn parse_poll_ms_zero_disables() {
        assert_eq!(parse_ppid_poll_ms(Some("0".to_string())), 0);
    }
}
