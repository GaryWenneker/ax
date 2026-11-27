//! Off-main-loop read-tool dispatch — CG: mcp/query-pool.ts (async semaphore MVP).

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::Semaphore;

pub const QUERY_POOL_SIZE_ENV: &str = "AX_QUERY_POOL_SIZE";
const MAX_POOL_SIZE: usize = 16;

pub fn resolve_pool_size() -> usize {
    pool_size_from(std::env::var(QUERY_POOL_SIZE_ENV).ok().as_deref())
}

/// The pool size for a raw `AX_QUERY_POOL_SIZE` value (`None` when unset).
fn pool_size_from(value: Option<&str>) -> usize {
    match value {
        None => default_pool_size(),
        Some(s) if s.trim().is_empty() => default_pool_size(),
        Some(s) => {
            let n = s.parse::<i64>().unwrap_or(-1);
            if n <= 0 {
                0
            } else {
                (n as usize).min(MAX_POOL_SIZE)
            }
        }
    }
}

fn default_pool_size() -> usize {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    cpus.saturating_sub(1).clamp(1, MAX_POOL_SIZE)
}

pub fn is_read_tool(name: &str) -> bool {
    matches!(
        name,
        "ax_explore"
            | "ax_search"
            | "ax_node"
            | "ax_status"
            | "ax_files"
            | "ax_context"
            | "ax_callers"
            | "ax_callees"
            | "ax_impact"
            | "ax_cycles"
            | "ax_path"
            | "ax_api"
            | "ax_affected"
            | "ax_recall"
            | "ax_history"
    )
}

pub struct QueryPool {
    semaphore: Arc<Semaphore>,
    healthy: AtomicBool,
}

impl QueryPool {
    pub fn new(size: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(size.max(1))),
            healthy: AtomicBool::new(true),
        }
    }

    pub fn healthy(&self) -> bool {
        self.healthy.load(Ordering::SeqCst)
    }

    pub async fn run<F, Fut, T>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<T, String>> + Send,
        T: Send,
    {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        let result = f().await;
        drop(permit);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_size_zero_disables() {
        // Tested without touching the process environment, which engines
        // built by tests running in parallel read.
        assert_eq!(pool_size_from(Some("0")), 0);
        assert_eq!(pool_size_from(Some("-3")), 0);
        assert_eq!(pool_size_from(Some("many")), 0);
        assert_eq!(pool_size_from(Some("2")), 2);
        assert_eq!(pool_size_from(Some("100000")), MAX_POOL_SIZE);
        assert_eq!(pool_size_from(None), default_pool_size());
        assert_eq!(pool_size_from(Some("  ")), default_pool_size());
    }

    #[test]
    fn read_tool_names() {
        assert!(is_read_tool("ax_explore"));
        assert!(!is_read_tool("ax_index"));
    }
}
