# Evidence: fix/review-web-server (ax-web)

Base commit: `9dca60e4`. Scope: `crates/ax-web` (Rust and web UI), plus the minimal edits outside it listed under "Edits outside ax-web".

RED means a test failed on the code before the fix. Where no RED was possible, the row says why and names the check that covers it.

## Findings

| Finding | State | RED seen | Test(s) | Commit |
|---|---|---|---|---|
| web-server-001 | Fixed | yes | `tests/lan_exposure.rs::an_anonymous_lan_bind_refuses_to_start`, `tests/local_owner_peer.rs::the_terminal_socket_needs_a_loopback_peer_even_on_a_lan_bind`, `request_guard::tests::a_lan_bind_needs_sign_in_a_share_token_or_the_explicit_flag`, `browser_origin::tests::terminal_needs_the_local_peer_mark_and_a_loopback_origin` | 725d6b7d, d5da6df9 |
| web-server-002 | Fixed | yes | `tests/spa_dist.rs::dot_dot_segments_cannot_leave_the_dist_folder` | 8eef76fb |
| web-server-003 | Fixed | yes | `tests/host_guard.rs::a_foreign_host_name_is_refused_everywhere`, `tests/host_guard.rs::configured_hosts_are_accepted`, `request_guard::tests::*` (3) | f9a30468 |
| web-server-004 | Fixed | yes | `tests/local_owner_peer.rs::only_a_loopback_peer_without_a_proxy_is_the_local_owner`, `request_guard::tests::only_an_unproxied_loopback_peer_is_local`, `local_owner::tests::owner_surface_rejects_bearer_share_and_foreign_browser_authority`, `web_auth::tests::reverse_proxy_browser_needs_a_session_to_set_a_password`, `dav::mount::tests::n1_loopback_only` | 725d6b7d |
| web-server-005 | Fixed | yes | `tests/microsoft_auth_guard.rs::a_share_session_cannot_change_microsoft_sign_in`, `::only_the_local_owner_changes_microsoft_sign_in` | b1a4ddbc |
| web-server-006 | Fixed | yes | `tests/skill_catalog_guard.rs::read_only_mode_refuses_every_catalog_write`, `::the_publish_repository_must_be_a_remote_url_or_inside_the_project` | a5eee2d7 |
| web-server-007 | Fixed | yes | `tests/sonar_proxy_guard.rs::read_only_mode_refuses_proxied_sonar_writes`, `::cross_site_or_origin_less_sonar_writes_are_refused` | f646c51a |
| web-server-008 | Fixed (code in 725d6b7d) | yes (old files restored temporarily) | `tests/local_owner_peer.rs::a_same_host_proxy_visitor_cannot_take_over_the_owner` | e8c4ca0e |
| web-server-009 | Fixed | yes | `tests/access_client_escalation.rs::an_admin_cannot_mint_owner_credentials`, `::an_admin_still_manages_its_own_and_lower_credentials` | c2973e1c |
| web-server-010 | Fixed | yes | `tests/agent_readonly.rs::read_only_mode_refuses_profile_delete_and_chat`, `::chat_needs_the_local_browser`, `tests/agent_install_guard.rs` | fc9ad345 |
| web-server-011 | Fixed | yes | `agent::tests::a_chatty_auth_cli_does_not_block_on_a_full_pipe`, `::a_stuck_auth_cli_is_stopped_at_the_timeout`, `::an_auth_cli_is_stopped_when_the_client_goes_away`, `::an_external_agent_run_is_killed_when_it_is_dropped`, `::an_external_agent_run_is_killed_at_the_timeout` | 209c1660 |
| web-server-012 | Fixed | yes | `agent_pty::tests::closing_the_terminal_kills_the_child` | 1b32ad8d |
| web-server-013 | Fixed | yes | `tests/vault_folders.rs::d8_stat_reads_metadata_not_the_file`, `::d9_large_folder_files_stream_in_full_and_by_range`, `::d10_overwriting_a_large_folder_file_with_put_works` | 970b73fc |
| web-server-014 | Fixed (performance only) | no: output does not change, only cost | `tests/dav_vault.rs::listings_match_pages_and_duplicate_memory_titles_resolve` (pin test), `::b02_rules_folder_lists_rules_with_real_size` | 8eae5012 |
| web-server-015 | Fixed | yes | `tests/dav_vault.rs::n7_move_folder_into_itself_is_refused_and_keeps_files`, `::n8_move_folder_replaces_destination_and_moves_every_row` | f8387b98 |
| web-server-016 | Fixed | yes | `tests/vault_folders.rs::d8_stat_reads_metadata_not_the_file`, `tests/dav_vault.rs::b01_root_lists_areas_and_drafts` | 970b73fc |
| web-server-017 | Fixed | yes | `dav::mount_tests::n1_origin_must_be_command_center`, `tests/dav_mount.rs::n1_page_on_another_local_port_is_403` | 82c48c41 |
| web-server-018 | Fixed | yes | `dav::mount_tests::n2_mount_dir_must_not_hide_files`, `tests/dav_mount.rs::n2_mount_never_covers_a_folder_with_files` | eade4c31 |
| web-server-019 | Fixed | yes | `tests/graph_api_bounds.rs::negative_limits_and_offsets_stay_bounded`, `::source_line_math_does_not_overflow` | 8d80b18f |
| web-server-020 | Fixed | yes | `tests/graph_api_bounds.rs::source_serves_only_indexed_files`, `::source_refuses_files_over_the_size_cap` | 8d80b18f |
| web-server-021 | Fixed | yes | `port_tests::a_foreign_listener_is_not_killed`, `port_tests::only_ax_commands_count_as_ax` | 161a4aea |
| web-server-022 | Fixed | yes | `tests/mcp_ops_guard.rs::read_only_mode_refuses_reload_and_audit`, `::another_website_cannot_reload_the_daemon`, `::audit_refuses_arbitrary_file_paths` | db80c75d |
| web-server-023 | Fixed | yes | `mcp_quality::tests::audit_is_cached_without_verbose_logs`, `::concurrent_refreshes_run_one_audit`, `::watcher_starts_on_first_request` | f7d0e5a3 |
| web-server-024 | Fixed | yes | `tests/mcp_trace_scope.rs::read_only_viewer_never_follows_another_project`, `::local_owner_still_follows_the_active_project` | a007a7c4 |
| web-server-025 | Fixed | yes | `tests/blocking_handlers.rs::memory_git_routes_run_off_the_runtime`, `::wiki_publish_and_docs_sync_run_off_the_runtime` | 017018cd |
| web-server-026 | Fixed | yes | `tests/policy_routes.rs::skill_routes_act_on_the_skill_when_a_rule_shares_the_name` | eb94a157 |
| web-server-027 | Fixed (cost and lock contention) | no: no wrong output to observe over HTTP | `global_pool::tests::opens_the_database_once_per_path` | 63ea0c63 |
| web-server-028 | Fixed | yes | `tests/policy_routes.rs::relocate_to_global_rolls_back_when_the_local_delete_fails` | 3b453ab7 |
| web-server-029 | Fixed | yes | `tests/policy_routes.rs::export_refuses_folders_outside_the_project` | f607ac6e |
| web-server-030 | Fixed | yes | `queries::fts_tests::search_treats_fts_syntax_as_plain_text`, `::negative_limits_do_not_mean_unlimited`, `::count_errors_are_returned` (guard) | 89a202ac |
| web-server-031 | Fixed | yes | `remote_host::tests::stop_treats_a_foreign_service_on_the_port_as_not_our_host`, `::stop_without_credentials_still_lets_removal_continue`, `::start_moves_to_a_free_port_when_another_service_holds_the_saved_one`, `::start_keeps_a_free_saved_port` | c7df9dcd |
| web-server-032 | Fixed | yes | `replication_setup::tests::a_running_setup_does_not_hold_the_node_state_and_rejects_a_second_start` | 60098855 |
| web-server-033 | Fixed | yes | `tests/ship_guard.rs::read_only_viewers_cannot_create_a_draft_pull_request`, `::read_only_viewers_cannot_run_the_sonar_login_check`, `::sonar_login_check_only_reaches_the_configured_or_a_loopback_host` (the one-line discover condition needs a live SonarQube and has no test) | 3ede797f |
| web-server-034 | Fixed | yes | `ship::tests::a_long_sonar_operation_keeps_settings_changed_while_it_ran`, `::a_failed_save_after_a_sonar_operation_is_reported` | 9aade723 |
| web-server-035 | Fixed | yes | `tests/ship_guard.rs::body_less_sonar_actions_refuse_other_sites_and_origin_less_requests`, `tests/sonar_proxy_guard.rs` (2) | f646c51a, fe476776 |
| web-server-036 | Fixed | yes | `sonar_proxy::tests::rewriting_a_large_bundle_is_linear_in_its_size`, `::rewriting_keeps_non_ascii_text_intact` | b54883a3 |
| web-server-037 | Fixed | yes | `tests/specs_routes.rs::read_only_list_does_not_import_and_still_lists`, `::read_only_viewers_can_read_the_hierarchy_tree`, `::list_rescans_plan_folders_only_after_the_interval_or_on_refresh` | dd115f9f, c998cea5 |
| web-server-038 | Fixed | yes | `tests/web_auth_gate.rs::a_parallel_burst_gets_no_more_than_five_guesses`, `web_auth::tests::a_limited_key_counts_nothing_and_success_forgives` | 05ed7709 |
| web-server-039 | Fixed | yes | `tests/web_auth_gate.rs::password_hashing_does_not_stall_other_requests` | 05ed7709 |
| web-server-040 | Fixed | yes | `tests/web_auth_gate.rs::one_address_cannot_spread_guesses_over_many_identities`, `web_auth::tests::attempts_older_than_the_window_are_dropped`, `::a_full_table_refuses_new_keys_but_not_known_ones`, `::failures_through_a_local_proxy_do_not_block_other_identities` | 05ed7709, 6ea8fc96 |
| web-server-041 | Fixed | yes | `tests/web_auth_gate.rs::static_assets_load_while_the_global_database_cannot_open` | 63ea0c63, 05ed7709 |
| web-server-042 | Fixed | yes | `tests/web_auth_gate.rs::scoped_client_tokens_keep_working_with_web_login_on` | 05ed7709 |
| web-server-043 | Fixed | yes | `tests/web_auth_http.rs::a_same_site_page_cannot_change_data_with_a_valid_session`, `::the_configured_origin_is_the_only_trusted_one` | 1151b42a |
| web-server-044 | Fixed (minimum fix, not a full allow-list) | yes | `tests/web_auth_http.rs::roles_decide_what_a_signed_in_member_may_reach` | 1151b42a |
| web-server-045 | Fixed (coverage) | yes (via 043/044 tests in the same file) | `tests/web_auth_http.rs` (7 tests) | 1151b42a |
| web-server-046 | Fixed (dead check removed) | no: the removed check never refused anything | `workspace::tests::only_absolute_paths_are_accepted`, `tests/workspace_routes.rs::only_the_local_browser_uses_workspace_routes_without_sign_in` | 500fde12 |
| web-server-047 | Fixed | yes | `tests/workspace_routes.rs::only_the_local_browser_uses_workspace_routes_without_sign_in`, `::read_only_mode_does_not_change_the_recent_list` | e9f760c7 |
| web-server-048 | Fixed | yes | `tests/workspace_switch.rs::a_cancelled_switch_does_not_block_later_switches`, `tests/hub_smoke.rs::hub_open_and_hot_switch` | e046aad7 |
| web-server-049 | Fixed | yes | `tests/workspace_purge_current.rs::purging_the_open_project_closes_it_and_says_so`, `workspace_state::closed_project_tests::project_routes_need_a_project_and_workspace_routes_do_not` | 1a9268a7, 74de5768 |
| web-server-050 | Fixed | yes | `tests/access_authorize.rs` (4 tests), `access_api::scope_tests::the_route_decides_the_scope_not_an_id_inside_it` | 9640b324 |
| web-server-051 | Fixed | yes | `access_api::pool_tests::an_existing_owner_is_checked_without_the_write_lock` | 63ea0c63, 77402b80 |
| web-server-052 | Fixed | yes | `utf8_stream::tests::*` (3), `mcp_trace::tests::a_character_half_written_to_the_log_is_read_once_complete` | f9a833c7 |
| web-server-053 | Fixed | yes | `agent_pty::reader_tests::output_waits_for_a_slow_browser_instead_of_piling_up`, `agent_pty::tests::closing_the_terminal_kills_the_child` | 1b32ad8d, f9a833c7 |
| web-server-054 | Fixed | yes | `shutdown_tests::an_open_event_stream_does_not_keep_the_server_running`, `shutdown_tests::sigterm_starts_the_shutdown` | d85e194a |
| web-server-055 | Fixed | yes | `mcp_trace::tests::the_seed_stops_before_a_line_still_being_written`, `::the_seed_of_a_large_log_is_only_its_last_whole_lines`, `::complete_lines_are_read_from_an_offset_and_a_half_line_waits` | 80c428bd |
| web-server-056 | Fixed | yes (race part: guard only) | `node_tracker::tests::an_unchanged_graph_is_not_loaded_again`, `::a_closed_workspace_drops_its_tracker`, `::a_node_that_replaces_the_newest_row_is_still_seen`, `::concurrent_snapshots_record_a_new_node_once` | e2c4f069 |
| web-server-057 | Partly fixed; refactor not done | yes | `tests/policy_routes.rs::a_captured_rule_reports_the_file_it_was_saved_to`, `::invalid_policy_settings_change_nothing`, `::a_policy_package_is_built_and_previewed` | bf87a05d |
| web-server-058 | Fixed | yes | `tests/microsoft_auth_guard.rs::saving_the_client_id_leaves_the_process_environment_alone`, `::saving_a_client_id_is_refused_while_the_environment_sets_one` | 121da1a1, 9313e062 |
| web-server-059 | Fixed (coverage; found two real read-only gaps) | yes | `tests/readonly_writes.rs::a_read_only_server_refuses_every_write`, `queries::graph_edge_tests::replicated_nodes_stay_in_their_own_graph`, `::node_filters_and_limits_apply` | 57edac98 |
| web-server-060 | Fixed | yes | `queries::graph_edge_tests::any_number_of_ids_can_be_asked_for`, `::the_graph_keeps_edges_between_its_nodes_only`, `::recent_nodes_bring_every_non_contains_edge_that_touches_them` | 5c7efffd |
| web-server-061 | Fixed | yes | `tests/settings_without_projects.rs::node_status_answers_when_the_saved_node_lists_no_project`, `::host_authorizations_answer_when_the_host_lists_no_project` | 6baa69da |
| web-server-062 | Fixed | yes | `tests/usage_tokenize.rs::tokenize_answers_normal_text_and_refuses_an_oversized_body`, `tests/readonly_writes.rs::a_read_only_server_refuses_every_write` | ff0e5f6a |
| web-server-063 | Fixed | yes | `tests/share_auth_failures.rs::node_credentials_get_a_clear_answer_and_client_failures_are_audited`, `share_auth::tests::token_comparison_matches_only_the_exact_token` | 90966507 |
| web-server-064 | Fixed | yes | `tests/share_auth_failures.rs::a_peer_that_keeps_failing_the_share_token_is_refused_for_a_while`, `share_auth::tests::a_locked_peer_cannot_tell_a_right_guess_from_a_wrong_one`, `share_auth::tests::failures_through_a_local_proxy_lock_nobody` | 90966507, 6ea8fc96, 2b329007 |
| web-server-065 | Fixed | yes | `ship::tests::a_panicking_evaluation_clears_the_evaluating_flag` | f63ebd64 |
| web-server-066 | Fixed | yes | `sonar_proxy::tests::a_host_fallback_after_invalidate_does_not_mark_the_cache_ready` | a6119d38 |

