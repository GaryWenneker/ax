# Evidence: fix/review-usage (ax-usage)

Branch `fix/review-usage`, based on `9dca60e4`. This file records the final state of every finding this PR owns, the tests that cover it, and the commands of the final check run.

States follow the approved spec (`docs/specs/full-review-fixes/SPEC.md`, "What fixed means"):

- **fixed**: a test failed before the fix and passes now (RED, then GREEN), or the row says why no test is possible and names the check that covers it.
- **needs owner decision**: part of the finding needs a product choice the spec does not cover. What could be fixed safely is fixed; the open part is listed under Open items.
- **not done**: the finding still holds and no fix was made in this PR.

## Findings

Test names are in `crates/ax-usage/src` unless another crate is named. "RED" is "yes" when the test was seen to fail before the fix.

| Finding | State | RED | Tests | Commit |
|---|---|---|---|---|
| usage-001 | fixed | yes | `activity::tests::event_ids_minted_in_the_same_millisecond_are_unique`, `activity::tests::events_recorded_back_to_back_are_all_stored` | 8d45f0ba |
| usage-002 | fixed | yes | `store::tests::a_database_at_the_current_schema_is_opened_without_rerunning_the_schema`, `store::tests::a_database_at_an_older_schema_is_migrated` | b16ff351, 655f906c (store.rs part) |
| usage-003 | fixed | yes | `budget::tests::one_wrongly_typed_field_keeps_the_rest_of_the_file`, `budget::tests::thresholds_out_of_range_are_ignored_and_out_of_order_ones_are_lowered`, `budget::tests::a_lower_hard_limit_alone_is_kept_and_lowers_the_defaults_under_it`, `budget::tests::a_file_with_a_bom_is_read` | b38dadb6, 97fecb2a (review round 3) |
| usage-004 | fixed | yes | `budget::tests::saving_refuses_to_replace_an_unparseable_ax_json`, `budget::tests::saving_keeps_keys_of_a_file_with_a_bom_and_writes_without_one`, `budget::tests::saving_global_budget_refuses_an_unparseable_config`, `budget::tests::project_context_budget_keeps_other_keys`, `budget::tests::saving_budget_into_an_empty_config_file_writes_it`, `budget::tests::saving_out_of_order_thresholds_is_refused` | b38dadb6, 231f75b9, 97fecb2a, 1f77dbfa (review rounds 1, 3, 4) |
| usage-005 | fixed | yes | `context_cache::tests::an_inline_reply_is_indexed_without_a_stored_copy`, `context_cache::tests::a_stubbed_reply_keeps_its_cache_id_in_the_index` | 655f906c |
| usage-006 | needs owner decision | yes | `context_cache::tests::session_index_rows_past_the_cache_ttl_are_pruned`, `context_cache::tests::session_lookups_use_an_index` | 655f906c |
| usage-007 | fixed | yes | `cost_report::tests::sessions_without_usage_events_add_to_event_spend`, `cost_report::tests::one_cheap_event_does_not_hide_imported_spend_from_the_budget`, `cost_report::tests::unpriced_events_do_not_hide_session_spend`, `cost_report::tests::a_session_with_quoted_events_is_counted_once` | c0323d5e |
| usage-008 | fixed | yes | `cursor_state::binding_tests::turn_start_marker_does_not_erase_the_window_binding`, `cursor_state::binding_tests::two_windows_keep_their_own_binding`, `cursor_state::binding_tests::bound_write_keeps_the_plain_marker_readable`, `cursor_state::binding_tests::recent_marker_accepts_a_json_record_left_by_an_older_hook`, ax-cli `project_hooks::nested_hook_cwd_is_bound_to_the_initialized_project_and_window` | ba29f0ec |
| usage-009 | fixed | yes | `cursor_state::tests::import_counts_only_sessions_it_changed`, `savings::tests::transcript_import_does_not_wipe_state_tokens` | dc4c7876 |
| usage-010 | fixed | yes | `durable::tests::oversized_bodies_are_rejected_not_cut`, `durable::tests::a_clipped_tool_note_says_it_was_cut` | 7c221df6 |
| usage-011 | fixed | yes | `durable::tests::compact_rejects_a_first_kept_outside_this_conversation`, `durable::tests::compact_keeps_entries_from_a_valid_first_kept`, `durable::tests::compaction_in_a_fork_hides_inherited_entries` | 7c221df6 |
| usage-012 | fixed | yes | `mcp_audit::tests::outbound_preview_stays_in_its_call_cluster` | bd72c608 |
| usage-013 | fixed | yes | `mcp_audit::tests::domain_checks_ignore_keywords_inside_tool_arguments`, `mcp_audit::tests::domain_checks_need_both_halves_on_one_domain_line`, `mcp_audit::tests::domain_checks_fire_on_real_domain_lines` | a20ecdcc |
| usage-014 | fixed | yes | `mcp_audit::tests::audit_treats_verbose_logging_as_on_without_ship_toml_key` | ac55cd3b |
| usage-015 | needs owner decision | yes | `mcp_verbose_log::tests::read_since_skips_dated_files_before_the_window`, `mcp_audit::tests::log_read_bound_follows_window_or_first_timed_session_event`, `mcp_audit::tests::any_verbose_log_on_disk_sees_old_files_outside_the_window` | 580573e9 |
| usage-016 | fixed | yes | `mcp_verbose_log::tests::parse_line_timestamp_handles_multibyte_char_at_byte_18` | 778e636c |
| usage-017 | fixed | yes | `pricing::tests::exact_catalog_id_beats_longer_superstring_id`, `pricing::tests::match_kind_order_contained_longest_then_containing_shortest`, `pricing_sync::tests::lookup_price_as_of_prefers_exact_id_over_superstring` | 2230de69 |
| usage-018 | fixed | yes | `tests/pricing_override.rs::reference_price_as_of_honours_user_override` | 5f871e5f |
| usage-019 | fixed | yes | `pricing_sync::tests::failed_sync_is_recorded_as_error`, `pricing_sync::tests::successful_sync_is_recorded_as_ok` | 1ab08209 |
| usage-020 | fixed | yes | `pricing_sync::tests::price_history_is_one_model_over_calendar_days`, `pricing_sync::tests::upsert_and_history` | c643337b |
| usage-021 | fixed | yes | `replication::tests::export_leaves_out_projects_whose_path_ends_with_this_one`, `replication::tests::complete_project_history_is_scoped_persistent_queryable_and_never_echoed` | 369dcde4 |
| usage-022 | fixed | yes | `replication::tests::export_leaves_out_the_global_price_catalog`, `replication::tests::received_history_never_replaces_local_prices` | b4592a1e |
| usage-023 | fixed | yes | `store::tests::open_pool_hands_out_one_shared_pool_per_path`, `store::tests::open_pool_reopens_a_removed_database_file`, `store::tests::a_shared_pool_keeps_working_in_a_later_runtime`, `store::tests::open_pool_does_not_wait_for_a_stale_pool_to_drain` | 403d10db, 8253b83b (review round 1) |
| usage-024 | fixed | yes | `savings::tests::savings_summary_reads_the_price_catalog_once_for_all_days`, `pricing_sync::tests::lookup_price_as_of_prefers_exact_id_over_superstring` | 2f4d0b99 |
| usage-025 | fixed | yes | `savings::tests::overlapping_sessions_do_not_share_mcp_calls`, `savings::tests::a_session_that_started_before_the_period_still_owns_its_calls` | 9a398283, 3211e43f (review round 1) |
| usage-026 | fixed | yes | `savings::tests::claude_multi_block_response_counts_usage_once`, `savings::tests::claude_repeated_response_keeps_largest_usage`, `savings::tests::claude_flat_assistant_line_counts_usage_once` | 6ba9ea28 |
| usage-027 | fixed | yes | `savings::tests::imported_transcripts_store_the_project_path` | c4a7afec |
| usage-028 | fixed | yes | `spec_hierarchy::tests::selecting_a_plan_written_against_a_superseded_spec_needs_review`, `spec_hierarchy::tests::a_plan_edit_after_a_spec_change_still_needs_review`, `spec_hierarchy::tests::selecting_a_plan_for_the_approved_spec_stays_current` | cc87ddce |
| usage-029 | fixed | yes | `spec_hierarchy::tests::revising_an_unknown_spec_is_a_missing_spec` | cc87ddce |
| usage-030 | fixed | yes | `spec_registry::tests::the_approved_spec_markdown_is_not_captured_as_another_spec`, `spec_registry::tests::a_dirty_spec_file_adds_one_draft_and_keeps_the_binding` | 7ed6b68c |
| usage-031 | fixed | yes | `spec_registry::tests::an_approved_body_ending_in_a_newline_is_not_a_conflict` | 7d41f5a7 |
| usage-032 | fixed | yes | `spec_registry::tests::an_unchanged_plan_file_is_not_read_again`, `spec_registry::tests::workspace_and_user_plans_keep_a_session` | 65d22b22, 58a337ab (test added in the final check; RED shown by disabling the skip) |
| usage-033 | fixed | yes | `spec_registry::tests::plan_files_do_not_replace_an_explicit_binding`, `spec_registry::tests::a_plan_file_binding_follows_the_newest_plan` | ab68a329 |
| usage-034 | fixed | yes | `store::tests::a_busy_database_fails_within_seconds_instead_of_minutes`, `store::tests::open_pool_hands_out_one_shared_pool_per_path` | 7af8f798, b15988bb (docs) |
| usage-035 | fixed | yes | `tool_trace::tests::secrets_in_arguments_previews_and_errors_are_redacted`, `redact::tests::text_keeps_its_line_breaks_and_loses_its_secrets`, `redact::tests::json_redacts_secret_keys_and_strings`, `activity::tests::redacts_secrets_in_summary` | 6021ad4e |
| usage-036 | fixed | yes | `turn_lifecycle::tests::redaction_keeps_the_line_structure_of_prompts_and_outcomes` | 11adc8cf |
| usage-037 | fixed | yes | `turn_lifecycle::tests::finish_stores_decisions_and_open_questions_in_the_working_context`, `turn_lifecycle::tests::finish_reports_a_working_context_write_it_could_not_make` | bc3c99eb, d7aada40 (review round 3) |
| usage-038 | fixed | yes | `redact::tests::identifiers_stay_and_secrets_after_a_marker_go` | 16a785ce |
| usage-039 | fixed | yes (mutation check) | `activity::tests::redacts_secrets_in_summary`, `activity::tests::failed_and_incomplete_events_keep_their_status_and_a_redacted_error`, `activity::tests::list_filters_by_source_session_both_and_neither`, `activity::tests::the_temporary_root_is_removed_when_dropped` | 9dcfb395 |
| usage-040 | fixed | yes | `context_authority::tests::a_mandatory_rule_gets_the_budget_before_an_earlier_advisory_skill`, `context_authority::tests::receipt_status_and_the_resolver_agree_on_what_blocks` | 7fee9aac |
| usage-041 | fixed | yes | `context_authority::tests::an_approval_records_who_approved_why_and_when`, `context_authority::tests::replay_approval_requires_an_approver_member`, `context_authority::tests::b6_concurrent_manifest_approve_one_wins`, `context_authority::tests::a_manifest_stores_model_tools_and_commit_and_replay_stays_not_implemented` | e40a27e8 |
| usage-042 | fixed | yes | `context_authority::tests::the_manifest_list_is_capped_and_reports_integrity_and_the_latest_replay`, `context_authority::tests::b6_list_summaries_handles_one_hundred_manifests` | 094a7360 |
| usage-043 | fixed | yes | `context_authority::tests::a_manifest_with_padded_optional_ids_still_verifies`, `context_authority::tests::the_manifest_seal_tells_absent_fields_from_empty_or_zero_ones`, `context_authority::tests::rows_sealed_in_the_legacy_format_verify_and_are_listed_as_legacy` | c2f6e8bc, 03ba7b04 (review round 1) |
| usage-044 | fixed | yes | `context_authority::tests::receipt_items_with_shifted_separators_seal_differently`, `context_authority::tests::seal_values_with_nul_bytes_cannot_shift_into_the_next_pair`, `context_authority::tests::rows_sealed_in_the_legacy_format_verify_and_are_listed_as_legacy` | c2f6e8bc, 03ba7b04 (review round 1) |
| usage-045 | fixed | no: test-only change | Covered by the `context_authority` test module staying green (wall-clock asserts removed, leaked pools dropped, fixtures shared) | fc702af9 |
| usage-046 | fixed | no: dead-code removal | Covered by compilation, `cargo clippy -p ax-usage --all-targets -- -D warnings` and the full ax-usage suite | 4664f987 |
| usage-047 | fixed | yes | `context_export::tests::an_unknown_session_is_an_error_not_an_empty_export`, `context_export::tests::a_failed_spec_lookup_is_an_error`, `context_export::tests::the_loader_never_claims_a_closed_inventory`, `context_export::tests::the_loader_reports_stored_turns_and_does_not_invent_decisions` | e3154bb2 |
| usage-048 | fixed | yes | `context_export::tests::a_leaked_multi_line_redacted_prompt_is_caught`, `context_export::tests::a_leaked_redacted_outcome_is_caught`, `context_export::tests::a_redacted_turn_exports_no_hash_of_the_secret` | e3154bb2 |
| usage-049 | fixed | yes | `continuity::tests::conflicts_compare_instants_not_timestamp_text`, `continuity::tests::a_tie_on_the_same_instant_keeps_exactly_one_decision`, `continuity::tests::historical_mode_compares_the_cutoff_as_an_instant`, `continuity::tests::a_checkpoint_reports_the_numerically_highest_version` | b0474069 |
| usage-050 | fixed | yes | `cursor_state::tests::model_config_skips_param_without_id_and_reads_bool_values` | 96dd9c8d |
| usage-051 | fixed | yes | `durable::tests::read_returns_one_page_of_the_working_view`, `durable::tests::an_entry_body_cannot_pose_as_another_entry_or_close_the_block`, `durable::tests::search_ignores_case_including_non_ascii_and_caps_hits`, `durable::tests::a_non_ascii_search_reads_past_the_first_batch` | 4cbe4470 |
| usage-052 | fixed | yes | `mcp_audit::tests::transcript_fallback_ignores_parent_and_sibling_workspaces`, `mcp_audit::tests::subagents_in_the_project_slug_does_not_hide_transcripts` | c2df2fa3, 2b661be7 (docs) |
| usage-053 | fixed | yes | `mcp_audit::tests::lsp_probe_runs_only_when_the_cheap_conditions_hold`, `mcp_audit::tests::version_check_gives_up_on_a_hanging_binary`, `mcp_audit::tests::version_check_accepts_a_quick_success` | 04253e5d |
| usage-054 | fixed | yes | `output_format::tests::save_refuses_to_replace_an_unparsable_config`, `output_format::tests::save_keeps_other_settings`, `output_format::tests::only_ax_tags_are_rewritten`, `output_format::tests::save_accepts_a_config_with_a_bom_or_only_a_bom` | 82d75778, 2048c33e (review round 3) |
| usage-055 | not done | no | none | none |
| usage-056 | not done | no | none | none |
| usage-057 | not done | no | none | none |
| usage-058 | not done | no | none | none |
| usage-059 | not done | no | none | none |
| usage-060 | not done | no | none | none |
| usage-061 | fixed | yes | `spec_hierarchy::tests::reads_do_not_need_the_write_lock_after_the_schema_exists` | 965b1438 |
| usage-062 | fixed | yes | `spec_registry::tests::the_spec_tables_have_one_shape_whichever_module_creates_them` | 7000cc2e |
| usage-063 | fixed | yes | `spec_hierarchy::tests::concurrent_edits_of_one_version_get_one_win_and_conflict_payloads` | 16fe5ef5 |
| usage-064 | fixed | yes | `spec_hierarchy::tests::an_unchanged_plan_is_queued_once_across_turns` | 2ca16422 |
| usage-065 | fixed | yes | `spec_registry::tests::approving_with_a_duplicate_linked_file_stores_one_seal`, `spec_registry::tests::a_failed_markdown_write_leaves_the_revision_unapproved` | e35f76e9 |
| usage-066 | fixed | yes | `spec_registry::tests::concurrent_drift_checks_store_one_note_and_all_report_drift`, `spec_registry::tests::a_failed_drift_check_is_reported_as_unknown_not_as_no_drift` | 258c2fb5 |
| usage-067 | fixed | yes | `spec_registry::tests::plan_files_with_one_name_in_two_folders_stay_two_specs`, `spec_registry::tests::long_plan_prompts_with_one_beginning_stay_two_specs`, `spec_registry::tests::a_flat_plan_folder_is_cut_at_the_file_limit`, `spec_registry::tests::a_long_plan_file_captured_under_its_legacy_id_keeps_that_id`, `spec_registry::tests::a_long_multi_line_plan_prompt_keeps_its_legacy_id` | e28f91bb, ce3779bd (review round 3), 71b863ab (review round 4) |
| usage-068 | fixed | yes (spec tests); no for the AX_USAGE_DB guard, see the commit | `spec_registry::tests::a_linked_file_change_does_not_rewrite_the_approved_body`, `spec_registry::tests::an_edited_markdown_file_is_a_conflict`, `spec_registry::tests::a_bound_turn_can_show_the_version_it_received` | ef68165c |
| usage-069 | fixed | yes | `store::tests::a_failed_additive_migration_is_an_error_and_is_retried`, `store::tests::reopening_a_migrated_database_ignores_existing_columns` | 68aec27d |
| usage-070 | fixed | yes | `tokenizer::tests::one_miss_then_one_hit_while_other_threads_count_files` | 5600acf4 |
| usage-071 | fixed | yes | `turn_lifecycle::tests::session_and_turn_ids_cannot_leave_the_turns_folder` | fada50b0 |
| usage-072 | fixed | yes | `turn_lifecycle::tests::status_records_skip_the_source_path_of_renames_and_copies`, `turn_lifecycle::tests::renamed_and_quoted_paths_are_recorded_as_real_paths`, `turn_lifecycle::tests::files_dirty_before_the_turn_are_not_attributed_to_it`, `turn_lifecycle::tests::a_project_in_a_subfolder_of_the_repo_records_root_relative_paths` | 60ae593a, f3333325 (review round 2) |
| usage-073 | fixed | yes | `turn_lifecycle::tests::turn_ids_minted_in_the_same_millisecond_differ`, `turn_lifecycle::tests::begin_never_overwrites_an_existing_turn`, `turn_lifecycle::tests::concurrent_events_on_one_turn_are_all_kept`, `turn_lifecycle::tests::concurrent_finishes_leave_one_turn_memory_and_no_stray_rows` | bbcf8453, d7aada40 (review round 3) |
| usage-074 | fixed | yes | `working_context::tests::a_new_item_does_not_clear_a_stale_fingerprint` | 9a7d8621 |
| usage-075 | fixed | yes | `working_context::tests::concurrent_adds_to_one_chat_keep_every_item`, `working_context::tests::a_corrupt_section_does_not_block_get_add_or_clear` | 7ce155e3 |

