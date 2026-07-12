//! Process liveness checks and port cleanup (no heavy deps).

/// PIDs with a TCP listener on `port`.
pub fn pids_listening_on_port(port: u16) -> Result<Vec<u32>, String> {
    #[cfg(windows)]
    {
        return pids_listening_on_port_windows(port);
    }
    #[cfg(unix)]
    {
        return pids_listening_on_port_unix(port);
    }
}

/// Stop processes listening on `port`, excluding `self_pid`. Returns how many were stopped.
pub fn kill_listening_on_port(port: u16, self_pid: u32) -> Result<usize, String> {
    let mut killed = 0usize;
    for pid in pids_listening_on_port(port)? {
        if pid == self_pid {
            continue;
        }
        if kill_pid_force(pid)? {
            killed += 1;
        }
    }
    Ok(killed)
}

fn local_endpoint_has_port(endpoint: &str, port: u16) -> bool {
    endpoint
        .rsplit_once(':')
        .and_then(|(_, p)| p.parse::<u16>().ok())
        == Some(port)
}

#[cfg(windows)]
fn pids_listening_on_port_windows(port: u16) -> Result<Vec<u32>, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("netstat")
        .args(["-ano"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("netstat failed: {e}"))?;
    parse_netstat_pids(&String::from_utf8_lossy(&out.stdout), port)
}

#[cfg(unix)]
fn pids_listening_on_port_unix(port: u16) -> Result<Vec<u32>, String> {
    if let Ok(out) = std::process::Command::new("lsof")
        .args(["-ti", &format!("tcp:{port}"), "-sTCP:LISTEN"])
        .output()
    {
        if out.status.success() {
            let pids = parse_pid_lines(&String::from_utf8_lossy(&out.stdout));
            if !pids.is_empty() {
                return Ok(pids);
            }
        }
    }

    if let Ok(out) = std::process::Command::new("ss")
        .args(["-ltnp", &format!("sport = :{port}")])
        .output()
    {
        if out.status.success() {
            let pids = parse_ss_pids(&String::from_utf8_lossy(&out.stdout));
            if !pids.is_empty() {
                return Ok(pids);
            }
        }
    }

    Ok(Vec::new())
}

fn parse_netstat_pids(text: &str, port: u16) -> Result<Vec<u32>, String> {
    let mut pids = Vec::new();
    for line in text.lines() {
        let upper = line.to_ascii_uppercase();
        if !upper.contains("LISTEN") {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(proto) = parts.next() else {
            continue;
        };
        if proto != "TCP" && proto != "TCPv6" {
            continue;
        }
        let Some(local) = parts.next() else {
            continue;
        };
        if !local_endpoint_has_port(local, port) {
            continue;
        }
        let Some(pid_str) = line.split_whitespace().last() else {
            continue;
        };
        if let Ok(pid) = pid_str.parse::<u32>() {
            if pid > 0 {
                pids.push(pid);
            }
        }
    }
    pids.sort_unstable();
    pids.dedup();
    Ok(pids)
}

#[cfg(unix)]
fn parse_pid_lines(text: &str) -> Vec<u32> {
    let mut pids = Vec::new();
    for line in text.lines() {
        if let Ok(pid) = line.trim().parse::<u32>() {
            if pid > 0 {
                pids.push(pid);
            }
        }
    }
    pids.sort_unstable();
    pids.dedup();
    pids
}

#[cfg(unix)]
fn parse_ss_pids(text: &str) -> Vec<u32> {
    let mut pids = Vec::new();
    for line in text.lines() {
        if let Some(start) = line.find("pid=") {
            let rest = &line[start + 4..];
            let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            if let Ok(pid) = rest[..end].parse::<u32>() {
                if pid > 0 {
                    pids.push(pid);
                }
            }
        }
    }
    pids.sort_unstable();
    pids.dedup();
    pids
}

fn kill_pid_force(pid: u32) -> Result<bool, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        return Ok(std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map(|s| s.success())
            .unwrap_or(false));
    }
    #[cfg(unix)]
    {
        return Ok(std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false));
    }
}

/// Whether `pid` is a running process.
pub fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        match out {
            Ok(o) => {
                let s = String::from_utf8_lossy(&o.stdout);
                s.contains(&pid.to_string()) && !s.to_ascii_lowercase().contains("no tasks")
            }
            Err(_) => false,
        }
    }

    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_process_is_alive() {
        assert!(is_pid_alive(std::process::id()));
    }

    #[test]
    fn dead_pid_is_not_alive() {
        assert!(!is_pid_alive(999_999_999));
    }

    #[test]
    fn local_endpoint_has_port_matches_exact_port() {
        assert!(super::local_endpoint_has_port("127.0.0.1:7070", 7070));
        assert!(super::local_endpoint_has_port("[::1]:7070", 7070));
        assert!(!super::local_endpoint_has_port("127.0.0.1:7071", 7070));
        assert!(!super::local_endpoint_has_port("127.0.0.1:17070", 7070));
    }

    #[test]
    fn parse_netstat_pids_finds_listener() {
        let sample = "\
  TCP    127.0.0.1:7070         0.0.0.0:0              LISTENING       4242
  TCP    127.0.0.1:7680         0.0.0.0:0              LISTENING       9999
";
        let pids = super::parse_netstat_pids(sample, 7070).unwrap();
        assert_eq!(pids, vec![4242]);
    }

    #[cfg(unix)]
    #[test]
    fn parse_ss_pids_extracts_pid() {
        let sample = "LISTEN 0 128 127.0.0.1:7070 0.0.0.0:* users:((\"ax\",pid=1234,fd=3))";
        assert_eq!(super::parse_ss_pids(sample), vec![1234]);
    }
}
