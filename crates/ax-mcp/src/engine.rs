//! Shared MCP engine with lazy Ax initialization.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ax_context::directory::find_nearest_ax_root;
use ax_core::Ax;
use ax_extraction::orchestrator::IndexOptions;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::query_pool::{QueryPool, resolve_pool_size};

async fn seed_memories_if_empty(pool: &SqlitePool, project_root: &Path) {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories")
        .fetch_one(pool)
        .await
        .unwrap_or(-1);
    if count != 0 {
        return;
    }
    if project_root.join(".git").exists() {
        match ax_memory::capture_git_history(pool, project_root, 100).await {
            Ok(r) => {
                eprintln!(
                    "[ax] auto-seeded memory vault: {} captured, {} trivial skipped",
                    r.captured, r.skipped_trivial
                );
                return;
            }
            Err(e) => eprintln!("[ax] git memory seed failed, falling back to graph: {e}"),
        }
    }
    match ax_memory::seed_from_graph(pool).await {
        Ok(n) if n > 0 => eprintln!("[ax] auto-seeded memory vault from graph: {n} memories"),
        Ok(_) => {}
        Err(e) => eprintln!("[ax] graph memory seed failed: {e}"),
    }
}

pub struct McpEngine {
    ax: Arc<Mutex<Option<Ax>>>,
    project_root: Option<PathBuf>,
    query_pool: Option<QueryPool>,
    catch_up_done: Arc<AtomicBool>,
}

impl McpEngine {
    pub fn new() -> Self {
        Self {
            ax: Arc::new(Mutex::new(None)),
            project_root: None,
            query_pool: None,
            catch_up_done: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_project_root(project_root: PathBuf) -> Self {
        let pool_size = resolve_pool_size();
        let query_pool = if pool_size > 0 {
            Some(QueryPool::new(pool_size))
        } else {
            None
        };
        Self {
            ax: Arc::new(Mutex::new(None)),
            project_root: Some(project_root),
            query_pool,
            catch_up_done: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the debounced file watcher and connect-time catch-up background services.
    pub fn start_background_services(project_root: &Path) {
        let _ = Ax::spawn_background_watch(project_root.to_path_buf());
    }

    pub fn query_pool(&self) -> Option<&QueryPool> {
        self.query_pool.as_ref()
    }

    pub async fn ensure_initialized(&mut self) -> Result<(), String> {
        if self.ax.lock().await.is_some() {
            return Ok(());
        }
        let root = if let Some(r) = &self.project_root {
            r.clone()
        } else {
            let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
            find_nearest_ax_root(&cwd).unwrap_or(cwd)
        };
        self.project_root = Some(root.clone());
        let ax = Ax::open(&root).await.map_err(|e| e.to_string())?;
        seed_memories_if_empty(ax.db_pool(), &root).await;
        *self.ax.lock().await = Some(ax);
        self.run_catch_up_sync().await;
        Ok(())
    }

    /// Filesystem reconciliation on first MCP session — catches edits made while the server was down.
    async fn run_catch_up_sync(&self) {
        if self.catch_up_done.swap(true, Ordering::SeqCst) {
            return;
        }
        let mut guard = self.ax.lock().await;
        let Some(ax) = guard.as_mut() else {
            return;
        };
        let opts = IndexOptions {
            quiet: true,
            ..IndexOptions::default()
        };
        match ax.sync(opts, None).await {
            Ok(result) if result.files_indexed > 0 => {
                tracing::info!(
                    "connect-time catch-up: synced {} file(s) in {}ms",
                    result.files_indexed,
                    result.duration_ms
                );
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("connect-time catch-up failed: {}", e),
        }
    }

    /// Fresh Ax handle + policy sync — avoids stale SQLite WAL in long-lived daemon.
    pub async fn ensure_policy_fresh(&mut self) -> Result<(), String> {
        let root = if let Some(r) = &self.project_root {
            r.clone()
        } else {
            let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
            find_nearest_ax_root(&cwd).ok_or_else(|| "no ax project root".to_string())?
        };
        self.project_root = Some(root.clone());
        let ax = Ax::open(&root).await.map_err(|e| e.to_string())?;
        ax.ensure_policy_ready()
            .await
            .map_err(|e| e.to_string())?;
        *self.ax.lock().await = Some(ax);
        Ok(())
    }

    pub async fn reopen_if_replaced(&mut self) -> Result<bool, String> {
        let mut guard = self.ax.lock().await;
        if let Some(ax) = guard.as_mut() {
            ax.reopen_if_replaced().await.map_err(|e| e.to_string())
        } else {
            Ok(false)
        }
    }

    pub async fn lock_ax(&self) -> tokio::sync::MutexGuard<'_, Option<Ax>> {
        self.ax.lock().await
    }

    pub fn project_root(&self) -> Option<&PathBuf> {
        self.project_root.as_ref()
    }
}