# Evidence: MCP server, remote MCP, remote client and Admin (fix/review-mcp-remote)

This PR owns `ax-mcp`, `ax-remote`, `ax-remote-client` and `ax-admin` (Rust and `crates/ax-admin/web-ui`). Base commit: `9dca60e4`. The group has 69 findings (`mcp-remote-001` to `mcp-remote-069`): 52 are **fixed**, 1 **needs an owner decision** and 16 are **not done** (050, 056 and 069 partly done). "RED" means a test was seen failing before the fix and passing after it.

## Findings

| Finding | State | RED | Tests that cover it | Commit |
|---|---|---|---|---|
| mcp-remote-001 | fixed | yes | `tests/remote_mcp.rs::remote_policy_save_cannot_choose_where_the_rule_file_goes` | 54266762 |
| mcp-remote-002 | fixed | yes | `tests/guard_paths.rs::guard_reads_project_files_but_not_files_outside_the_project`, `guard_does_not_follow_a_symlink_out_of_the_project`, `guard_does_not_read_oversized_files`, `guard_does_not_block_on_a_fifo`, `diagnostics_does_not_read_files_outside_the_project` | 97c4e8b3 |
| mcp-remote-003 | fixed | yes | `ax-remote-client credentials::tests::a_sign_in_used_by_the_cli_is_not_revoked_when_ax_admin_lets_go`, `a_sign_in_stored_before_users_were_recorded_is_never_revoked_by_ax_admin`, `a_sign_in_only_ax_admin_uses_is_revoked_when_it_lets_go` | 3339cfbb |
| mcp-remote-004 | fixed | yes | `daemon::tests::a_failed_accept_does_not_end_the_accept_loop` | 91560557 |
| mcp-remote-005 | fixed | yes | `daemon::tests::a_tcp_session_without_the_token_is_closed_before_anything_runs`, `a_tcp_session_with_the_token_is_served`, `connect_any_sends_the_token_to_a_tcp_daemon`, `daemon_info_with_a_token_is_private_to_the_user` | 19e1da1e |
| mcp-remote-006 | fixed | yes | `daemon::tests::a_client_that_stops_reading_does_not_block_other_clients` | d51d1c85 |
| mcp-remote-007 | fixed | n/a: the finding asks for tests; covered by the new tests and the RED tests of 008, 011 and 012 | `daemon::tests::the_idle_timer_does_not_stop_a_daemon_with_a_client`, `the_idle_timer_stops_a_daemon_without_clients`, `a_connecting_client_disarms_the_idle_timer_and_the_last_disconnect_rearms_it`, `an_idle_timeout_of_zero_never_arms` | 333941a1 |
| mcp-remote-008 | fixed | yes | `daemon_lock::tests::a_lock_still_being_written_is_not_cleared_as_stale`, `concurrent_acquirers_get_exactly_one_lock`, `an_old_unreadable_lock_is_cleared`, `a_lock_of_a_dead_pid_is_cleared_and_a_live_one_is_kept`, `release_and_rewrite_only_touch_this_process_lock`; `daemon::tests::shutdown_leaves_the_files_of_another_daemon`, `shutdown_removes_its_own_files` | 9a4942ce |
| mcp-remote-009 | fixed | yes | `engine::tests::first_request_does_not_wait_for_the_catch_up_sync`, `a_policy_tool_first_also_starts_the_catch_up`, `the_catch_up_flag_clears_when_the_sync_finishes`, `the_real_catch_up_indexes_files_changed_while_no_server_ran`; review rounds 1 and 3 (shared catch-up, remote vault seeding) | ef687db3, 1113e5b0, 1a0e856c, 49d2ab5c |
| mcp-remote-010 | fixed | yes | `engine::tests::policy_tool_calls_keep_one_ax_handle`, `policy_tool_calls_see_rules_saved_by_another_process` | fa5707d4 |
| mcp-remote-011 | fixed | yes | `liveness_watchdog::tests::beat_age_never_underflows_when_the_clock_steps_back`, `beat_clock_measures_age_on_the_monotonic_clock`, `a_poll_that_overslept_counts_as_a_suspension` | c2c0f586 |
| mcp-remote-012 | fixed | yes | `liveness_watchdog::tests::heartbeat_stops_when_the_watched_runtime_makes_no_progress`, `heartbeat_without_a_runtime_beats_every_interval`, `the_runtime_ticker_stops_when_the_runtime_is_blocked` | 7e880b39 |
| mcp-remote-013 | fixed | yes | `proxy::tests::a_proxy_restart_is_skipped_when_another_proxy_already_restarted_on_this_build`, `a_restart_waits_while_another_restart_holds_the_restart_lock` | a7ed0132 |
| mcp-remote-014 | fixed | yes | `daemon_lock::tests::a_live_pid_that_started_after_the_lock_is_not_the_daemon`, `only_an_ax_binary_can_be_the_daemon`, `stop_leaves_a_process_that_is_not_the_daemon_alone`, `stop_asks_politely_first_and_then_force_kills`, `a_lock_whose_pid_was_recycled_is_cleared` | a7ed0132 |
| mcp-remote-015 | fixed | yes | `remote::auth::key_refresh::a_failed_refresh_keeps_verifying_with_the_cached_keys`, `a_slow_key_endpoint_does_not_hold_up_verification`, `a_rotated_key_is_fetched_on_first_use_and_unknown_kids_are_rate_limited`; review round 3: `tests/remote_limits.rs` unknown-kid wait | 8fb23a3b, e6754765 |
| mcp-remote-016 | fixed | yes | `remote::oauth::tests::a_full_pending_table_evicts_the_oldest_request_instead_of_refusing`, `one_registered_client_cannot_hold_more_than_its_share_of_pending_requests`, `a_full_client_table_makes_room_by_removing_an_unused_client`, `unauthenticated_oauth_endpoints_are_rate_limited`; review rounds 1 and 3: per-address limits next to a host-wide limit, `client_address_header` setting | d6e45535, 0738a11e, 070f5585 |
| mcp-remote-017 | fixed | yes | `tests/remote_client_db.rs::client_tokens_use_one_shared_pool_instead_of_opening_the_database_per_request` | 7692a854 |
| mcp-remote-018 | fixed | yes | `tests/remote_limits.rs::one_principal_cannot_use_up_every_session`, `one_principal_cannot_hold_every_request_slot`; review round 1: invalid-token request still gets 401 while every shared slot is taken | 7692a854, 8fbd1afd |
| mcp-remote-019 | fixed | yes | `tests/remote_mcp.rs::remote_chat_and_cache_ids_survive_a_reconnect_by_the_same_principal`, `remote_chats_and_caches_are_principal_owned`, `remote_context_reset_rehydrates_policy_and_fork_keeps_principal_ownership`, `remote_durable_branches_register_only_server_issued_child_ids`, `remote_tools_deny_execution_writes_root_overrides_and_foreign_chats` | f28bff68 |
| mcp-remote-020 | fixed | yes | `remote::server::bridge_tests::bridge_keeps_a_quiet_connection_open_until_the_idle_limit`, `bridge_closes_a_connection_that_stays_idle` | 32c9079d |
| mcp-remote-021 | fixed | yes | `transport::tests::read_request_reports_end_of_input`, `read_request_skips_blank_lines_and_then_reports_end_of_input` | 8c45fdaa |
| mcp-remote-022 | fixed | n/a: test-infrastructure defect; non-vacuity checked by mutation | `server::policy_integration::*` (now on an indexed fixture project), `test_env::tests::tests_never_see_the_real_home_or_usage_database` | 769ef87e |
| mcp-remote-023 | fixed | n/a: scheduling race, no deterministic RED | `test_env::tests::env_var_restores_the_previous_value`, `query_pool` parser test (module removed later by 048), `tests/preflight_standard_contract.rs`; full `ax-mcp` lib run | 769ef87e |
| mcp-remote-024 | fixed | yes | `tests/index_force.rs::failed_forced_rebuild_keeps_the_previous_graph`, `forced_rebuild_drops_files_that_are_gone`; review round 1: replicated rows survive a forced rebuild and `index_all` | 6f196a3d, c9f86b07 |
| mcp-remote-025 | fixed | yes | `tools::tests::base64_decode_honors_padding_and_rejects_bad_input` | 01ca4e47 |
| mcp-remote-026 | fixed | n/a: coverage tests; teeth shown by mutation | `ax-remote-client client::discovery_tests::*` (6 tests), `credentials::tests::connection_metadata_round_trips_privately_and_rejects_a_damaged_file` | f3ea2c90 |
| mcp-remote-027 | fixed | yes | `client::exchange_tests::a_rotated_refresh_token_is_kept_when_the_server_is_briefly_unavailable`, `a_revoked_grant_is_reported_but_the_rotated_refresh_token_is_still_stored`, `local_provider_exchange_checks_pkce_resource_project_access_and_rotating_refresh` | d28543ea |
| mcp-remote-028 | fixed | yes | `client::proxy_tests::per_call_http_errors_become_json_rpc_errors_and_the_proxy_keeps_running`, `a_rejected_token_is_refreshed_once_and_the_request_retried`, `an_expired_session_is_initialized_again_and_the_request_retried` | dcb3901e |
| mcp-remote-029 | fixed | yes | `ax-remote ship_config::tests::a_ship_toml_that_does_not_parse_is_reported_and_never_overwritten`, `a_valid_or_missing_ship_toml_loads_and_saves` | 303dd0c8, f16e95e3 |
| mcp-remote-030 | fixed | yes (cargo freshness: `ax-admin` stayed Fresh after touching `crates/ax-web/web-ui/src/index.css` before the fix, Dirty after) | build script, checked with `cargo build -p ax-admin -v`; `cargo test -p ax-admin` | 13aa9be2 |
| mcp-remote-031 | fixed | yes | `ax-admin tests/admin.rs::a_tool_the_server_marks_as_admin_gets_the_admin_timeout`, `the_remote_server_marks_exactly_its_management_tools_as_admin`, `b6_slow_and_oversized_answers_are_errors` (updated); `mcp::tests::admin_tools_get_ten_minutes_and_other_tools_do_not`; web-ui `remoteData.test.ts`, `pages.test.ts` | 93a8cb21 |
| mcp-remote-032 | fixed | yes | `tests/admin.rs::b2_two_calls_on_one_expired_session_reopen_it_once`, `b2_an_expired_session_is_reopened_once` | c9915baf |
| mcp-remote-033 | fixed | yes | `tests/admin.rs::an_edit_does_not_bring_back_a_connection_removed_meanwhile`, `store::tests::update_changes_an_existing_entry_and_never_adds_one` | 3665100b |
| mcp-remote-034 | fixed | yes | `store::tests::two_stores_on_one_file_keep_each_others_changes`, `a_damaged_file_is_reported_and_left_alone`, `configuration_file_is_private` | f8503c8c |
| mcp-remote-035 | fixed | n/a: fails only on a loaded machine or as root | `tests/admin.rs::a_slow_environment_does_not_hold_up_the_others` (rewritten), `a_failed_save_leaves_keys_and_connections_as_they_were`, `store::tests::a_failed_save_changes_nothing_in_memory`, `a10_*` sign-in tests | 6631e3c1 |
| mcp-remote-036 | fixed | yes | `context_budget::tests::blocks_that_cannot_fit_are_not_rendered_one_by_one`, `size_checks_select_the_same_blocks_as_rendering_each` | f2560ac4 |
| mcp-remote-037 | fixed | yes | `daemon::tests::an_accepted_client_counts_before_its_task_runs`, `a_shutdown_started_by_the_idle_timer_is_not_cancelled_by_itself` | fc7fab46 |
| mcp-remote-038 | fixed | yes | `daemon::tests::a_line_longer_than_the_limit_closes_the_connection`, `proxy_pump::tests::a_daemon_line_longer_than_the_limit_is_treated_as_a_lost_daemon` | 9ce44e8e |
| mcp-remote-039 | fixed | yes | `daemon::tests::a_failed_call_is_an_internal_error_and_an_unknown_method_is_not_found` | 0a493607 |
| mcp-remote-040 | fixed | yes | `ppid_watchdog::tests::a_failed_parent_lookup_is_not_a_lost_parent` | 76fc763e |
| mcp-remote-041 | fixed | yes | `proxy_pump::tests::a_new_daemon_receives_the_clients_initialize_before_other_lines` | 8072ab22 |
| mcp-remote-042 | fixed | yes | `remote::auth::key_refresh::a_few_seconds_of_clock_difference_with_the_provider_are_allowed`, `a_clock_skew_of_zero_checks_token_times_exactly`, `remote::config::tests::the_clock_skew_defaults_to_a_minute_and_has_an_upper_bound` | 0f8f5fa1, c8f94c62 |
| mcp-remote-043 | fixed | yes | `remote::config::tests::allowed_origins_are_stored_as_the_origin_a_browser_sends` | 3479806b |
| mcp-remote-044 | fixed | yes | `remote::oauth::tests::token_errors_use_rfc_codes_and_hide_database_details` | 0521b584 |
| mcp-remote-045 | fixed | yes | `tests/remote_startup.rs::an_invalid_host_control_credential_fails_before_any_lease_or_socket_exists` | 96c70218 |
| mcp-remote-046 | fixed | yes | `replication_credentials::tests::replace_overwrites_a_stored_value_that_does_not_parse` | e20def99 |
| mcp-remote-047 | fixed | yes | `server::error_codes::invalid_tool_arguments_answer_invalid_params`, `daemon::tests::a_failed_call_is_an_internal_error_and_an_unknown_method_is_not_found`, `tests/remote_mcp.rs` (remote logging path has no dedicated test) | fe57c4b2 |
| mcp-remote-048 | fixed (dead code removed) | n/a: no behavior change | full `ax-mcp` suite, `cargo clippy -p ax-mcp --all-targets -- -D warnings` | 010758fe |
| mcp-remote-049 | fixed | yes (trace part; the telemetry part has no meaningful test) | `server::policy_integration::remote_traces_never_carry_the_host_cursor_session` | f3427d47 |
| mcp-remote-050 | not done (partly) | n/a | the chat-id key list is one `CHAT_ID_KEYS` const, covered by `tests/remote_mcp.rs`. The large splits of `preflight()` and `post()` were not started. | c50c3f31 |
| mcp-remote-051 | fixed | yes | `tools::tests::schemas_accept_what_the_handlers_accept`, `server::wrap_tests::token_budget_advice_names_only_parameters_the_tool_accepts`, `tests/remote_admin.rs::e2_admin_tools_are_described_as_admin_scoped_without_project_arguments` | c50c3f31 |
| mcp-remote-052 | needs owner decision | n/a | none. A cheap check needs a persisted index generation row written by `ax index` and `ax sync` (ax-db and ax-extraction, another PR). | — |
| mcp-remote-053 | fixed | yes | `tools::tests::explore_options_are_capped_and_never_wrap`, `list_limits_default_and_cap` | c50c3f31 |
| mcp-remote-054 | fixed | yes | `tests/remote_mcp.rs::remote_tests_never_write_the_real_usage_database`; `tests/remote_admin.rs` and `tests/remote_mcp.rs` on the shared `tests/common/mod.rs` | 67e5718c |
| mcp-remote-055 | not done | n/a | `tests/remote_admin.rs` still waits with fixed polling limits | — |
| mcp-remote-056 | not done (partly) | n/a | `tests/common/mod.rs::isolate_process` now isolates `remote_mcp.rs` and `remote_admin.rs` (67e5718c); `tests/project_context_isolation.rs` still does not isolate the usage database | 67e5718c |
| mcp-remote-057 | not done | n/a | provider error redirect without `iss` still waits for the timeout | — |
| mcp-remote-058 | not done | n/a | finished sign-in attempts still count against the limit of 16 | — |
| mcp-remote-059 | not done | n/a | logout still needs the metadata entry to remove the keychain secret, and runs blocking calls on the executor | — |
| mcp-remote-060 | not done | n/a | each proxied request still reads the token store | — |
| mcp-remote-061 | not done | n/a | the proxy and access check still advertise `text/event-stream` while parsing JSON only | — |
| mcp-remote-062 | not done | n/a | Windows connection-file replace order unchanged | — |
| mcp-remote-063 | not done | n/a | Azure DevOps client timeout and PR link unchanged | — |
| mcp-remote-064 | not done | n/a | Azure DevOps reviewer request unchanged | — |
| mcp-remote-065 | not done | n/a | `RemoteConfig::default()` unchanged | — |
| mcp-remote-066 | not done | n/a | GitHub reviewer request status check unchanged | — |
| mcp-remote-067 | not done | n/a | GitHub `list_comments` pagination unchanged | — |
| mcp-remote-068 | not done | n/a | unused provider methods still present | — |
| mcp-remote-069 | not done (partly) | n/a | the save is now atomic and refuses a file that does not parse (mcp-remote-029, 303dd0c8); comments and key order are still lost on save | 303dd0c8 |

