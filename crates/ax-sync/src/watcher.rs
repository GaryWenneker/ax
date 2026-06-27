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

    pub async fn start(&mut self, opts: WatcherOptions) -> Result<(), ax_utils::errors::AxError> {
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
