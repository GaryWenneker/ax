//! Per-model LLM token usage stored in `~/.ax/usage.db`.

mod period;
mod store;

pub use period::{resolve_period, UsagePeriod};
pub use store::{
    open_pool, query_summary, record_from_response, record_usage, usage_db_path, ModelUsageSummary,
    UsageQuery, UsageRecord, UsageSummary,
};