Notes:

- 050 (the remaining splits) and 055 to 069 were left because the disk was below the 15 GB limit when their batch ran (11 GB free), so no build could be started for them. Each still holds against the branch head.
- Edits outside this PR's crates: `crates/ax-core/src/lib.rs` (mcp-remote-024: drop files a complete forced rebuild did not write; review round 1: skip replicated rows), `crates/ax-cli/src/commands/daemon.rs` (mcp-remote-014: `ax daemon stop` uses the daemon identity check), `crates/ax-web/src/remote_host.rs` (new remote config field), `Cargo.lock` (ax-remote `tempfile` dev-dependency), `docs/CODEGRAPH_AX_COMPARE_MATRIX.md`, and site pages `getting-started/configuration.md`, `guides/indexing.md`, `guides/remote-admin.md`, `guides/remote-mcp.md`, `reference/cli.md`, `reference/mcp-server.md`.

## Review rounds

| Round | Findings | Fixed | Commits |
|---|---|---|---|
| 1 | 4 | 4 | c9f86b07, 8fbd1afd, 1113e5b0, 1a0e856c (docs), 0738a11e |
| 2 | 3 | 0 | — (carried into round 3) |
| 3 | 3 | 3 | e6754765, 49d2ab5c, 070f5585 |
| 4 | 0 | — | — |

