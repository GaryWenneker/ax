//! Git state watcher on .git/HEAD, refs, index.

use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tracing::info;

use crate::events::{ShipEvent, ShipEventBus};

/// A running git watcher. Dropping it stops the file watcher and its task.
pub(crate) struct GitWatch {
    _watcher: notify::RecommendedWatcher,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for GitWatch {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub(crate) fn start_git_watcher<E, Fut>(
    git_roots: Vec<PathBuf>,
    bus: ShipEventBus,
    evaluate: E,
) -> Result<GitWatch, String>
where
    E: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    if git_roots.is_empty() {
        return Err("no git repositories to watch".into());
    }

    let (tx, rx) = mpsc::channel(64);
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            // A full channel already holds a pending trigger; dropping is fine.
            let _ = tx.try_send(());
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

    let roots = git_roots.clone();
    let fingerprint = move || {
        let roots = roots.clone();
        async move {
            tokio::task::spawn_blocking(move || git_fingerprint(&roots))
                .await
                .unwrap_or_default()
        }
    };
    let mut evaluate = evaluate;
    let on_change = move || {
        let branch = git_roots
            .first()
            .and_then(|root| ax_git::current_branch(root).ok().flatten());
        info!(?branch, repos = git_roots.len(), "git state changed");
        bus.publish(ShipEvent::GitChanged { branch });
        evaluate()
    };
    let task = tokio::spawn(watch_loop(rx, QUIET_PERIOD, fingerprint, on_change));

    Ok(GitWatch {
        _watcher: watcher,
        task,
    })
}

/// How long git must stay quiet before an evaluation starts.
const QUIET_PERIOD: Duration = Duration::from_millis(300);

/// A summary of the git state the gate reads, per repository: the branch,
/// the HEAD commit and the staged content (`git ls-files -s`, which leaves the
/// index file untouched). Stat-only index rewrites do not change it.
fn git_fingerprint(roots: &[PathBuf]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for root in roots {
        root.hash(&mut hasher);
        for args in [
            &["symbolic-ref", "-q", "HEAD"][..],
            &["rev-parse", "-q", "--verify", "HEAD"][..],
            &["ls-files", "-s"][..],
        ] {
            let out = std::process::Command::new("git")
                .current_dir(root)
                .args(args)
                .output();
            match out {
                Ok(o) => o.stdout.hash(&mut hasher),
                Err(e) => e.to_string().hash(&mut hasher),
            }
        }
    }
    format!("{:016x}", hasher.finish())
}

/// Runs `on_change` for bursts of git events.
///
/// Trailing-edge debounce: after an event, waits until no event has arrived
/// for `quiet`, then runs once. Events that arrive while a run is in progress
/// are coalesced into the next check. A run starts only when the git
/// fingerprint differs from the one the previous run left behind, so the
/// run's own git activity (checkpoint commits, rollbacks, index refreshes)
/// does not start another run.
async fn watch_loop<F, FFut, E, EFut>(
    mut rx: mpsc::Receiver<()>,
    quiet: Duration,
    mut fingerprint: F,
    mut on_change: E,
) where
    F: FnMut() -> FFut,
    FFut: Future<Output = String>,
    E: FnMut() -> EFut,
    EFut: Future<Output = ()>,
{
    let mut last_seen: Option<String> = None;
    while rx.recv().await.is_some() {
        loop {
            match tokio::time::timeout(quiet, rx.recv()).await {
                Ok(Some(())) => continue,
                Ok(None) => return,
                Err(_) => break,
            }
        }
        let now = fingerprint().await;
        if last_seen.as_deref() == Some(now.as_str()) {
            continue;
        }
        on_change().await;
        // Drop what the run itself queued; the fingerprint decides whether
        // anything is left to evaluate.
        while rx.try_recv().is_ok() {}
        last_seen = Some(fingerprint().await);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    /// A fake repository: `state` is what the fingerprint reads, `runs`
    /// counts evaluations.
    #[derive(Clone, Default)]
    struct Fake {
        state: Arc<Mutex<u32>>,
        runs: Arc<AtomicUsize>,
    }

    impl Fake {
        fn fingerprint(&self) -> impl FnMut() -> std::future::Ready<String> {
            let state = self.state.clone();
            move || std::future::ready(state.lock().unwrap().to_string())
        }
    }

    const QUIET: Duration = Duration::from_millis(50);

    #[tokio::test(start_paused = true)]
    async fn a_burst_of_events_runs_one_evaluation_after_it_settles() {
        let fake = Fake::default();
        let (tx, rx) = mpsc::channel(64);
        let runs = fake.runs.clone();
        let task = tokio::spawn(watch_loop(rx, QUIET, fake.fingerprint(), move || {
            runs.fetch_add(1, Ordering::SeqCst);
            std::future::ready(())
        }));

        tokio::time::sleep(Duration::from_secs(1)).await;
        for _ in 0..5 {
            *fake.state.lock().unwrap() += 1;
            tx.send(()).await.unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(fake.runs.load(Ordering::SeqCst), 0, "must wait for quiet");
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(fake.runs.load(Ordering::SeqCst), 1);
        drop(tx);
        task.await.unwrap();
    }

    /// An evaluation that touches git itself (auto-commit, an index refresh)
    /// must not start another evaluation.
    #[tokio::test(start_paused = true)]
    async fn events_caused_by_the_evaluation_do_not_retrigger_it() {
        let fake = Fake::default();
        let (tx, rx) = mpsc::channel(64);
        let runs = fake.runs.clone();
        let state = fake.state.clone();
        let self_tx = tx.clone();
        let task = tokio::spawn(watch_loop(rx, QUIET, fake.fingerprint(), move || {
            runs.fetch_add(1, Ordering::SeqCst);
            let state = state.clone();
            let self_tx = self_tx.clone();
            async move {
                // A long run that commits a checkpoint and refreshes the index.
                tokio::time::sleep(Duration::from_secs(2)).await;
                *state.lock().unwrap() += 100;
                let _ = self_tx.send(()).await;
                tokio::time::sleep(Duration::from_secs(1)).await;
                let _ = self_tx.send(()).await;
            }
        }));

        tokio::time::sleep(Duration::from_secs(1)).await;
        *fake.state.lock().unwrap() += 1;
        tx.send(()).await.unwrap();
        tokio::time::sleep(Duration::from_secs(30)).await;
        assert_eq!(fake.runs.load(Ordering::SeqCst), 1);
        drop(tx);
        task.abort();
    }

    /// A change that leaves the git state as the last run left it (an index
    /// stat refresh) is ignored; a real change afterwards runs again.
    #[tokio::test(start_paused = true)]
    async fn only_a_changed_git_state_runs_again() {
        let fake = Fake::default();
        let (tx, rx) = mpsc::channel(64);
        let runs = fake.runs.clone();
        let task = tokio::spawn(watch_loop(rx, QUIET, fake.fingerprint(), move || {
            runs.fetch_add(1, Ordering::SeqCst);
            std::future::ready(())
        }));

        tokio::time::sleep(Duration::from_secs(1)).await;
        *fake.state.lock().unwrap() += 1;
        tx.send(()).await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(fake.runs.load(Ordering::SeqCst), 1);

        tx.send(()).await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(fake.runs.load(Ordering::SeqCst), 1, "same state: no run");

        *fake.state.lock().unwrap() += 1;
        tx.send(()).await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(fake.runs.load(Ordering::SeqCst), 2);
        drop(tx);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn dropping_the_watch_stops_watching() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".git/refs")).unwrap();
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();

        let runs = Arc::new(AtomicUsize::new(0));
        let counter = runs.clone();
        let watch = start_git_watcher(vec![repo], ShipEventBus::new(4), move || {
            counter.fetch_add(1, Ordering::SeqCst);
            std::future::ready(())
        })
        .unwrap();
        let task_handle = watch.task.abort_handle();
        drop(watch);
        tokio::task::yield_now().await;
        assert!(task_handle.is_finished(), "the watch task must stop");
    }
}

#[cfg(test)]
mod send_sync {
    /// The daemon (which holds the watch) is shared across tasks.
    #[allow(dead_code)]
    fn daemon_is_send_and_sync() {
        fn check<T: Send + Sync>() {}
        check::<crate::ShipDaemon>();
    }
}
