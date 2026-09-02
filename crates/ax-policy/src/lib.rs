pub mod agents_share;
pub mod builtin_packs;
pub mod capture;
pub mod config;
pub mod format;
pub mod guard;
pub mod hierarchy;
pub mod index;
pub mod matcher;
pub mod migrate;
pub mod pack;
pub mod parse;
pub mod paths;
pub mod ide_seed;
pub mod review;
pub mod seed;
pub mod skill_groups;
pub mod store;
pub mod types;
pub mod zip_package;

pub use capture::{
    capture_interview_questions, detect_directive, finalize_proposal, interview_instruction_text,
    propose_rule_from_prompt, resolve_unique_id, CaptureInterviewQuestion, CaptureProposal,
};
pub use config::{
    effective_storage, find_policy_root, load_policy_config, load_policy_roots,
    policy_storage_status, policy_sync_enabled, write_global_policy_storage,
    write_project_policy_storage, write_project_policy_sync, write_project_require_review,
    PolicyConfig, PolicyRoot, PolicyStorage, PolicyStorageStatus,
};
pub use builtin_packs::{
    install_builtin_pack, list_builtin_packs, BuiltinPackInfo, BuiltinPackInstallResult,
};
pub use pack::{
    default_pack_path, export_pack, import_pack, import_pack_with_options, pack_status,
    PackExportResult, PackImportResult, PackStatus,
};
pub use review::{
    approve_pending, ensure_pending_dirs, list_pending, pending_diff, reject_pending, show_pending,
    PendingDiff, PendingItem, ReviewActionResult,
};
pub use format::{build_preflight_meta, format_inject_block};
pub use guard::{guard_operation, guard_with_context};
pub use agents_share::{
    agents_dir, agents_share_violations, ensure_ax_share_gitignore, inactive_dir,
    is_git_export_candidate, legacy_policy_dir, link_cursor_skills_to_agents,
    migrate_legacy_policy_to_agents, relocate_rule_file, relocate_skill_dir,
    resolve_shareable_write_dir,
};
pub use hierarchy::{
    ensure_private_gitignore, ensure_scope_dirs, find_workspace_root, policy_dir_for_scope,
    policy_layer_dirs, policy_layers, PolicyLayer,
};
pub use index::{
    enrich_rule_row, enrich_skill_row, ensure_policy_ready, export_policy_to_files, get_rule,
    get_skill, import_policy_from_files, index_policy, list_rules, list_rules_enriched,
    list_skills, list_skills_enriched, policy_exists, policy_exists_filesystem, policy_has_content,
    policy_status, policy_tools_enabled, rule_row_to_doc, skill_row_to_doc, ExportResult,
    ImportMode,
};
pub use migrate::{
    import_migrate_candidates, migrate_interview_instruction, migrate_rule_questions,
    migrate_skill_questions, migrate_to_database, scan_policy_candidates, MigrateApplyResult,
    MigrateCandidate, MigratePlan, MigrateSkipped,
};
pub use matcher::{match_policy, max_inject_chars};
pub use parse::{
    parse_rule_file, parse_skill_file, serialize_rule, serialize_rule_stub, serialize_skill,
    serialize_skill_stub,
};
pub use paths::{
    ensure_policy_dirs, ensure_scaffold, is_stub_body, policy_root, resolve_source_path,
    rules_dir, skills_dir, STUB_BODY_MARKER,
};
pub use ide_seed::{seed_ide_agent_workflow, sync_ide_bootstrap, verify_ide_bootstrap, IdeSeedResult};
pub use zip_package::{
    build_policy_zip, decision_key, diff_policy_zip_item, preview_policy_zip, restore_policy_zip,
    default_restore_action, slug_package_filename, ItemDiff, PackSpec, PreviewItem, RestoreAction,
    RestoreResult,
    ZipPkgError, ZipPreview, ZIP_PACKAGE_MAX_BYTES,
};
pub use seed::{
    check_cursor_rule_duplicates, seed_cursor_skills, seed_default_policy, seed_global_cursor_skills,
    seed_global_policy_skills, seed_global_policy, seed_project_cursor_skills, sync_instructions, verify_content,
    verify_instructions, InstructionCheck, SeedResult, SyncResult,
};
pub use skill_groups::{catalog as skill_group_catalog, catalog_json as skill_groups_json, resolve_skill_group};
pub use store::{open_rw_pool, PolicyStore};
pub use types::*;
