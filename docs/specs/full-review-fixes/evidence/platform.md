# Evidence: platform crates (fix/review-platform)

This PR owns `ax-global-db`, `ax-share`, `ax-installer`, `ax-desktop-client`, `ax-agent`, `ax-telemetry`, `ax-ship`, `ax-docs-catalog`, `ax-plugins`, `ratatouille` and `ax-smoke-tests`. Base commit: `9dca60e4`. Findings platform-001 to platform-030 are **fixed**; platform-031 to platform-049 are **not done** (see "Open items"). "RED" means a test was seen failing before the fix and passing after it.

## Findings

| Finding | State | RED | Tests that cover it | Commit |
|---|---|---|---|---|
| platform-001 | fixed | yes | `ax-agent profiles::tests::create_profile_refuses_an_empty_id`, `create_profile_refuses_an_agent_that_leaves_the_profiles_folder`, `remove_profile_never_deletes_a_folder_outside_the_profiles_base`, `remove_profile_never_deletes_the_agent_folder_itself`, `remove_profile_deletes_its_own_folder`; `ax-web tests/agent_profiles_guard.rs profile_routes_respect_readonly_and_refuse_escaping_ids` | 136454a5 |
| platform-002 | fixed | yes | `ax-installer targets::unparsable_config_tests::invalid_json_is_left_untouched`, `non_object_server_map_is_refused_without_panic`, `jsonc_that_still_does_not_parse_is_left_untouched`, `vscode_jsonc_with_trailing_commas_keeps_other_servers`, `valid_json_keeps_every_other_key`, `missing_or_empty_file_starts_from_an_empty_object`; `targets::strip_jsonc_tests::keeps_strings_and_removes_comments_and_trailing_commas` | edd206b1 |
| platform-003 | fixed | yes | `ax-plugins host::tests::untrusted_project_plugin_never_runs`, `trusted_plugin_runs_until_its_manifest_changes`, `unparsable_trust_store_is_not_overwritten`, `discovers_process_plugin_manifest` (adjusted); review round 1: `editing_a_trusted_plugin_script_drops_the_trust`, `editing_a_trusted_wasm_module_drops_the_trust`, `a_trust_store_from_before_folder_hashing_still_parses`, `content_hash_covers_names_and_bytes`; manual `ax init` / `ax index` / `ax plugins trust` run in a sandboxed HOME | 7994ed7a, 7bce8dbd |
| platform-004 | fixed | yes | `ax-agent config::tests::malformed_config_is_never_overwritten`, `concurrent_recent_project_updates_are_not_lost`, `saving_keeps_every_other_top_level_key` | 6b25c4cf |
| platform-005 | fixed | yes | `ax-agent cursor_auth::tests::saved_tokens_are_private_to_the_user`; review round 1: `secrets_replace_a_readable_file_without_leaving_temp_files` | 3ac36f1f, 8a92f60d |
| platform-006 | fixed | yes | `ax-desktop-client api::client::timeout_tests::sse_stream_is_not_cut_by_the_request_timeout`, `lsp_enrich_outlives_the_request_timeout` | df139726, 28ca345d |
| platform-007 | fixed | yes | `ax-desktop-client api::client::timeout_tests::unread_mcp_trace_stream_stays_bounded`, `unread_ship_event_stream_stays_bounded` | 4d7f2a2d |
| platform-008 | fixed | yes | `ax-desktop-client pages::nodes::tests::empty_result_does_not_start_another_load`, `failed_load_does_not_start_another_load` | 3a88b1aa |
| platform-009 | fixed | yes | `ax-docs-catalog config::tests::absolute_jsonl_path_is_rejected`, `parent_dir_wiki_local_is_rejected`, `every_configured_path_must_stay_inside_the_project`, `symlink_out_of_the_project_is_rejected`, `wiki_sub_paths_must_stay_relative`, `remote_that_looks_like_an_option_is_rejected`, `dot_means_the_project_root` | 1521f1ac, 7ae1b00b |
| platform-010 | fixed | yes | `ax-docs-catalog tests::stalled_clone_stops_at_the_time_limit`, `git_runs_without_prompts_or_stdin`, `clones_and_pulls_a_local_wiki` | 171ec3b5 |
| platform-011 | fixed | yes | `ax-global-db access::tests::admin_plus_approver_keeps_admin_scopes`, `opening_repairs_an_approver_projection_that_hid_admin`, `each_role_holds_every_scope_of_the_roles_below_it` | e06ac972 |
| platform-012 | fixed | yes | `ax-global-db access::tests::client_auth_skips_recent_touches`, `client_auth_survives_a_failed_touch`, `node_auth_survives_a_failed_touch` | d101c945 |
| platform-013 | fixed | yes | `ax-global-db access::tests::enrollment_registration_does_not_restore_revoked_project_access`, `authenticated_registration_cannot_restore_blocked_replica` (existing) | 431f0023 |
| platform-014 | fixed | yes | `ax-global-db access::tests::issuing_a_node_credential_revokes_the_earlier_ones` | 431f0023 |
| platform-015 | fixed | yes | `ax-global-db sync::tests::a_failed_sync_keeps_the_previous_global_rows` | 4d796078 |
| platform-016 | fixed | yes | `ax-installer cli_only::tests::install_with_reversed_markers_adds_a_block_and_keeps_the_text` | 07c9278f |
| platform-017 | fixed | yes | `ax-installer global_config_tests::an_unparsable_config_is_left_untouched`, `a_missing_config_gets_the_scaffolds`, `existing_keys_are_kept_when_scaffolds_are_added`; `ax-share config::tests::share_settings_never_replace_an_unparsable_project_config`, `share_settings_keep_the_other_project_keys`, `auth::microsoft::tests::an_unparsable_config_is_not_overwritten_with_the_client_id`, `client_id_is_merged_into_an_existing_config`; review round 1: `config::tests::share_settings_leave_no_lock_file_in_the_project` | 5437316d, ba0d1596 |
| platform-018 | fixed | yes | `ax-installer targets::mcp_path_tests::codex_block_is_valid_toml_for_a_windows_path`, `codex_block_upserts_with_process_cwd` (existing) | 9967929f |
| platform-019 | fixed | yes | `ax-installer targets::hermes_yaml_tests::install_keeps_other_servers_and_toolsets`, `uninstall_removes_only_the_ax_entries`, `install_into_an_empty_file_creates_both_blocks`, `unparsable_yaml_is_refused`; `hermes_yaml::tests::comments_and_layout_outside_the_ax_entries_are_kept`, `flow_style_falls_back_to_plain_yaml_with_the_same_value`, `a_binary_path_that_needs_quoting_round_trips`, `a_non_list_cli_toolset_is_refused` | 4544ccad |
| platform-020 | fixed | yes | `ax-installer targets::mcp_path_tests::a_user_hook_that_only_mentions_the_subcommand_is_not_touched`, `uninstall_keeps_user_hooks_that_share_a_group_with_the_ax_hook`, `ax_hook_commands_are_recognised_by_binary_and_exact_subcommand` | 76696a7b |
| platform-021 | fixed | yes | `ax-installer targets::jsonc_rewrite_tests::rewriting_a_commented_settings_file_keeps_text_and_a_backup_of_the_comments`, `targets::strip_jsonc_tests::keeps_strings_and_removes_comments_and_trailing_commas` | 8cf08946 |
| platform-022 | fixed | yes | `ax-plugins host::tests::a_plugin_that_writes_before_reading_its_input_does_not_deadlock`, `a_hung_plugin_is_stopped_after_its_timeout`; with `--features wasm`: `a_looping_wasm_plugin_is_interrupted_after_its_timeout`, `a_wasm_plugin_claiming_a_huge_output_is_refused_before_allocating`, `a_wasm_module_is_compiled_once_per_host` | fd973af0 |
| platform-023 | fixed | yes | `ax-share config::tests::the_git_token_is_never_written_to_ax_json_nor_returned_by_the_api`, `an_empty_token_keeps_the_stored_one_and_clear_token_removes_it`, `a_token_left_in_an_older_ax_json_moves_to_the_store_on_save`; ax-web web-ui `npm test` and `npm run build` | f66b50f6, 9271b963 |
| platform-024 | fixed | yes | `ax-share providers::github::tests::pull_refuses_a_subpath_that_leaves_the_clone`, `push_refuses_a_subpath_that_would_delete_outside_the_clone`, `repo_urls_and_branches_that_git_would_read_as_options_are_refused`, `accepted_repo_url_forms` | a90b3336 |
| platform-025 | fixed (path part) | yes | `ax-share providers::gitlab_api::tests::pull_refuses_tree_paths_that_escape_the_pull_dir`, `pull_refuses_parent_dirs_below_the_pack_prefix` | a53955fa |
| platform-026 | fixed | yes | `ax-share providers::gitlab_api::tests::push_without_memory_keeps_remote_shared_memory`, `push_without_a_pack_keeps_the_remote_pack` | a53955fa |
| platform-027 | fixed | yes | `ax-share providers::onedrive::tests::push_onedrive_addresses_the_shared_drive_not_me_drive`, `push_onedrive_reports_the_http_status_of_a_failed_listing`, `pull_onedrive_follows_next_link_pages` | e3428831 |
| platform-028 | fixed | yes | `ax-ship git_root::tests::sync_keeps_configured_nested_git_roots`, `sync_drops_only_configured_roots_that_are_gone`, `sync_leaves_a_single_git_root_alone`, `sonar_names_keep_configured_nested_roots` | 4ea71710 |
| platform-029 | fixed | yes | `ax-ship git_watcher::tests::a_burst_of_events_runs_one_evaluation_after_it_settles`, `events_caused_by_the_evaluation_do_not_retrigger_it`, `only_a_changed_git_state_runs_again`, `dropping_the_watch_stops_watching`; review round 1: `a_change_during_a_run_is_evaluated_after_it`, `a_change_after_the_run_s_own_commit_is_evaluated`, `a_disturbed_run_is_evaluated_again` | 97027145, b67fddb6 |
| platform-030 | fixed | yes | `ax-ship tests/auto_commit_errors.rs a_diff_error_after_the_checkpoint_reverts_it` | b05932e5 |
| platform-031 | not done | no | none | none |
| platform-032 | not done | no | none | none |
| platform-033 | not done | no | none | none |
| platform-034 | not done | no | none | none |
| platform-035 | not done | no | none | none |
| platform-036 | not done | no | none | none |
| platform-037 | not done | no | none | none |
| platform-038 | not done | no | none | none |
| platform-039 | not done | no | none | none |
| platform-040 | not done | no | none | none |
| platform-041 | not done | no | none | none |
| platform-042 | not done | no | none | none |
| platform-043 | not done | no | none | none |
| platform-044 | not done | no | none | none |
| platform-045 | not done | no | none | none |
| platform-046 | not done | no | none | none |
| platform-047 | not done | no | none | none |
| platform-048 | not done | no | none | none |
| platform-049 | not done | no | none | none |

