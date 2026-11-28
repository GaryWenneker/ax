# Evidence: policy, skill catalog and quality (fix/review-policy)

This PR owns `ax-policy`, `ax-skill-catalog` and `ax-quality`. Base commit: `9dca60e4`. Every finding below is in state **fixed**. "RED" means a test was seen failing before the fix and passing after it.

## Findings

| Finding | State | RED | Tests that cover it | Commit |
|---|---|---|---|---|
| policy-001 | fixed | yes | `ide_seed::tests::an_existing_clinerules_without_an_ax_block_keeps_its_team_rules`, `an_existing_clinerules_keeps_text_before_and_after_the_ax_block`, `a_clinerules_directory_is_skipped_and_the_seed_continues`, `a_seeded_clinerules_is_left_unchanged_on_reseed` | a6c49bf5 |
| policy-002 | fixed | yes | `paths::tests::an_absolute_source_outside_project_and_roots_is_rejected`, `an_absolute_source_inside_a_configured_root_is_allowed`, `a_source_that_is_not_a_policy_file_is_rejected`, `a_project_source_that_is_a_policy_file_is_allowed`, `a_project_source_linked_outside_is_rejected`, `a_root_source_with_an_absolute_rest_is_rejected`; `pack::provenance_tests::pack_import_never_writes_a_rule_source_outside_policy_files` | f36090a9 |
| policy-003 | fixed | yes | `zip_package::tests::restore_never_writes_a_double_slash_member_outside_agents`, `zip_entry_names_with_odd_segments_are_refused` | 6e352d1f |
| policy-004 | fixed | yes | `wellknown::tests::a_tar_entry_with_an_absolute_path_is_refused`, `a_tar_entry_with_a_parent_dir_is_refused`, `a_tar_link_entry_is_skipped`, `a_plain_tar_installs` | 24e9f28a |
| policy-005 | fixed | yes | `capture::tests::a_prompt_whose_lowercase_changes_byte_length_does_not_panic`, `a_marker_is_found_case_insensitively_on_the_original_text`, `directive_prefixes_and_markers_are_ascii_for_the_case_insensitive_search` | c31d6b2a |
| policy-006 | fixed | yes | `capture::tests::a_long_non_ascii_title_is_cut_on_a_character_boundary` | c31d6b2a |
| policy-007 | fixed | yes | `config::tests::writers_refuse_an_unparseable_config_and_leave_it_untouched`, `writers_create_a_missing_config`; locking added in review round 1: `parallel_writers_of_different_keys_keep_every_key` | 8d5f03ee, f2fb5827 |
| policy-008 | fixed | yes | `agents_share::tests::agents_dir_ignores_an_unsafe_configured_value` | 13d7c80b |
| policy-009 | fixed | yes | `guard::tests::dot_dot_segments_do_not_escape_rule_globs`, `paths_outside_the_project_are_refused`, `a_link_out_of_the_project_is_refused`, `globs_ignore_case_on_case_insensitive_platforms` | b24a0276 |
| policy-010 | fixed | yes | `guard::tests::a_broken_regex_directive_blocks_instead_of_vanishing`, `a_broken_glob_blocks_instead_of_never_matching`, `forbid_content_checks_content_that_is_not_utf8`, `a_write_without_content_reports_unchecked_content_rules` | 8183d902 |
| policy-011 | fixed | yes | `guard::tests::secrets_rule_covers_common_secret_file_names` (extended in review rounds 3 and 4), `builtin_checks_need_a_tag_or_a_whole_word_in_the_rule_id` | bc870d8a, 5722c4c5, 10d965a6 |
| policy-012 | fixed | yes | `history::tests::a_damaged_line_is_skipped_instead_of_failing_every_seed`, `an_append_after_a_torn_last_line_starts_on_a_new_line`, `parallel_appends_keep_every_line_whole` | 096da137 |
| policy-013 | fixed | yes | `ide_seed::tests::a_hand_written_agents_md_does_not_mark_ax_files_as_removed`, `a_shared_file_with_the_ax_block_still_marks_an_existing_install` | e9c9e65c |
| policy-014 | fixed | yes | `index::tests::a_rule_that_stops_parsing_keeps_its_row_and_is_reported`; review rounds 1 and 3: `a_file_that_parses_but_fails_validation_loses_its_row`, `types::tests::a_skip_says_whether_its_row_was_kept` | ef9099d5, 5e910833, efb5e387, 3977201c |
| policy-015 | fixed | yes | `index::tests::an_unchanged_import_writes_nothing` | 8c11dd6d |
| policy-016 | fixed | yes | `parse::tests::a_skill_name_must_be_kebab_case`, `index::tests::export_refuses_a_skill_name_that_leaves_the_output_folder`, `store::tests::save_skill_refuses_a_name_that_is_not_kebab_case_before_writing` | d153dee1 |
| policy-017 | fixed | yes | `index::tests::a_missing_file_owned_source_does_not_fail_database_mode` | ffec5fe6, 5e910833 |
| policy-018 | fixed | yes | `matcher::tests::skill_trigger_needs_a_whole_word`, `skill_description_ignores_punctuation_around_prompt_words` | 6dd2acc5 |
| policy-019 | fixed | yes | `matcher::tests::a_relative_file_resolves_against_the_input_cwd`, `a_new_file_under_a_linked_cwd_keeps_its_relative_path` | 5c8dcc9a, 6da9358f |
| policy-020 | fixed | yes | `migrate::tests::scan_leaves_pending_private_and_inactive_files_alone`, `scan_ignores_rule_and_skill_files_outside_policy_folders`, `duplicate_ids_resolve_by_layer_precedence_not_walk_order` | ae835047, 6da9358f |
| policy-021 | fixed | yes | `pack::export_tests::export_refuses_a_folder_that_is_not_a_pack`, `export_into_an_existing_pack_replaces_only_pack_contents`, `export_into_an_empty_or_new_folder_works` | 7ee88701 |
| policy-022 | fixed | yes | `pack::import_id_tests::an_entry_whose_file_names_another_id_cannot_overwrite_it`, `a_mismatched_entry_is_not_staged_for_review_either` | 7ee88701 |
| policy-023 | fixed | yes | `parse::tests::skill_free_text_survives_a_save_unchanged`, `rule_free_text_survives_a_save_unchanged` | 5f7fa5c4 |
| policy-024 | fixed | yes | `review::tests::one_bad_pending_file_does_not_hide_the_others`, `a_good_item_can_be_diffed_next_to_a_bad_one`, `showing_a_bad_item_names_its_file_and_field`, `approving_a_bad_item_names_its_file_and_field`; `pack::import_parse_tests::an_unparsable_pack_file_is_listed_and_the_rest_imports` | 86d50c67 |
| policy-025 | fixed | yes | `review::tests::the_diff_shows_an_active_rule_that_lives_in_agents`, `the_kind_picks_the_skill_when_a_rule_shares_its_id` | 1d0a211b |
| policy-026 | fixed | yes | `seed::tests::restore_item_writes_one_deleted_skill_back_by_its_name`, `restore_item_needs_a_kind_when_a_rule_and_a_skill_share_the_id`, `restore_item_writes_one_deleted_rule_back` | 7a431f73 |
| policy-027 | fixed | yes | `stacks::tests::a_file_the_user_had_before_apply_survives_upgrade_and_remove`, `upgrade_skips_user_edit`, `apply_two_stacks_is_idempotent_and_respects_edits` | 1e7410d5 |
| policy-028 | fixed | yes | `store::tests::save_rule_refuses_a_bad_id_before_writing_any_file`, `lookups_ignore_a_path_like_id`, `save_skill_refuses_a_name_that_is_not_kebab_case_before_writing` | d82d8e47 |
| policy-029 | fixed | yes | `store::tests::a_rename_that_fails_validation_keeps_the_old_rule`, `a_rename_moves_the_rule_to_its_new_id`, `a_rename_keeps_the_shared_source_file` | b8745cfc |
| policy-030 | fixed | yes | `store::tests::disabling_a_skill_leaves_the_rule_with_the_same_name` | 1ed7dfef |
| policy-031 | fixed | yes | `zip_package::tests::zip_member_that_expands_past_the_entry_limit_is_refused`, `zip_members_that_expand_past_the_total_limit_are_refused`, `zip_with_too_many_members_is_refused`, `zip_members_within_the_limits_are_read`, `diff_of_huge_inputs_stays_bounded_and_correct` | eeb913c5 |
| policy-032 | fixed | yes | `ax-quality bootstrap::tests::persist_token_writes_the_file_and_leaves_the_process_environment_alone`, `persist_token_file_is_readable_by_the_owner_only`; `ax-utils fs_safe::tests::atomic_write_private_leaves_a_0600_file` (review round 1) | cc15632c, 96fe5bf2 |
| policy-033 | fixed | yes | `bootstrap::tests::migration_leaves_projects_ax_did_not_create_alone`, `migration_renames_only_a_project_ax_recorded_and_never_deletes`; opt-in from review round 1: `migrate_legacy_keys_adopts_and_renames_legacy_projects_without_a_record`, `migrate_legacy_keys_is_off_unless_the_config_sets_it` | df92e09c, f7b4991b |
| policy-034 | fixed | yes | `bootstrap::tests::user_env_command_refuses_names_that_are_not_plain_identifiers`, `user_env_command_keeps_name_and_value_off_the_command_line` | cc15632c |
| policy-035 | fixed | yes | `bootstrap::tests::sonar_is_ready_only_when_status_is_up`, `wait_for_sonar_gives_up_at_the_deadline_when_a_request_hangs`, `container::tests::status_body_is_up_only_for_up` | 5c825159 |
| policy-036 | fixed | yes | `container::tests::sonar_is_published_on_loopback_for_a_local_host`, `sonar_is_published_on_all_interfaces_only_for_a_named_host`, `scanner_reaches_an_ax_managed_sonar_over_its_network`; review round 3: `scanner_joins_the_sonar_network_when_no_container_name_is_set`, `scanner_uses_the_runtime_that_holds_the_sonar_container`, `scanner_falls_back_to_the_host_url_off_the_sonar_network` | 649b2336, 0a7bedcb |
| policy-037 | fixed | not seen in the final run (see note) | `ax-utils child::tests::output_is_captured_not_inherited`, `a_hanging_command_is_killed_at_the_deadline`, `stdin_is_closed`, `a_background_grandchild_holding_the_pipes_does_not_block_the_result`; `ax-quality container::tests::a_stuck_cli_neither_hangs_install_nor_blocks_the_runtime` | 43f3c332, d73dc42e, 11e55fe5 |
| policy-038 | fixed | not seen in the final run (see note) | `container::tests::install_leaves_a_sonarqube_container_ax_did_not_create_alone`, `ax_labels_the_containers_it_creates` | 09ceb747, d73dc42e |
| policy-039 | fixed | yes | `sonar::tests::gate_for_one_repo_of_a_multi_repo_workspace_reads_the_scanned_key`, `skipped_repo_of_a_multi_repo_workspace_reports_its_prefixed_key`, `scan_and_gate_keys_agree` | 6e3d5fa0 |
| policy-040 | fixed | yes | `sonar::tests::gate_waits_for_the_submitted_analysis_and_reads_its_status`, `gate_does_not_pass_when_no_status_was_computed`, `gate_fails_when_the_analysis_task_failed`, `gate_is_pending_when_the_analysis_stays_queued`, `without_compute_engine_status_the_gate_polls_past_none`, `snapshot_does_not_wait_and_keeps_none_neutral` | a1a6272e |
| policy-041 | fixed | not seen in the final run (see note) | `sonar::tests::a_scanner_that_writes_a_lot_of_output_still_finishes`, `a_failed_scanner_reports_the_last_stderr_line`; `ax-utils child::tests::only_the_tail_of_a_large_stream_is_kept` | 43f3c332, 11e55fe5 |
| policy-042 | fixed | yes (fix reverted temporarily, test failed, fix restored) | `sonar::tests::multi_repo_inclusions_are_relative_to_the_scan_base_directory` | 6b9ed426 |
| policy-043 | fixed | yes | `audit::tests::a_symlinked_skill_md_is_not_read`, `find_skill_dir_does_not_pick_a_symlinked_skill` | f1e6f1ab |
| policy-044 | fixed | yes | `audit::tests::one_bad_audit_entry_does_not_hide_the_others`, `store_report_leaves_no_temp_files` (guard, passed before) | 267fcc99 |
| policy-045 | fixed | yes | `audit::tests::code_fences_and_non_path_tokens_are_not_missing_files` | 353b6a7f |
| policy-046 | fixed | yes | `install::tests::installs_a_skill_whose_skill_md_is_at_the_source_root` | 02d2a3a3 |
| policy-047 | fixed | yes | `publish::tests::stages_next_to_skills_published_earlier`, `refuses_bulk_and_stages_one` | 9fb4b160 |
| policy-048 | fixed | yes | `ax-cli commands::skill_catalog::tests::downloads_refuse_private_addresses`; review round 1: `remote::tests::an_allow_listed_private_host_is_fetched_and_others_are_refused`, `settings::tests::allow_private_hosts_round_trips` | 3c9ed20a, 49bb451a |
| policy-049 | fixed | yes | `remote::tests::a_git_clone_from_a_silent_server_stops_at_the_deadline`, `git_commands_never_prompt`, `git_commands_run_in_the_c_locale` (review round 1); `ax-utils child::tests::stdin_is_closed`, `a_hanging_group_and_its_children_are_killed_at_the_deadline`, `only_the_tail_of_a_large_stream_is_kept` | 4e1ad639, 61dc49cf, 11e55fe5 |
| policy-050 | fixed | yes | `remote::tests::mapped_shared_and_this_network_addresses_are_private`, `ipv6_literals_are_checked_as_addresses_not_resolved`, `the_connection_uses_the_vetted_addresses`, `a_redirect_is_vetted_and_pinned_again`, `skill_downloads_reject_private_and_non_http_targets` | 2ce109c6 |
| policy-051 | fixed | yes | `search::tests::security_audits_without_main_cut_at_a_char_boundary` | 23ecccee |
| policy-052 | fixed | yes | `wellknown::tests::a_failed_replace_keeps_the_installed_skill`, `a_skill_whose_frontmatter_names_another_skill_is_refused`, `a_failed_first_install_leaves_nothing_behind`, `an_index_name_that_is_not_a_skill_name_is_refused`, `a_replace_swaps_in_the_new_version`; `install::tests::a_replace_swaps_the_whole_directory_and_leaves_no_staging` | 92f32dbe |
| policy-053 | fixed | yes (body cap without Content-Length: no RED, the old code also failed, after buffering) | `wellknown::tests::a_zip_that_expands_past_the_cap_stops_extracting`, `a_tar_that_expands_past_the_cap_stops_extracting`, `a_file_list_longer_than_the_cap_is_refused_before_fetching`, `a_file_list_past_the_byte_cap_stops_fetching`, `a_zip_entry_escaping_the_archive_is_refused`, `a_plain_zip_installs`; `remote::tests::a_body_past_the_cap_is_refused_without_a_content_length` | 837b876a |
| policy-054 | fixed | yes (WARN mapping) | `sonar::tests::a_warn_gate_passes_and_keeps_its_conditions`, `a_gate_api_error_is_unknown_and_not_passed`, `a_multi_repo_gate_reads_every_repo_key_and_fails_on_one_error`, `a_disabled_client_skips_the_gate`, `groups_dirty_files_per_repo` | d93cd342 |
| policy-055 | fixed | yes | `agents_share::tests::cursor_skill_symlink_follows_configured_agents_dir`, `guard::tests::require_skill_exempts_the_configured_agents_dir_only`, `builtin_packs::tests::stack_install_reports_the_configured_agents_dir`, `capture::tests::finalized_preview_path_uses_the_configured_agents_dir` | 65e9226b |
| policy-056 | fixed | yes | `bootstrap::tests::user_nodes_and_edges_only_raise_warnings`, `a_broken_seed_entity_fails_before_any_file_is_written`, `project_rule_ids_must_be_kebab_case` | 5bc74d17 |
| policy-057 | fixed | yes | `config::tests::dot_ax_json_alias_is_read_and_written_for_every_setting` | 6788d658 |
| policy-058 | fixed | yes | all `ax-policy` lib tests under `AX_HOME_DIR` with a `storage=database` global config (5 failed before); `capture::tests::non_ascii_prompts_produce_a_safe_proposal` | f9069cb4 |
| policy-059 | fixed | yes | `hierarchy::tests::policy_layers_loads_the_config_once`, `config::tests::workspace_roots_with_a_member_filter_apply_to_that_member_only` (refactor guard) | 0a35aa4f |
| policy-060 | fixed | yes | `ide_seed::tests::an_edited_continue_mcp_config_is_kept_unless_forced`, `instruction_blocks_carry_the_shared_memory_sentence` | 56bfea36 |
| policy-061 | fixed | yes | `index::tests::disk_stale_is_false_right_after_import_with_a_shadowed_id`, `disk_stale_is_false_right_after_import_of_a_stub`, `disk_stale_sees_a_deleted_rule_and_skill`, `disk_stale_ignores_database_owned_rows` | 2765b920 |
| policy-062 | fixed | yes | `matcher::tests::contextual_skills_rank_by_match_score_before_priority` | 85798cd1, 84bf94b1 |
| policy-063 | fixed | yes | `parse::tests::frontmatter_closes_only_on_a_line_that_is_exactly_three_dashes`, `empty_frontmatter_and_crlf_split`, `an_unknown_status_or_scope_is_a_validation_error` | 8a9493ff |
| policy-064 | fixed | yes | `stacks::tests::detect_works_in_a_project_inside_or_named_like_an_ignored_folder` | 5446d7f2 |
| policy-065 | fixed | yes | `stacks::tests::apply_keeps_every_installed_stack_in_the_config`, `status_reports_an_unedited_older_template_as_upgradable_not_drifted`, `status_does_not_create_policy_folders` | 5446d7f2 |
| policy-066 | fixed | not recorded | `ax-quality bootstrap::tests::ensure_sonar_projects_checks_reachability_once`, `ensure_sonar_ready_for_scan_checks_reachability_once`, `the_http_client_is_shared`, `container::tests::discover_probes_each_runtime_and_container_once` | bd06bbe0 |
| policy-067 | fixed | not recorded | `bootstrap::tests::encode_query_percent_encodes_utf8_bytes`, `token_urls_encode_the_token_name` | e3d95964 |
| policy-068 | fixed | no (already fixed by the policy-037/041 change; regression test added) | `container::tests::ensure_running_keeps_container_commands_off_the_runtime` | 9a9f19ee |
| policy-069 | fixed | not recorded | `sonar::tests::native_scan_passes_the_token_in_the_environment_only`, `container_scan_passes_the_token_name_only`, `a_scan_without_a_token_sets_no_token_variable`; review rounds 1 and 2: `the_scanner_version_is_read_from_its_banner`, `an_old_native_scanner_is_refused_before_it_runs_without_the_token` | 3cb9acc8, 88c8ee8e, 21ac0f2d |
| policy-070 | fixed (dead code removed) | n/a: no test can run unreachable code; covered by the unchanged scan tests and clippy | same tests as policy-069 | 3cb9acc8 |
| policy-071 | fixed | not recorded | `ax-skill-catalog audit::tests::brief_neutralizes_skill_controlled_text`, `verdict_caps_check_and_suggestion` | 500b120b |
| policy-072 | fixed | not recorded | `remote::tests::git_clone_refuses_option_like_and_unsupported_remotes`, `git_remote_check_accepts_https_ssh_and_scp_forms` | 872bd2f1 |
| policy-073 | fixed | not recorded | `source::tests::shorthand_rejects_names_github_does_not_allow`, `scp_style_source_for_another_host_says_to_use_https`; review round 2: `source::tests::a_skill_path_that_leaves_the_repository_is_refused`, `remote::tests::skill_tree_stays_inside_the_clone`, `skill_tree_refuses_a_link_out_of_the_clone`, `a_git_source_with_an_escaping_path_is_refused_before_cloning` | cc0453cb, c48c8ec9 |
| policy-074 | fixed | not recorded | `source::tests::self_hosted_gitlab_keeps_its_port`, `azure_query_values_are_percent_decoded`, `ref_candidates_move_path_segments_into_the_branch`; `remote::tests::a_slashed_branch_in_a_tree_url_is_found_by_retrying`, `a_clone_error_that_is_not_a_missing_branch_is_not_retried` | cc0453cb |
| policy-075 | fixed | not recorded | `wellknown::tests::a_listed_file_with_a_drive_or_unc_prefix_is_refused`, `a_listed_file_whose_name_contains_two_dots_installs` | 8123a479 |

