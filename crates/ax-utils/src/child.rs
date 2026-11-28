//! External commands with a deadline and drained output.
//!
//! [`run_with_timeout`] closes stdin, pipes stdout and stderr and reads them on
//! their own threads while the child runs (so a chatty child never blocks on a
//! full pipe and nothing reaches the parent's stdout), and kills the child when
//! the deadline passes. Once the child exits, the readers get a short grace
//! period to finish; a background grandchild that still holds the pipes does
//! not hold up the result, and the output read so far is returned.
//! [`run_group_with_timeout`] also starts the child in its own process group
//! and kills the whole group at the deadline.

use std::io::Read;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

/// How much of each output stream is kept (the tail).
pub const KEEP_BYTES: usize = 64 * 1024;

/// How long the output readers may run after the child exited.
const READ_GRACE: Duration = Duration::from_secs(2);

/// Result of a command that ran to completion.
#[derive(Debug)]
pub struct CmdOutput {
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
pub enum RunError {
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
            Self::TimedOut(t) => write!(f, "timed out after {}s and was stopped", t.as_secs()),
            Self::Wait(e) => write!(f, "error waiting for process: {e}"),
        }
    }
}

impl std::error::Error for RunError {}

/// One output stream being read on its own thread.
struct Drain {
    tail: Arc<Mutex<Vec<u8>>>,
    done: mpsc::Receiver<()>,
}

fn trim_tail(tail: &mut Vec<u8>, keep: usize) {
    if tail.len() > keep {
        tail.drain(..tail.len() - keep);
    }
}

fn drain(mut reader: impl Read + Send + 'static) -> Drain {
    let tail = Arc::new(Mutex::new(Vec::new()));
    let (tx, done) = mpsc::channel();
    let shared = Arc::clone(&tail);
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let mut tail = shared.lock().unwrap_or_else(|e| e.into_inner());
                    tail.extend_from_slice(&buf[..n]);
                    if tail.len() > 2 * KEEP_BYTES {
                        trim_tail(&mut tail, KEEP_BYTES);
                    }
                }
            }
        }
        let _ = tx.send(());
    });
    Drain { tail, done }
}

/// Waits for the reader until `until`, then returns what it has read.
fn collect(drain: Option<Drain>, until: Instant) -> String {
    let Some(drain) = drain else {
        return String::new();
    };
    let _ = drain
        .done
        .recv_timeout(until.saturating_duration_since(Instant::now()));
    let mut tail = drain.tail.lock().unwrap_or_else(|e| e.into_inner());
    trim_tail(&mut tail, KEEP_BYTES);
    String::from_utf8_lossy(&tail).into_owned()
}

/// Runs `cmd` to completion or until `timeout` passes, whichever is first.
pub fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<CmdOutput, RunError> {
    run(cmd, timeout, false)
}

/// [`run_with_timeout`], with the child leading its own process group so the
/// deadline kills helpers it started too (for example `git-remote-https`).
pub fn run_group_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<CmdOutput, RunError> {
    run(cmd, timeout, true)
}

#[cfg(unix)]
fn kill(child: &mut std::process::Child, group: bool) {
    if group {
        if let Ok(pid) = libc::pid_t::try_from(child.id()) {
            // SAFETY: kill(2) with a negative pid signals the process group the
            // child leads (it was spawned with process_group(0)).
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
    }
    let _ = child.kill();
}

#[cfg(not(unix))]
fn kill(child: &mut std::process::Child, _group: bool) {
    let _ = child.kill();
}

fn run(cmd: &mut Command, timeout: Duration, group: bool) -> Result<CmdOutput, RunError> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    if group {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().map_err(|e| RunError::Spawn(e.to_string()))?;
    let out = child.stdout.take().map(drain);
    let err = child.stderr.take().map(drain);
    let started = Instant::now();
    let deadline = started + timeout;
    let mut pause = Duration::from_millis(10);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let until = deadline.min(Instant::now() + READ_GRACE);
                return Ok(CmdOutput {
                    status,
                    stdout: collect(out, until),
                    stderr: collect(err, until),
                });
            }
            Ok(None) => {
                let elapsed = started.elapsed();
                if elapsed >= timeout {
                    kill(&mut child, group);
                    let _ = child.wait();
                    // A grandchild may still hold the pipes open; do not wait for the readers.
                    return Err(RunError::TimedOut(timeout));
                }
                std::thread::sleep(pause.min(timeout - elapsed));
                pause = (pause * 2).min(Duration::from_millis(250));
            }
            Err(e) => {
                kill(&mut child, group);
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
    fn a_hanging_group_and_its_children_are_killed_at_the_deadline() {
        let started = Instant::now();
        // The grandchild keeps the pipes open; the group kill must end it too.
        let err =
            run_group_with_timeout(&mut sh("sleep 30 & sleep 30"), Duration::from_millis(300))
                .expect_err("times out");
        assert!(err.to_string().contains("timed out"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_background_grandchild_holding_the_pipes_does_not_block_the_result() {
        for group in [false, true] {
            let started = Instant::now();
            let out = run(
                &mut sh("sleep 30 & echo done"),
                Duration::from_secs(20),
                group,
            )
            .expect("runs");
            assert!(out.success());
            assert_eq!(out.stdout, "done\n");
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "{:?}",
                started.elapsed()
            );
        }
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
