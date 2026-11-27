//! External commands (git) with a deadline and no terminal interaction.
//!
//! stdin is closed, stdout and stderr are piped and drained on their own
//! threads, and when the deadline passes the whole process group is killed, so
//! a helper such as `git-remote-https` does not outlive its parent.

use std::io::Read;
use std::process::{Command, ExitStatus, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How much of each output stream is kept (the tail).
const KEEP_BYTES: usize = 64 * 1024;

pub(crate) struct CmdOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

fn drain(mut reader: impl Read + Send + 'static) -> JoinHandle<String> {
    std::thread::spawn(move || {
        let mut tail: Vec<u8> = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    tail.extend_from_slice(&buf[..n]);
                    if tail.len() > 2 * KEEP_BYTES {
                        tail.drain(..tail.len() - KEEP_BYTES);
                    }
                }
            }
        }
        if tail.len() > KEEP_BYTES {
            tail.drain(..tail.len() - KEEP_BYTES);
        }
        String::from_utf8_lossy(&tail).into_owned()
    })
}

#[cfg(unix)]
fn kill_tree(child: &mut std::process::Child) {
    if let Ok(pid) = libc::pid_t::try_from(child.id()) {
        // SAFETY: kill(2) with a negative pid signals the process group the
        // child leads (it was spawned with process_group(0)).
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    let _ = child.kill();
}

#[cfg(not(unix))]
fn kill_tree(child: &mut std::process::Child) {
    let _ = child.kill();
}

/// Runs `cmd` to completion or until `timeout` passes, whichever is first.
pub(crate) fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<CmdOutput, String> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let program = cmd.get_program().to_string_lossy().into_owned();
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not start {program}: {e}"))?;
    let out = child.stdout.take().map(drain);
    let err = child.stderr.take().map(drain);
    let collect = |h: Option<JoinHandle<String>>| h.and_then(|h| h.join().ok()).unwrap_or_default();
    let started = Instant::now();
    let mut pause = Duration::from_millis(10);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Ok(CmdOutput {
                    status,
                    stdout: collect(out),
                    stderr: collect(err),
                });
            }
            Ok(None) => {
                let elapsed = started.elapsed();
                if elapsed >= timeout {
                    kill_tree(&mut child);
                    let _ = child.wait();
                    return Err(format!(
                        "{program} timed out after {}s and was stopped",
                        timeout.as_secs()
                    ));
                }
                std::thread::sleep(pause.min(timeout - elapsed));
                pause = (pause * 2).min(Duration::from_millis(250));
            }
            Err(e) => {
                kill_tree(&mut child);
                let _ = child.wait();
                return Err(format!("error waiting for {program}: {e}"));
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sh(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.args(["-c", script]);
        c
    }

    #[test]
    fn stdin_is_closed() {
        let out = run_with_timeout(&mut sh("cat"), Duration::from_secs(5)).expect("runs");
        assert!(out.status.success());
    }

    #[test]
    fn a_hanging_command_and_its_children_are_killed_at_the_deadline() {
        let started = Instant::now();
        // The grandchild keeps the pipes open; the group kill must end it too.
        let err = run_with_timeout(&mut sh("sleep 30 & sleep 30"), Duration::from_millis(300))
            .err()
            .expect("times out");
        assert!(err.contains("timed out"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_large_stream_does_not_block_the_child() {
        let out = run_with_timeout(
            &mut sh("head -c 1048576 /dev/zero; echo end"),
            Duration::from_secs(10),
        )
        .expect("runs");
        assert_eq!(out.stdout.len(), KEEP_BYTES);
        assert!(out.stdout.ends_with("end\n"));
    }
}
