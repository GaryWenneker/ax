//! Context-token savings metrics stored in `~/.ax/usage.db`.

mod activity;
mod budget;
mod cache_status;
pub mod context_authority;
mod context_cache;
mod context_export;
mod context_plan;
mod continuity;
mod cost;
mod cost_report;
mod cursor_state;
mod domain_log;
mod durable;
mod log_brand;
mod mcp_audit;
mod mcp_verbose_log;
mod period;
mod pricing;
mod pricing_fetch;
mod pricing_sync;
pub mod replay_validation;
pub mod replication;
mod reuse_cache;
mod savings;
mod spec_documents;
mod spec_hierarchy;
mod spec_registry;
mod store;
mod tokenizer;
mod tool_trace;
mod turn_lifecycle;
mod working_context;

pub use activity::{
    event_to_json, list_activity, record_activity, record_cli_command, ActivityEvent,
    ActivityRecord, ActivitySource, ActivityStatus, TokenMetrics, ACTIVITY_SCHEMA_VERSION,
};
pub use budget::{
    banner, cycle_stats, daily_plan, display_amount, format_money, global_config_path, level_for,
    load_settings, load_settings_with_diagnostics, monthly_budget_usd, parse_mode, parse_settings,
    parse_settings_with_diagnostics, price_band, project_budget, save_global_budget,
    save_project_context_budget, simulate, working_schedule, BudgetDecision, BudgetLevel,
    BudgetMode, BudgetSectionPatch, BudgetSettings, BudgetSnapshot, Currency, CycleStats,
    DailyPlan, DaySpend, PriceBand, Simulation,
};
pub use cache_status::{
    cache_group_key, format_cache_status_lines, format_context_store_line, load_cache_status,
    CacheStatusSnapshot,
};
pub use context_cache::{
    cache_enabled, cache_oversized_reply, cache_project_reply, cache_threshold, expand_cached,
    expand_project_cached, format_catalog, format_session_ledger, ingest_jsonl_oversized,
    note_session_event, project_scope, recent_catalog, recent_session_catalog, session_ledger,
    spawn_note_session_event, stash_project_text, stash_text, tool_chunks_from_jsonl, CacheOutcome,
    CatalogEntry, ExpandPage, StashReceipt,
};
pub use context_export::{
    load_export_input, validate_context_export, CoverageReport, DecisionInput, ExportBundle,
    ExportInput, FailureInput, IdentityInput, RefInput, Section, TurnInput,
};
pub use context_plan::{
    compare_snapshots, cycle_efficiency, efficiency, optional_room, select_context,
    snapshot_from_parts, ContextBlock, ContextChange, ContextClass, ContextEfficiency,
    ContextSnapshot, CycleEfficiency, MEMORY_RESERVE_TOKENS,
};
pub use continuity::{
    continuity_prompt, copy_identifier, gate_failures, render_report, same_mode, score,
    seal_checkpoint, validate_continuity, Arm, Checkpoint, CopyKind, KnowledgeItem, Scenario,
    SessionIdentity, TestMode, TestReference, ValidationReport,
};
pub use cost::{calculate, Cost, CostComponent, CostConfidence, CostUsage};
pub use cost_report::{
    budget_check, build_report, collect_report, color_cost_banner, format_summary, load_events,
    quote_recorded, today_iso, CostReport, RecordedUsage,
};
pub use cursor_state::{
    active_cursor_session_path, bound_cursor_session_path, cursor_state_vscdb_path,
    import_cursor_composer_state, normalize_cursor_model, parse_composer_data,
    parse_composer_input_tokens, parse_composer_model_config, read_active_cursor_session,
    read_bound_cursor_session, read_recent_cursor_session, write_active_cursor_session,
    write_bound_cursor_session, ComposerStateRow,
};
pub use domain_log::{
    log_action, log_cli, log_domain_event, log_embed, log_lsp, log_memory, log_plugin, log_policy,
    log_share, log_ship, log_ship_ci, log_workspace,
};
pub use durable::{
    durable_apply, durable_project_apply, note_project_tool_if_open, note_tool_if_open,
};
pub use log_brand::{format_ax_mcp_trace, format_ax_tagged, AX_LOG_ICON};
pub use mcp_audit::{
    audit_project, cursor_project_slug, find_cursor_transcripts, format_markdown_report,
    latest_snapshot_path, load_latest_snapshot, persist_snapshot, AuditOptions, EnrichmentMetrics,
    Finding, QualitySnapshot, ToolMix, DEFAULT_WINDOW_MINUTES,
};
pub use mcp_verbose_log::{
    append_verbose_log, current_log_path, has_older_log_day, label_log_line, list_dated_log_files,
    mcp_verbose_log_path, migrate_legacy_log, nearest_dated_log_before, path_for_date,
    previous_calendar_day, read_log_for_day, read_merged_verbose_log, read_ship_timezone,
    replicated_log_files, replicated_log_origin, rotation_calendar_date, verbose_enabled,
    LEGACY_LOG_NAME,
};
pub use period::{resolve_period, UsagePeriod};
pub use pricing::{
    input_cost_usd, invalidate_price_cache, price_as_of, price_for_cost, price_for_model,
    price_for_model_with_source, pricing_config_path, pricing_info, reference_pricing,
    reference_pricing_with_source, refresh_price_cache_from_db, ModelPricing, PricingInfo,
};
pub use pricing_fetch::{
    aa_api_key, SOURCE_ARTIFICIAL_ANALYSIS, SOURCE_CODING_AGENTS, SOURCE_OPENROUTER,
};
pub use pricing_sync::{
    ensure_daily_pricing_sync, list_coding_agents, list_latest_prices, price_history,
    pricing_status, spawn_ensure_daily_pricing_sync, sync_pricing, CodingAgentRow,
    PricingCatalogRow, PricingHistoryPoint, PricingStatus, PricingSyncReport, SourceSyncStatus,
};
pub use reuse_cache::{
    conversation_key, format_session_context, index_fingerprint, index_matches, render_hit,
    reuse_cacheable, reuse_enabled, reuse_lookup, reuse_record_hit, reuse_session_entries,
    reuse_store, session_from_args, ContextEntry, IndexHashes, ReuseCandidate, ReuseHit,
    HOOK_SESSION_MAX_AGE, SESSION_CONTEXT_TOKENS,
};
pub use savings::{
    cli_mcp_tool, current_assumptions, estimate_savings, flush_cli_call, format_mcp_call_log,
    import_agent_logs, is_savings_eligible_tool, mcp_call_log_limit, parse_cursor_hook_model,
    parse_cursor_hook_session_id, query_call_token_detail, query_savings_summary, recent_mcp_calls,
    record_mcp_call, record_session_model_tag, spawn_record_mcp_call, stash_cli_result,
    CallTokenDetail, ImportResult, McpCallLogEntry, McpCallRecord, SavingsAssumptions,
    SavingsEstimate, SavingsQuery, SavingsSummary, MCP_CALL_LOG_DEFAULT_LIMIT,
    MCP_CALL_LOG_MAX_LIMIT, PREVIEW_MAX_BYTES,
};
pub use spec_documents::{classify_documents_from_json, import_spec_from_json, ListedPlan};
pub use spec_hierarchy::{
    alternative_plan_from_json, approve_artifact, approve_for_member, approve_from_json,
    bind_approval_actor, capture_plan_draft, compare_plan_from_json,
    confirm_compatibility_from_json, confirm_plan_compatibility, create_alternative_plan,
    create_hierarchy_from_json, create_spec_with_plan, create_workstream, dry_run_legacy,
    ensure_hierarchy_schema, gated_list, gated_preflight_block, hierarchy_enabled,
    hierarchy_flag_from_json, import_review_from_json, import_review_ids, import_spec_without_plan,
    link_policy_from_json, link_session, link_session_from_json, list_hierarchy_from_json,
    list_plan_revisions, mark_execution_ready, provenance_from_json, revise_plan,
    revise_plan_from_json, revise_spec, revise_spec_from_json, select_plan, select_plan_from_json,
    selected_plans, set_hierarchy_flag, specs_for_session, ApprovalRequest, CreatedSpecPlan,
    LegacyReport, NewAlternativePlan, NewSpecPlan, NewSpecWithoutPlan, NewWorkstream, PlanEdit,
    PlanSelection,
};
pub use spec_registry::{
    approve_revision, approved_revision, capture_finished_plan, check_linked_files,
    delivered_version, ensure_spec_schema, import_plan_mode_files, import_workspace_plans,
    is_plan_request, list_revisions, list_specs, on_turn_finished, record_delivery, revision,
    save_revision, show_spec, spec_context_block, spec_pointer_for, sync_markdown, SpecHistoryRow,
    SpecListItem, SpecPointer, SpecRevision,
};
pub use store::{open_pool, usage_db_path};
pub use tokenizer::{
    count_file_tokens, count_tokens, token_cache_status, tokenize_text, tokenizer_available,
    truncate_utf8, TokenCacheStatus, TokenizeResult, FILE_TOKEN_CACHE_CAPACITY,
    TOKENIZE_MAX_INPUT_BYTES, TOKENIZE_MAX_TOKENS,
};
pub use tool_trace::{log_tool_error, log_tool_inbound, log_tool_outbound, ToolTrace};
pub use turn_lifecycle::{
    turn_begin, turn_checkpoint, turn_event, turn_fail, turn_finish, turn_status,
    SemanticMemoryInput, TurnEventInput, TurnFinishInput, TurnFinishReceipt, TurnSnapshot,
    TurnState, TURN_SOURCE_CLI,
};
pub use working_context::{
    fork_working_context, handoff_working_context, session_nudge, working_context_apply,
    working_context_block, NUDGE_AFTER_TURNS, WORKING_CONTEXT_TOKENS,
};