Totals: 67 fixed, 2 need an owner decision (usage-006, usage-015), 6 not done (usage-055 to usage-060), 0 not a defect.

## Review rounds

| Round | Findings | Fixed |
|---|---|---|
| 1 | 4 | 4 |
| 2 | 1 | 1 |
| 3 | 4 | 4 |
| 4 | 2 | 2 |

Round commits: 8253b83b, 3211e43f, 231f75b9, 03ba7b04 (round 1); f3333325 (round 2); 97fecb2a, ce3779bd, 2048c33e, d7aada40 (round 3); 1f77dbfa, 71b863ab (round 4).

## Final check run

All commands ran in `/Users/gary/io/ax-wt/usage`. The ax-usage test, clippy and fmt rows ran at 58a337ab; the ax-cli and site rows ran at 71b863ab, and the only change since is a new ax-usage unit test.

| Command | Result |
|---|---|
| `cargo test -p ax-usage` | pass: lib 490 passed, 0 failed, 1 ignored; `tests/pricing_override.rs` 1 passed; doc tests 0 |
| `cargo clippy -p ax-usage --all-targets -- -D warnings` | pass, no warnings in the crate |
| `cargo clippy -p ax-cli --all-targets -- -D warnings` (ax-cli callers were changed) | pass |
| `cargo test -p ax-cli --test project_hooks` | pass: 2 passed, 0 failed |
| `cargo test -p ax-cli --bin ax` | pass: 145 passed, 0 failed |
| `cargo fmt --all --check` | pass, no differences |
| `npm ci` then `npm test` in `site` (site docs were changed) | pass: 89 tests, 89 passed, 0 failed |
| `npx astro build` in `site` | pass: 49 pages built |

