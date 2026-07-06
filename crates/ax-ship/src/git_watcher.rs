//! Git state watcher on .git/HEAD, refs, index.

use std::path::PathBuf;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tracing::info;

use crate::events::{ShipEvent, ShipEventBus};

pub async fn start_git_watcher(project_root: PathBuf, bus: ShipEventBus) -> Result<(), String> {
    let git_dir = project_root.join(".git");
    if !git_dir.exists() {
        return Err("no .git directory".into());
    }

    let (tx, mut rx) = mpsc::channel(64);
    let mut watcher = notify::recommended_watcher(move |res| {
        if let Ok(event) = res {
            let _ = tx.blocking_send(event);
        }
    })
    .map_err(|e| e.to_string())?;

    for sub in ["HEAD", "index", "refs"] {
        let path = git_dir.join(sub);
        if path.exists() {
            let mode = if sub == "refs" {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            watcher.watch(&path, mode).map_err(|e| e.to_string())?;
        }
    }

    std::mem::forget(watcher);

    tokio::spawn(async move {
        let mut debounce = tokio::time::Instant::now();
        while let Some(_event) = rx.recv().await {
            if debounce.elapsed() < Duration::from_millis(300) {
                continue;
            }
            debounce = tokio::time::Instant::now();
            let branch = ax_git::current_branch(&project_root).ok().flatten();
            info!(?branch, "git state changed");
            bus.publish(ShipEvent::GitChanged { branch });
            if let Ok(report) = crate::evaluate_project(project_root.clone()).await {
                bus.publish(ShipEvent::ReportUpdated(report));
            }
        }
    });

    Ok(())
}