Findings 031 to 049 were not started because the disk had less than 15 GB free when their batches ran. Each one was re-checked against the code and still holds (040 is partly stale: `profiles.rs` now has tests).

## Review rounds

| Round | Findings | Fixed |
|---|---|---|
| 1 | 5 | 5 (commits 28ca345d, 7bce8dbd, 8a92f60d, ba0d1596, b67fddb6) |
| 2 | 3 | 0 |
| 3 | 6 | 0 |
| 4 | 4 | 0 |

The batch summary counted 1 fix for round 1; the branch has one commit for each of its 5 findings. Four of those commits were made without a build because of the disk limit. They are compiled, tested and linted in the final run below, which passed. Rounds 2 to 4 changed no code.

## Final run

Run on the branch head before this file was added, with a fresh build folder (`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_INCREMENTAL=0` to stay within the disk). All commands passed.

| Command | Result |
|---|---|
| `cargo fmt --all --check` | 2 diffs in `crates/ax-share/src/config.rs` (a review round 1 change); fixed with `cargo fmt --all` in 0fd8565e, then clean |
| `cargo test -p ax-global-db` | 41 passed, 0 failed |
| `cargo test -p ax-share` | 44 passed, 0 failed |
| `cargo test -p ax-installer` | 109 passed, 0 failed |
| `cargo test -p ax-agent` | 21 passed, 0 failed |
| `cargo test -p ax-telemetry` | 3 passed, 0 failed |
| `cargo test -p ratatouille` | 32 passed, 0 failed |
| `cargo test -p ax-docs-catalog` | 25 passed, 0 failed |
| `cargo test -p ax-plugins` | 9 passed, 0 failed |
| `cargo test -p ax-plugins --features wasm` | 13 passed, 0 failed |
| `cargo test -p ax-ship` | 21 passed, 0 failed (20 unit, 1 integration) |
| `cargo test -p ax-desktop-client` | 6 passed, 0 failed |
| `cargo test -p ax-smoke-tests` | 1 passed, 0 failed |
| `cargo test -p ax-web --test agent_profiles_guard` (caller, platform-001) | 1 passed, 0 failed |
| `cargo check -p ax-cli` (caller, platform-003 `ax plugins`) | passed |
| `cargo clippy -p <crate> --all-targets -- -D warnings` for each of the 11 owned crates | 11 of 11 clean |
| `cargo clippy -p ax-plugins --all-targets --features wasm -- -D warnings` | clean |
| ax-web web-ui `npm test` | 91 passed, 0 failed |
| ax-web web-ui `npm run build` | passed; the rebuilt dist for the platform-023 UI change is committed in 9271b963 |

Total: 417 test runs passed, 0 failed (312 in the owned crates with default features, 13 with `wasm`, 1 in ax-web, 91 in the web UI).

## Open items

- platform-031 to platform-049 are not done (disk limit during their batches). The remaining work for each is listed in the batch results: ship test runner blocking and timeout (031), cargo multi-test filter (032), telemetry byte-slice panic and opt-out (033, 034), ratatouille SIGINT inheritance (035), `chunk_text` boundary panic (036), project scan cache (037), Cursor vscdb empty keys and transaction (038, 039), missing desktop and cursor tests (040), desktop startup wait and blocking calls (041, 043, 044), trace-line slice panic (042), `graph_slice` cap (045), `delete_policy_item_from` IMMEDIATE transaction (046), share git/HTTP timeouts and prompts (047), per-run share work folders (048), breaking-change detection (049).
- platform-025: requiring https before sending the GitLab `PRIVATE-TOKEN` was not changed. The spec has no safe-default row for it and platform-024 keeps http repo URLs, so the owner needs to decide it.
- Review rounds 2 to 4 reported 13 findings and fixed none; they are left for the owner.
- `ax-share` tests print `error: pathspec 'main' did not match` from git in temporary repositories. The tests pass; it is output noise only.
