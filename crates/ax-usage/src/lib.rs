//! Context-token savings metrics stored in `~/.ax/usage.db`.

mod cursor_state;
mod domain_log;
mod mcp_audit;
mod mcp_verbose_log;
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
pub use cursor_state::{
    active_cursor_session_path, cursor_state_vscdb_path, import_cursor_composer_state,
    normalize_cursor_model, parse_composer_data, parse_composer_input_tokens,
    parse_composer_model_config, read_active_cursor_session, write_active_cursor_session,
    ComposerStateRow,
};
pub use mcp_audit::{
    audit_project, cursor_project_slug, find_cursor_transcripts, format_markdown_report,
    latest_snapshot_path, load_latest_snapshot, persist_snapshot, AuditOptions, EnrichmentMetrics,
    Finding, QualitySnapshot, ToolMix, DEFAULT_WINDOW_MINUTES,
};
pub use mcp_verbose_log::{
    append_verbose_log, current_log_path, has_older_log_day, list_dated_log_files,
    mcp_verbose_log_path, migrate_legacy_log, nearest_dated_log_before, path_for_date,
    previous_calendar_day, read_log_for_day, read_merged_verbose_log, read_ship_timezone,
    rotation_calendar_date, LEGACY_LOG_NAME,
};
pub use domain_log::{
    log_action, log_domain_event, log_embed, log_lsp, log_plugin, log_share, log_ship_ci,
    log_workspace,
};
pub use savings::{
    current_assumptions, estimate_savings, import_agent_logs, is_savings_eligible_tool,
    parse_cursor_hook_model, parse_cursor_hook_session_id, query_call_token_detail,
    query_savings_summary, record_mcp_call, record_session_model_tag, spawn_record_mcp_call,
    CallTokenDetail, ImportResult, McpCallRecord, SavingsAssumptions, SavingsEstimate,
    SavingsQuery, SavingsSummary, PREVIEW_MAX_BYTES,
};
pub use store::{open_pool, usage_db_path};
pub use tokenizer::{
    count_file_tokens, count_tokens, tokenize_text, tokenizer_available, truncate_utf8,
    TokenizeResult, TOKENIZE_MAX_INPUT_BYTES, TOKENIZE_MAX_TOKENS,
};
