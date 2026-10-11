use std::io::{self, Write};
use std::process::{Command, Stdio};

use crate::Step;

pub fn run_steps(steps: &[Step]) -> io::Result<i32> {
    for step in steps {
        println!();
        println!("==> {}", step.label);
        let _interrupt = IgnoreSigint::install();
        let mut child = Command::new(&step.program)
            .args(&step.args)
            .current_dir(&step.cwd)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!("could not start {} ({}): {error}", step.label, step.program),
                )
            })?;
        let status = child.wait()?;
        let code = status.code().unwrap_or(1);
        if code != 0 {
            let _ = writeln!(io::stderr(), "{} failed with exit {code}", step.label);
            return Ok(code);
        }
    }
    Ok(0)
}

struct IgnoreSigint;

impl IgnoreSigint {
    fn install() -> Self {
        // SIGINT reaches the whole foreground group. Ignore it in this process so
        // Ctrl+C stops the child and the menu can come back.
        #[cfg(unix)]
        unsafe {
            libc::signal(libc::SIGINT, libc::SIG_IGN);
        }
        Self
    }
}

impl Drop for IgnoreSigint {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::signal(libc::SIGINT, libc::SIG_DFL);
        }
    }
}
