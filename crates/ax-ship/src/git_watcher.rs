//! Git state watcher on .git/HEAD, refs, index.

use std::path::PathBuf;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tracing::info;

use crate::events::{ShipEvent, ShipEventBus};

pub async fn start_git_watcher(
    workspace_root: PathBuf,
    git_roots: Vec<PathBuf>,
    bus: ShipEventBus,
) -> Result<(), String> {
    if git_roots.is_empty() {
        return Err("no git repositories to watch".into());
    }

    let (tx, mut rx) = mpsc::channel(64);
    let mut watcher = notify::recommended_watcher(move |res| {
        if let Ok(event) = res {
            let _ = tx.blocking_send(event);
        }
    })
    .map_err(|e| e.to_string())?;

    for git_root in &git_roots {
        let git_dir = git_root.join(".git");
        if !git_dir.exists() {
            return Err(format!("no .git directory at {}", git_root.display()));
        }
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
    }

    std::mem::forget(watcher);

    tokio::spawn(async move {
        let mut debounce = tokio::time::Instant::now();
        while let Some(_event) = rx.recv().await {
            if debounce.elapsed() < Duration::from_millis(300) {
                continue;
            }
            debounce = tokio::time::Instant::now();
            let branch = git_roots
                .first()
                .and_then(|root| ax_git::current_branch(root).ok().flatten());
            info!(?branch, repos = git_roots.len(), "git state changed");
            bus.publish(ShipEvent::GitChanged { branch });
            if let Ok(report) = crate::evaluate_project(workspace_root.clone()).await {
                bus.publish(ShipEvent::ReportUpdated { report });
            }
        }
    });

    Ok(())
}
