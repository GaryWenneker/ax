//! Shared MCP engine with lazy Ax initialization.

use std::path::PathBuf;
use std::sync::Arc;

use ax_context::directory::find_nearest_ax_root;
use ax_core::Ax;
use tokio::sync::Mutex;

use crate::query_pool::{QueryPool, resolve_pool_size};

pub struct McpEngine {
    ax: Arc<Mutex<Option<Ax>>>,
    project_root: Option<PathBuf>,
    query_pool: Option<QueryPool>,
}

impl McpEngine {
    pub fn new() -> Self {
        Self {
            ax: Arc::new(Mutex::new(None)),
            project_root: None,
            query_pool: None,
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
        }
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