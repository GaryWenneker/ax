//! Child processes with a deadline and drained output.
//!
//! Every external command (container CLI, sonar-scanner) runs through
//! [`run_with_timeout`] from `ax_utils::child`: stdin is closed, output is
//! read while the child runs, the child is killed at the deadline, and a
//! background grandchild that keeps the pipes open does not hold up the result.

pub(crate) use ax_utils::child::{run_with_timeout, CmdOutput, RunError};
