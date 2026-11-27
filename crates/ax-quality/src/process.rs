//! Child processes with a deadline and drained output.
//!
//! Every external command (container CLI, sonar-scanner) runs through
//! [`run_with_timeout`]: stdin is closed, stdout and stderr are piped and read
//! on their own threads while the child runs (so a chatty child never blocks
//! on a full pipe and nothing reaches the parent's stdout), and the child is
//! killed when the deadline passes.

use std::io::Read;
use std::process::{Command, ExitStatus, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How much of each output stream is kept (the tail).
const KEEP_BYTES: usize = 64 * 1024;

/// Result of a command that ran to completion.
#[derive(Debug)]
pub(crate) struct CmdOutput {
    pub status: ExitStatus,
    /// The last [`KEEP_BYTES`] of stdout.
    pub stdout: String,
    /// The last [`KEEP_BYTES`] of stderr.
    pub stderr: String,
}

impl CmdOutput {
    pub fn success(&self) -> bool {
        self.status.success()
    }

    /// Last non-empty stderr line, for an error message.
    pub fn stderr_tail_line(&self) -> Option<&str> {
        self.stderr.lines().map(str::trim).rfind(|l| !l.is_empty())
    }
}

/// Why a command did not produce a [`CmdOutput`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RunError {
    /// The program could not be started.
    Spawn(String),
    /// The deadline passed; the child was killed.
    TimedOut(Duration),
    /// Waiting for the child failed.
    Wait(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "could not start: {e}"),
            Self::TimedOut(t) => write!(f, "timed out after {}s — killed", t.as_secs()),
            Self::Wait(e) => write!(f, "error waiting for process: {e}"),
        }
    }
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

/// Runs `cmd` to completion or until `timeout` passes, whichever is first.
pub(crate) fn run_with_timeout(
    cmd: &mut Command,
    timeout: Duration,
) -> Result<CmdOutput, RunError> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| RunError::Spawn(e.to_string()))?;
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
                    let _ = child.kill();
                    let _ = child.wait();
                    // A grandchild may still hold the pipes open; do not wait for the readers.
                    return Err(RunError::TimedOut(timeout));
                }
                std::thread::sleep(pause.min(timeout - elapsed));
                pause = (pause * 2).min(Duration::from_millis(250));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::Wait(e.to_string()));
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
    fn output_is_captured_not_inherited() {
        let out = run_with_timeout(&mut sh("echo hello; echo oops >&2"), Duration::from_secs(5))
            .expect("runs");
        assert!(out.success());
        assert_eq!(out.stdout, "hello\n");
        assert_eq!(out.stderr_tail_line(), Some("oops"));
    }

    #[test]
    fn a_hanging_command_is_killed_at_the_deadline() {
        let started = Instant::now();
        let err = run_with_timeout(&mut sh("exec sleep 30"), Duration::from_millis(300))
            .expect_err("times out");
        assert_eq!(err, RunError::TimedOut(Duration::from_millis(300)));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn stdin_is_closed() {
        // `cat` would wait forever on an inherited terminal or open pipe.
        let out = run_with_timeout(&mut sh("cat"), Duration::from_secs(5)).expect("runs");
        assert!(out.success());
    }

    #[test]
    fn only_the_tail_of_a_large_stream_is_kept() {
        let out = run_with_timeout(
            &mut sh("head -c 1048576 /dev/zero; echo end"),
            Duration::from_secs(10),
        )
        .expect("runs");
        assert_eq!(out.stdout.len(), KEEP_BYTES);
        assert!(out.stdout.ends_with("end\n"));
    }

    #[test]
    fn a_missing_program_is_a_spawn_error() {
        let err = run_with_timeout(
            &mut Command::new("/nonexistent/ax-no-such-program"),
            Duration::from_secs(1),
        )
        .expect_err("fails");
        assert!(matches!(err, RunError::Spawn(_)));
    }
}