## Commands run for this evidence (one run, on the branch head before this file)

| Command | Result |
|---|---|
| `cargo fmt --all --check` | first run: 2 files differed (test blocks added in this branch); `cargo fmt --all` applied in 2c4c5b8d; then clean |
| `cargo test -p ax-mcp --no-fail-fast` | 348 passed, 1 failed (lib 263; integration: catalog_payload_size 1, context_budget_contract 3, guard_paths 5, ice_preflight 3 + 1 failed, index_force 4, neural_preflight 7, new_tools_smoke 2, preflight_standard_contract 5, project_context_isolation 16, remote_admin 16, remote_client_db 1, remote_limits 4, remote_mcp 17, remote_startup 1) |
| `cargo test -p ax-mcp --test ice_preflight` after c0e5394f | pass: 4 passed, 0 failed (ax-mcp total for this run: 349 passed, 0 failed) |
| `cargo test -p ax-remote` | pass: 10 passed, 0 failed |
| `cargo test -p ax-remote-client` | pass: 23 passed, 0 failed |
| `cargo test -p ax-admin` | pass: 43 passed, 0 failed (lib 10, tests/admin.rs 33) |
| `cargo test -p ax-core` (caller changed for mcp-remote-024) | pass: 49 passed, 0 failed (lib 39, build_writers 1, policy_dedup_hooks 1, sync 8) |
| `cargo clippy -p ax-mcp -p ax-remote -p ax-remote-client -p ax-admin --all-targets -- -D warnings` (also run one crate at a time) | clean |
| `cargo clippy -p ax-cli -p ax-core -p ax-web --all-targets -- -D warnings` (callers changed by this PR) | clean |
| `npm test` in `crates/ax-admin/web-ui` | pass: 36 passed, 0 failed |
| `npm run typecheck` in `crates/ax-admin/web-ui` | pass |
| `npm run build` in `crates/ax-admin/web-ui` | pass |
| `npm test` in `site/` (docs changed) | pass: 89 passed, 0 failed |
| `npm run build` in `site/` | pass |

