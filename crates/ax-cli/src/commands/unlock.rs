//! Clear a stale `.ax/ax.lock`, stopping only the ax process that holds this project's lock.

use std::path::Path;
use std::time::Duration;

use ax_context::directory::{get_ax_dir, is_initialized};

use crate::commands::ax_process::{is_ax_process, process_running, stop_pid};
use crate::commands::resolve_project_root;
use crate::ui::{info_line, ok_line};

/// How long the lock holder gets after SIGTERM before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(5);
/// How long to wait for the holder to disappear after SIGKILL.
const KILL_WAIT: Duration = Duration::from_secs(3);

pub async fn run(path: Option<String>) -> Result<(), String> {
    let root = resolve_project_root(path);
    if !is_initialized(&root) {
        return Err(format!(
            "project not initialized in {} — run ax init first",
            root.display()
        ));
    }
    let lock_path = get_ax_dir(&root).join("ax.lock");

    // The lock file is never deleted (that would let two writers lock two
    // different inodes); clearing it only works when nobody holds the lock.
    if !lock_path.exists() || ax_utils::clear_stale_lock(&lock_path) {
        println!(
            "{}",
            ok_line("Lock cleared. You can run ax init or ax index again.")
        );
        return Ok(());
    }

    let holder = read_lock_pid(&lock_path);
    if let Some(pid) = holder.filter(|&p| is_stoppable_holder(p)) {
        stop_pid(pid, STOP_GRACE, KILL_WAIT).await?;
        println!("{}", info_line(format!("Stopped ax process {pid}")));
    }

    if !ax_utils::clear_stale_lock(&lock_path) {
        let who = holder.map(|p| format!(" by PID {p}")).unwrap_or_default();
        return Err(format!(
            "{} is still held{who}, possibly by a process in another container or user; stop it first",
            lock_path.display()
        ));
    }
    println!(
        "{}",
        ok_line("Lock cleared. You can run ax init or ax index again.")
    );
    Ok(())
}

/// Only a running ax process other than this one is ever signalled.
fn is_stoppable_holder(pid: u32) -> bool {
    pid != std::process::id() && process_running(pid) && is_ax_process(pid)
}

fn read_lock_pid(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}
