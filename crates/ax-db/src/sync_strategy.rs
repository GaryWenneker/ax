//! Index vs sync decisions based on project metadata (`last_full_sync`).

use crate::queries::QueryBuilder;
use ax_utils::errors::AxError;

pub const LAST_FULL_SYNC_METADATA_KEY: &str = "last_full_sync";

/// Match the global-sync TypeScript policy: weekly full rebuild at most.
const FULL_REBUILD_INTERVAL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Pure helper — `needsFullRebuild` in `src/database/sync-strategy.ts`.
pub fn needs_full_rebuild_from_timestamp(last_ms: Option<i64>, now_ms: i64) -> bool {
    let Some(ts) = last_ms else {
        return true;
    };
    if ts <= 0 {
        return true;
    }
    now_ms.saturating_sub(ts) > FULL_REBUILD_INTERVAL_MS
}

impl QueryBuilder {
    /// Returns whether the project needs a full re-index (no recent `last_full_sync`).
    pub async fn needs_full_rebuild(&self) -> Result<bool, AxError> {
        let raw = self.get_metadata(LAST_FULL_SYNC_METADATA_KEY).await?;
        let last = raw.as_deref().and_then(|s| s.parse::<i64>().ok());
        Ok(needs_full_rebuild_from_timestamp(last, now_ms()))
    }

    pub async fn record_full_sync(&self) -> Result<(), AxError> {
        self.set_metadata(LAST_FULL_SYNC_METADATA_KEY, &now_ms().to_string())
            .await
    }
}