Not run: the full `site` build script (it also writes the Azure template and renders OG images, which the doc changes do not touch), the whole-workspace test suite (out of scope for this PR), and the ax-mcp integration test `server::reuse_integration::durable_transcript_records_a_tool_and_compact_keeps_the_original_searchable` (it ran green during usage-011 and was not rerun in this final pass).

## Open items

- **usage-006 (owner decision):** how long to keep `ax_activity_event` rows (activity history in Command Center Logging) and `ax_durable_entry` rows (durable transcripts). The session index is pruned and indexed; the other two tables grow without limit until the owner picks a default age and a setting.
- **usage-015 (owner decision):** automatic retention of verbose MCP log files (keep forever, N days, or a byte cap, and whether it is a `ship.toml` setting). Reading is now bounded to the audit window; old files are not deleted, and the guide says so.
- **usage-055 (not done):** `period.rs` still errors on a nonexistent or ambiguous local midnight (DST at midnight) and does not reject `from > to`.
- **usage-056 (not done):** `pricing.rs` `user_override_for` re-reads and re-parses `pricing.toml` on every call.
- **usage-057 (not done):** `#[allow(dead_code)]` on the Artificial Analysis, benchmark and coding-agent types in `pricing_fetch.rs` and `pricing_sync.rs`, the per-row benchmark query in `list_latest_prices`, and the unused `days` argument of `list_coding_agents`.
- **usage-058 (not done):** `reuse_cache.rs` `session_entries` hashes files per row on the async thread with no memo.
- **usage-059 (not done):** the transcript import upsert uses `>=` on `source_mtime`, reads files with blocking `std::fs`, and upserts events without one transaction per session.
- **usage-060 (not done):** savings tests set `AX_SAVINGS_CF_MODE` and `AX_USAGE_DB` without a shared lock or RAII restore, and pricing and reuse-cache tests are not isolated from the developer's `~/.ax/pricing.toml`.
- usage-033 keeps the live-session shortcut that binds the newest `~/.cursor/plans` file across chats, because those files carry no project marker and an existing test requires it.
- usage-041 does not derive `actor_kind` from an authenticated transport, because no transport calls the approval functions yet.
- usage-049 keeps the continuity scenario fixtures in the public API; moving them behind `#[cfg(test)]` would remove exported items.
