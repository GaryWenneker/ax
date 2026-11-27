//! External commands (git) with a deadline and no terminal interaction.
//!
//! Commands run through `ax_utils::child::run_group_with_timeout`: stdin is
//! closed, output is read while the child runs, and when the deadline passes
//! the whole process group is killed, so a helper such as `git-remote-https`
//! does not outlive its parent. A background helper that keeps the pipes open
//! after git exits does not hold up the result.

use std::process::Command;
use std::time::Duration;

pub(crate) use ax_utils::child::CmdOutput;
use ax_utils::child::RunError;

/// Runs `cmd` to completion or until `timeout` passes, whichever is first.
/// Errors name the program.
pub(crate) fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<CmdOutput, String> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    ax_utils::child::run_group_with_timeout(cmd, timeout).map_err(|e| match e {
        RunError::Spawn(e) => format!("could not start {program}: {e}"),
        RunError::TimedOut(t) => {
            format!("{program} timed out after {}s and was stopped", t.as_secs())
        }
        RunError::Wait(e) => format!("error waiting for {program}: {e}"),
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn errors_name_the_program() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "exec sleep 30"]);
        let err = run_with_timeout(&mut cmd, Duration::from_millis(200)).unwrap_err();
        assert!(err.starts_with("sh timed out"), "{err}");
        let err = run_with_timeout(
            &mut Command::new("/nonexistent/ax-no-such-program"),
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(err.contains("could not start"), "{err}");
    }
}
