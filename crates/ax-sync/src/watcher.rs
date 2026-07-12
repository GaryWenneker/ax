//! File watcher with debounce and pending file tracking.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ax_types::PendingFile;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{mpsc, Mutex};

pub struct WatcherOptions {
    pub debounce_ms: u64,
}

impl Default for WatcherOptions {
    fn default() -> Self {
        Self { debounce_ms: 500 }
    }
}

pub struct FileWatcher {
    project_root: PathBuf,
    pending: Arc<Mutex<HashMap<String, PendingFile>>>,
    active: Arc<Mutex<bool>>,
    degraded: Arc<Mutex<bool>>,
    degraded_reason: Arc<Mutex<Option<String>>>,
    watcher: Option<RecommendedWatcher>,
}

impl FileWatcher {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            pending: Arc::new(Mutex::new(HashMap::new())),
            active: Arc::new(Mutex::new(false)),
            degraded: Arc::new(Mutex::new(false)),
            degraded_reason: Arc::new(Mutex::new(None)),
            watcher: None,
        }
    }

    pub async fn start(&mut self, _opts: WatcherOptions) -> Result<(), ax_utils::errors::AxError> {
        let (tx, mut rx) = mpsc::channel(256);
        let pending = self.pending.clone();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.blocking_send(event);
            }
        })
        .map_err(|e| ax_utils::errors::AxError::Other(e.to_string()))?;

        watcher
            .watch(&self.project_root, RecursiveMode::Recursive)
            .map_err(|e| ax_utils::errors::AxError::Other(e.to_string()))?;

        self.watcher = Some(watcher);
        *self.active.lock().await = true;

        let pending_clone = pending.clone();
        let root = self.project_root.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                let now = now_ms();
                let mut map = pending_clone.lock().await;
                for path in event.paths {
                    let rel = path
                        .strip_prefix(&root)
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_else(|_| path.to_string_lossy().replace('\\', "/"));
                    if rel.contains("/.ax/") || rel.starts_with(".ax/") {
                        continue;
                    }
                    map.entry(rel.clone()).or_insert_with(|| PendingFile {
                        path: rel.clone(),
                        first_seen_ms: now,
                        last_seen_ms: now,
                        indexing: false,
                    });
                    if let Some(p) = map.get_mut(&rel) {
                        p.last_seen_ms = now;
                        p.indexing = false;
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn stop(&mut self) {
        self.watcher = None;
        *self.active.lock().await = false;
    }

    pub async fn is_active(&self) -> bool {
        *self.active.lock().await
    }

    pub async fn is_degraded(&self) -> bool {
        *self.degraded.lock().await
    }

    pub async fn get_degraded_reason(&self) -> Option<String> {
        self.degraded_reason.lock().await.clone()
    }

    pub async fn get_pending_files(&self) -> Vec<PendingFile> {
        self.pending.lock().await.values().cloned().collect()
    }

    pub async fn clear_pending(&self, paths: &[String]) {
        let mut map = self.pending.lock().await;
        for p in paths {
            map.remove(p);
        }
    }

    /// Paths quiet for at least `debounce_ms` and not currently indexing.
    pub async fn get_ready_files(&self, debounce_ms: u64) -> Vec<String> {
        let now = now_ms();
        let debounce = debounce_ms as i64;
        self.pending
            .lock()
            .await
            .values()
            .filter(|p| !p.indexing && now - p.last_seen_ms >= debounce)
            .map(|p| p.path.clone())
            .collect()
    }

    pub async fn mark_indexing(&self, paths: &[String]) {
        let mut map = self.pending.lock().await;
        for p in paths {
            if let Some(entry) = map.get_mut(p) {
                entry.indexing = true;
            }
        }
    }

    pub async fn wait_until_ready(&self, timeout_ms: u64) -> bool {
        tokio::time::sleep(Duration::from_millis(timeout_ms.min(100))).await;
        true
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn insert_pending(watcher: &FileWatcher, path: &str, last_seen_ms: i64) {
        watcher.pending.lock().await.insert(
            path.to_string(),
            PendingFile {
                path: path.to_string(),
                first_seen_ms: last_seen_ms,
                last_seen_ms,
                indexing: false,
            },
        );
    }

    #[tokio::test]
    async fn ready_files_respect_debounce() {
        let w = FileWatcher::new(PathBuf::from("."));
        insert_pending(&w, "old.ts", now_ms() - 10_000).await;
        insert_pending(&w, "fresh.ts", now_ms()).await;

        let ready = w.get_ready_files(500).await;
        assert_eq!(ready, vec!["old.ts".to_string()]);
    }

    #[tokio::test]
    async fn marked_indexing_files_are_not_ready() {
        let w = FileWatcher::new(PathBuf::from("."));
        insert_pending(&w, "a.ts", now_ms() - 10_000).await;
        w.mark_indexing(&["a.ts".to_string()]).await;

        assert!(w.get_ready_files(500).await.is_empty());
    }

    #[tokio::test]
    async fn clear_pending_removes_entries() {
        let w = FileWatcher::new(PathBuf::from("."));
        insert_pending(&w, "a.ts", now_ms() - 10_000).await;
        insert_pending(&w, "b.ts", now_ms() - 10_000).await;
        w.clear_pending(&["a.ts".to_string()]).await;

        let pending = w.get_pending_files().await;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].path, "b.ts");
    }

    #[tokio::test]
    async fn live_watcher_picks_up_file_writes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let mut w = FileWatcher::new(root.clone());
        w.start(WatcherOptions::default()).await.unwrap();
        assert!(w.is_active().await);

        std::fs::write(root.join("hello.ts"), "export const x = 1;\n").unwrap();

        let mut found = false;
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if w.get_pending_files().await.iter().any(|p| p.path == "hello.ts") {
                found = true;
                break;
            }
        }
        w.stop().await;
        assert!(found, "watcher should record the new file as pending");
    }

    #[tokio::test]
    async fn events_under_ax_dir_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join(".ax")).unwrap();
        let mut w = FileWatcher::new(root.clone());
        w.start(WatcherOptions::default()).await.unwrap();

        std::fs::write(root.join(".ax/ax.db"), "not really a db").unwrap();

        tokio::time::sleep(Duration::from_millis(800)).await;
        let pending = w.get_pending_files().await;
        w.stop().await;
        assert!(
            pending.iter().all(|p| !p.path.starts_with(".ax/")),
            "files under .ax/ must not become pending: {pending:?}"
        );
    }
}
