//! Concurrency, security, and utility primitives for ax.

pub mod debounce;
pub mod errors;
pub mod file_lock;
pub mod logger;
pub mod memory;
pub mod mutex;
pub mod paths;
pub mod security;
pub mod text_encoding;

pub use debounce::{debounce, Throttle};
pub use errors::*;
pub use file_lock::{FileLock, LockUnavailableError};
pub use logger::{get_logger, set_logger, Logger};
pub use memory::MemoryMonitor;
pub use mutex::AsyncMutex;
pub use paths::validate_path_within_root;
pub use security::{is_config_leaf_node, CONFIG_LEAF_LANGUAGES, SENSITIVE_PATHS};
pub use text_encoding::read_text_file;

/// Process items in batches to avoid OOM on large result sets.
pub async fn process_in_batches<T, F, Fut>(
    items: Vec<T>,
    batch_size: usize,
    mut callback: F,
) -> Result<(), AxError>
where
    T: Clone,
    F: FnMut(Vec<T>) -> Fut,
    Fut: std::future::Future<Output = Result<(), AxError>>,
{
    if batch_size == 0 {
        return Err(AxError::Config(ConfigError::new("batch_size must be > 0")));
    }
    let mut offset = 0;
    while offset < items.len() {
        let end = std::cmp::min(offset + batch_size, items.len());
        let batch = items[offset..end].to_vec();
        callback(batch).await?;
        offset = end;
    }
    Ok(())
}

/// Synchronous batch processing variant.
pub fn process_in_batches_sync<T, F>(items: Vec<T>, batch_size: usize, mut callback: F) -> Result<(), AxError>
where
    T: Clone,
    F: FnMut(Vec<T>) -> Result<(), AxError>,
{
    if batch_size == 0 {
        return Err(AxError::Config(ConfigError::new("batch_size must be > 0")));
    }
    let mut offset = 0;
    while offset < items.len() {
        let end = std::cmp::min(offset + batch_size, items.len());
        let batch = items[offset..end].to_vec();
        callback(batch)?;
        offset = end;
    }
    Ok(())
}