The one failure, `ice_preflight::preflight_returns_exact_indexed_manifest_line`, was not caused by this PR: the test file was unchanged and it passed with `AX_HOME_DIR` pointed at a temporary directory. It read the developer's `~/.ax/config.json` (`context.budgetTokens: 12000`) and global policy, so the required policy filled the budget and the source line was dropped. c0e5394f isolates the ax home in that test file with the shared `common::isolate_process` fixture; it then passes with the real home present.

## Open items

- mcp-remote-052 needs an owner decision: where a persisted index generation row lives (ax-db / ax-extraction) and which PR owns it.
- mcp-remote-050 (split `preflight()` and `post()`, move schemas next to handlers) and mcp-remote-055 to 069 are not done; 056 and 069 are partly done (see the table).
- mcp-remote-001: the ax-policy side (`resolve_source_path` accepting absolute paths outside the project or policy roots) belongs to the policy PR.
- mcp-remote-003: keeping a sign-in attempt pending for a retry after a transient probe failure, and a "shared sign-in" note in the Admin remove dialog, were not done.
- mcp-remote-006: passing the connection id into `handle_request` instead of the ambient `set_active` state is longer-term work.
- mcp-remote-007: `restart_daemon` end to end, `daemon_conn.rs` and the proxy spawn path have no tests.
- mcp-remote-005: the `$XDG_RUNTIME_DIR` socket path on Linux has no dedicated test.
- mcp-remote-047: the remote failure logging has no automated test (no tracing subscriber in the dev-dependencies).
- `ax-remote` has a pre-existing `unused dependency ax-git` manifest warning (not a clippy error; the PR did not touch that dependency).
- The full test suites of `ax-cli` and `ax-web` were not run; their changes here are small caller edits that compile and pass clippy.
