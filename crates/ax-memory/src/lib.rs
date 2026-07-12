//! ax-memory: persistent memory vault (decisions, fixes, conventions) in ax.db.
//!
//! Memories live in the project graph database and are recalled with FTS5
//! ranking combined with confidence decay, so fresh knowledge outranks stale.

pub mod capture;
pub mod embed;
pub mod format;
pub mod store;
pub mod types;

pub use capture::{capture_git_history, GitCaptureResult};
pub use format::format_memories_inject_block;
pub use store::{
    delete, effective_confidence, find_similar, fts_query_from_text, get, list, recall, remember,
    update,
};
pub use types::{MemoryMatch, MemoryRow, RememberInput, MEMORY_KINDS};

use sqlx::SqlitePool;

/// Top memories for a user prompt, used by `ax_preflight` injection.
/// Only returns confident matches to keep the inject small.
pub async fn recall_for_prompt(
    pool: &SqlitePool,
    prompt: &str,
    limit: usize,
) -> Result<Vec<MemoryMatch>, ax_utils::errors::AxError> {
    let mut matches = recall(pool, prompt, limit).await?;
    matches.retain(|m| m.score > 0.0);
    Ok(matches)
}