Final-check commits in this run: `5eea0cb3` (the Specs page source test now expects `/api/specs?refresh=1`, which c998cea5 introduced; embedded `dist` rebuilt from the current sources) and `ecc58c16` (rustfmt on `share_auth.rs`).

## Review rounds

| Round | Findings | Fixed | Commits |
|---|---|---|---|
| 1 | 5 | 5 | 6ea8fc96, 9313e062, c998cea5 |
| 2 | 1 | 1 | 2b329007 |
| 3 | 0 | 0 | none |

## Commands run (this run)

| Command | Result |
|---|---|
| `npm test` (crates/ax-web/web-ui) | first run 1 failure (`specsPage.test.ts`, broken by c998cea5); after 5eea0cb3: 91 passed, 0 failed |
| `npm run build` (crates/ax-web/web-ui) | pass (chunk-size warning only) |
| `cargo test -p ax-web` | 43 test binaries, 350 passed, 0 failed, 1 ignored (`seed_walkthrough_manifests`, a manual seed that needs `AX_WALKTHROUGH_ROOT`) |
| `cargo clippy -p ax-web --all-targets -- -D warnings` | pass, no warnings |
| `cargo fmt --all --check` | first run: differences in `crates/ax-web/src/share_auth.rs`; after ecc58c16: clean |