Note on policy-037, 038 and 041: an earlier run of this batch made these fixes. The run that recorded the evidence checked them against the code but did not see the RED step itself. For policy-042 the fix was reverted for a moment and the test failed. The process tests first added in `ax-quality` and `ax-skill-catalog` `process.rs` moved to the shared runner `ax_utils::child` in review round 1 (11e55fe5). "Not recorded" for policy-066 to 075 means the batch results available when this file was written do not say whether RED was seen. Each of these commits adds the tests listed.

## Review rounds

| Round | Findings | Fixed | Commits |
|---|---|---|---|
| 1 | 8 | 8 | 49bb451a, 61dc49cf, 11e55fe5, 96fe5bf2, f2fb5827, efb5e387, f7b4991b, 88c8ee8e (plus `cargo fmt` in 49610dd0) |
| 2 | 2 | 2 | c48c8ec9, 21ac0f2d |
| 3 | 3 | 3 | 5722c4c5, 3977201c, 0a7bedcb |
| 4 | 1 | 1 | 10d965a6, 8577e43b (docs) |

## Commands run for this evidence (one run, on the branch head before this file)

| Command | Result |
|---|---|
| `cargo test -p ax-policy` | pass: 380 passed, 0 failed (lib); 0 doctests |
| `cargo test -p ax-skill-catalog` | pass: 94 passed, 0 failed |
| `cargo test -p ax-quality` | pass: 65 passed, 0 failed |
| `cargo test -p ax-utils` (changed in review round 1) | pass: 38 + 1 passed, 0 failed |
| `cargo test -p ax-cli --bins skill_catalog` (caller changed for policy-048) | pass: 1 passed, 0 failed |
| `cargo test -p ax-ship --lib` (caller changed for policy-040) | pass: 9 passed, 0 failed |
| `cargo clippy -p ax-policy --all-targets -- -D warnings` | clean |
| `cargo clippy -p ax-skill-catalog --all-targets -- -D warnings` | clean |
| `cargo clippy -p ax-quality --all-targets -- -D warnings` | clean |
| `cargo clippy -p ax-utils --all-targets -- -D warnings` | clean |
| `cargo clippy -p ax-cli -p ax-web -p ax-mcp -p ax-ship --all-targets -- -D warnings` (callers changed by this PR) | clean |
| `cargo fmt --all --check` | clean |
| `npm test` in `site/` (docs changed) | pass: 89 passed, 0 failed |
| `npm run build` in `site/` | pass |

## Open items

- The full test suites of `ax-cli`, `ax-web` and `ax-mcp` were not run in this PR. They contain small caller changes (one or a few lines each) that compile and pass clippy; their full suites run in the final workspace test on `main`.
- RED for policy-066 to 075 is not recorded in the batch results used here (see the note above).
