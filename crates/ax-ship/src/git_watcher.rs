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
    Fut: Future<Output = OwnGit> + Send + 'static,
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

/// What an evaluation did to git itself, so the watcher can tell its own
/// changes from changes the user made while it ran.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum OwnGit {
    /// The run made no git changes: any change during it came from elsewhere.
    #[default]
    Untouched,
    /// The run changed git (a checkpoint commit, a rollback) and this is the
    /// [`git_fingerprint`] right after its last own change.
    LeftAt(String),
    /// Git changed underneath the run between its own changes.
    Disturbed,
}

/// A summary of the git state the gate reads, per repository: the branch,
/// the HEAD commit and the staged content (`git ls-files -s`, which leaves the
/// index file untouched). Stat-only index rewrites do not change it.
pub(crate) fn git_fingerprint(roots: &[PathBuf]) -> String {
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
/// for `quiet`, then runs once. A run starts only when the git fingerprint
/// differs from the state the previous run evaluated, so the run's own git
/// activity (checkpoint commits, rollbacks, index refreshes) does not start
/// another run. Events that arrive while a run is in progress are dropped,
/// but the fingerprint after the run is compared with the state the run
/// expects to leave (see [`OwnGit`]): when the user committed, staged or
/// switched branch during the run, the loop evaluates again once git is quiet.
async fn watch_loop<F, FFut, E, EFut>(
    mut rx: mpsc::Receiver<()>,
    quiet: Duration,
    mut fingerprint: F,
    mut on_change: E,
) where
    F: FnMut() -> FFut,
    FFut: Future<Output = String>,
    E: FnMut() -> EFut,
    EFut: Future<Output = OwnGit>,
{
    let mut last_seen: Option<String> = None;
    let mut rerun = false;
    loop {
        if !rerun && rx.recv().await.is_none() {
            return;
        }
        rerun = false;
        loop {
            match tokio::time::timeout(quiet, rx.recv()).await {
                Ok(Some(())) => continue,
                Ok(None) => return,
                Err(_) => break,
            }
        }
        let before = fingerprint().await;
        if last_seen.as_deref() == Some(before.as_str()) {
            continue;
        }
        let own = on_change().await;
        // Drop what the run itself queued; the fingerprint decides whether
        // anything is left to evaluate.
        while rx.try_recv().is_ok() {}
        let after = fingerprint().await;
        let expected = match own {
            OwnGit::Untouched => Some(before),
            OwnGit::LeftAt(fp) => Some(fp),
            OwnGit::Disturbed => None,
        };
        if expected.as_deref() == Some(after.as_str()) {
            last_seen = Some(after);
        } else {
            info!("git changed during the evaluation; evaluating again");
            last_seen = expected;
            rerun = true;
        }
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
            std::future::ready(OwnGit::Untouched)
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
                let left = {
                    let mut s = state.lock().unwrap();
                    *s += 100;
                    s.to_string()
                };
                let _ = self_tx.send(()).await;
                tokio::time::sleep(Duration::from_secs(1)).await;
                let _ = self_tx.send(()).await;
                OwnGit::LeftAt(left)
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
            std::future::ready(OwnGit::Untouched)
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

    type RunFuture = std::pin::Pin<Box<dyn Future<Output = OwnGit> + Send>>;

    /// A fake evaluation lasting `secs` that then reports `own()`.
    fn long_run(
        runs: Arc<AtomicUsize>,
        secs: u64,
        own: impl Fn() -> OwnGit + Clone + Send + 'static,
    ) -> impl FnMut() -> RunFuture + Send + 'static {
        move || {
            runs.fetch_add(1, Ordering::SeqCst);
            let own = own.clone();
            Box::pin(async move {
                tokio::time::sleep(Duration::from_secs(secs)).await;
                own()
            }) as RunFuture
        }
    }

    /// The user commits while a long evaluation (that makes no git changes
    /// of its own) runs: the change is evaluated once the run ends.
    #[tokio::test(start_paused = true)]
    async fn a_change_during_a_run_is_evaluated_after_it() {
        let fake = Fake::default();
        let (tx, rx) = mpsc::channel(64);
        let task = tokio::spawn(watch_loop(
            rx,
            QUIET,
            fake.fingerprint(),
            long_run(fake.runs.clone(), 10, || OwnGit::Untouched),
        ));

        tokio::time::sleep(Duration::from_secs(1)).await;
        *fake.state.lock().unwrap() += 1;
        tx.send(()).await.unwrap();
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert_eq!(fake.runs.load(Ordering::SeqCst), 1);
        // Mid-run commit.
        *fake.state.lock().unwrap() += 1;
        tx.send(()).await.unwrap();
        tokio::time::sleep(Duration::from_secs(60)).await;
        assert_eq!(
            fake.runs.load(Ordering::SeqCst),
            2,
            "mid-run change was lost"
        );
        drop(tx);
        task.await.unwrap();
    }

    /// The run commits a checkpoint and the user changes git afterwards,
    /// still during the run: evaluated again; the checkpoint alone is not.
    #[tokio::test(start_paused = true)]
    async fn a_change_after_the_run_s_own_commit_is_evaluated() {
        let fake = Fake::default();
        let (tx, rx) = mpsc::channel(64);
        let state = fake.state.clone();
        let runs = fake.runs.clone();
        let user_tx = tx.clone();
        let user_state = fake.state.clone();
        let task = tokio::spawn(watch_loop(rx, QUIET, fake.fingerprint(), move || {
            let n = runs.fetch_add(1, Ordering::SeqCst);
            let state = state.clone();
            let user_state = user_state.clone();
            let user_tx = user_tx.clone();
            async move {
                // Own checkpoint commit.
                let left = {
                    let mut s = state.lock().unwrap();
                    *s += 100;
                    s.to_string()
                };
                tokio::time::sleep(Duration::from_secs(2)).await;
                if n == 0 {
                    // The user stages something while the gate still runs.
                    *user_state.lock().unwrap() += 1;
                    let _ = user_tx.send(()).await;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
                OwnGit::LeftAt(left)
            }
        }));

        tokio::time::sleep(Duration::from_secs(1)).await;
        *fake.state.lock().unwrap() += 1;
        tx.send(()).await.unwrap();
        tokio::time::sleep(Duration::from_secs(60)).await;
        // Run 1, then run 2 for the user's change. Run 2's own checkpoint
        // leaves git as it expects, so there is no run 3.
        assert_eq!(fake.runs.load(Ordering::SeqCst), 2);
        drop(tx);
        task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn a_disturbed_run_is_evaluated_again() {
        let fake = Fake::default();
        let (tx, rx) = mpsc::channel(64);
        let first = Arc::new(AtomicUsize::new(0));
        let task = tokio::spawn(watch_loop(
            rx,
            QUIET,
            fake.fingerprint(),
            long_run(fake.runs.clone(), 1, move || {
                if first.fetch_add(1, Ordering::SeqCst) == 0 {
                    OwnGit::Disturbed
                } else {
                    OwnGit::Untouched
                }
            }),
        ));

        tokio::time::sleep(Duration::from_secs(1)).await;
        *fake.state.lock().unwrap() += 1;
        tx.send(()).await.unwrap();
        tokio::time::sleep(Duration::from_secs(30)).await;
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
            std::future::ready(OwnGit::Untouched)
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
