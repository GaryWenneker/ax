//! Context-token savings metrics stored in `~/.ax/usage.db`.

mod period;
mod pricing;
mod savings;
mod store;
mod tokenizer;

pub use period::{resolve_period, UsagePeriod};
pub use pricing::{
    input_cost_usd, price_for_model, pricing_config_path, pricing_info, reference_pricing,
    ModelPricing, PricingInfo,
};
pub use savings::{
    current_assumptions, estimate_savings, import_agent_logs, is_savings_eligible_tool,
    query_savings_summary, record_mcp_call, spawn_record_mcp_call, ImportResult, McpCallRecord,
    SavingsAssumptions, SavingsEstimate, SavingsQuery, SavingsSummary,
};
pub use store::{open_pool, usage_db_path};
pub use tokenizer::{count_file_tokens, count_tokens, tokenizer_available};