## Edits outside ax-web

- `crates/ax-policy/src/store.rs`: typed `set_rule_enabled` / `set_skill_enabled` / `set_rule_storage` / `set_skill_storage` (web-server-026).
- `crates/ax-usage/src/spec_hierarchy.rs`: schema setup skips writes when everything exists, so read-only pools can read (web-server-037).
- `crates/ax-cli/src/main.rs`, `crates/ax-cli/src/commands/share.rs`, `crates/ax-desktop-client/src/main.rs`, `crates/ax-desktop-client/src/server.rs`: `--allow-anonymous-lan` and the new `ServeOptions` field (web-server-001).
- `Cargo.lock`: dependency entries for ax-web.
- Site docs under `site/src/content/docs/` (web-api.md, cli.md and guides) for every behavior change.

These crates were not built or tested in this run; the task limits checks to the owned crate.

## Open items

- web-server-057: the maintainability refactor (split `policy.rs` into modules, generic rule and skill handlers over a policy kind, shared payload parsing) is not done. It should be a pure move kept green by `tests/policy_routes.rs` and the policy unit tests.
- web-server-006: `ax-skill-catalog`'s `git_clone` does not put `--` before the URL. The web route validates the URL first (a leading `-` is refused), so this is defence in depth in another crate.
- web-server-025: git calls run off the async workers but have no timeout of their own.
- web-server-044: the gate uses an admin-path list, not a full allow-list.
- web-server-040: behind a reverse proxy on the same machine, all clients share the loopback peer address; the per-identity limit still applies.
- Crates outside ax-web that this PR touched (ax-cli, ax-desktop-client, ax-policy, ax-usage) were not compiled or tested in this run.
