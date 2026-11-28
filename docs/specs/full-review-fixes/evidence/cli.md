# Evidence: fix/review-cli (ax-cli)

Base commit: `9dca60e4`. This PR owns the `ax-cli` crate. Each state follows the spec's "What fixed means" section. "RED seen" means a test failed on the old code before the fix.

## Findings

| Id | State | Covering test(s) | Commit |
|---|---|---|---|
| cli-001 | Fixed (RED seen) | `commands::cursor::tests::show_json_never_contains_tokens` | 53e9bc83 |
| cli-002 | Fixed (RED seen) | `commands::daemon::tests::{stale_pid_of_unrelated_process_is_not_killed, stale_pid_with_other_exe_identity_is_not_killed, identified_daemon_is_terminated_and_waited_for, daemon_ignoring_sigterm_is_killed_after_grace, dead_pid_is_reported_as_no_daemon}` | 503b0e61, 8e834bed, af83b5b2 |
| cli-003 | Fixed (RED seen) | `commands::db::tests::{a_failed_follow_up_step_fails_the_repair_even_when_the_recheck_is_clean, a_failed_follow_up_step_is_kept_next_to_the_recheck_findings, a_clean_recheck_without_follow_up_errors_is_ok}` | 9f83ffce |
| cli-004 | Fixed (RED seen) | `tests/offload_consent.rs::explore_json_does_not_call_the_offload_endpoint` | 51b976d6 |
| cli-005 | Fixed (RED seen) | `tests/offload_consent.rs::{init_does_not_switch_on_offload_from_an_api_key_in_the_environment, init_keeps_an_offload_endpoint_the_user_configured}` | 51b976d6 |
| cli-006 | Fixed (RED seen) | `tests/project_root_subdir.rs` (5 tests: query commands and prompt hook from a subdirectory, init in a subdirectory, no project above, `~/.ax` never a project) | cb8cc0ec |
| cli-007 | Fixed (RED seen) | `commands::policy_pull::tests::{clone_args_end_options_before_the_url, clone_args_reject_a_url_that_looks_like_an_option, a_symlinked_rule_is_not_followed, symlinks_in_skills_are_not_followed, a_rules_folder_that_links_out_of_the_clone_is_refused, a_tree_over_the_file_cap_is_refused, a_tree_over_the_byte_cap_is_refused}`, `commands::skill_catalog::tests::git_clone_refuses_a_url_that_looks_like_an_option` | 61920286 |
| cli-008 | Fixed (RED seen) | `commands::policy_pull::tests::{failed_clone_keeps_the_previous_vendored_copy, successful_clone_replaces_the_vendored_copy, a_changed_local_rule_is_kept_without_force, force_replaces_a_changed_local_rule, an_identical_local_rule_counts_as_unchanged, a_changed_local_skill_is_kept_without_force, force_replaces_a_changed_local_skill_whole, new_rules_and_skills_are_copied}` | 61920286 |
| cli-009 | Fixed (RED seen) | `commands::policy::tests::{dedup_note_does_not_claim_a_dry_run_cleaned_anything, dedup_note_reports_a_real_run_as_cleaned, dedup_note_is_silent_when_nothing_was_found, dedup_note_points_a_stopped_check_at_the_real_command}` | cf5c40cc |
| cli-010 | Fixed (RED seen) | `tests/policy_cli.rs::{policy_test_passes_on_a_fresh_project_without_ides, policy_test_passes_on_a_fresh_cursor_project, policy_test_fails_when_the_startup_skill_is_missing, hidden_self_flag_adds_the_ax_repository_checks}` | ed339eff |
| cli-011 | Fixed (RED seen) | `tests/policy_cli.rs::{storage_switch_keeps_the_config_when_the_migration_fails, storage_switch_json_reports_what_was_migrated}` | f132b587 |
| cli-012 | Fixed (RED seen) | `commands::prompt_hook::tests::{cap_inject_cuts_on_a_char_boundary, cap_inject_keeps_short_text}` | af62baaf |
| cli-013 | Fixed (RED seen) | `commands::read_guard::tests::concurrent_claims_keep_every_entry` | edd0281f |
| cli-014 | Fixed (RED seen) | `commands::savings::hook_install_tests::hook_script_is_executable` | 4249e03e |
| cli-015 | Fixed (RED seen) | `commands::savings::hook_install_tests::{unparsable_hooks_json_is_left_untouched, non_object_hooks_json_is_refused_without_panic, foreign_hooks_survive_and_install_is_idempotent, symlinked_hooks_json_keeps_its_link}` | 4249e03e, 71f2e400 |
| cli-016 | Fixed (RED seen) | `tests/spec_approve.rs::non_interactive_spec_approve_is_refused`, `commands::spec::tests::{non_terminal_stdin_is_refused, confirmation_needs_the_exact_word}` | c770f367 |
| cli-017 | Fixed (RED seen) | `commands::stop_hook::tests::subdirectory_project_scans_its_changed_files`, `commands::turn_hook::tests::dirty_files_hashes_content_when_project_is_a_repo_subdirectory` | 904a5b89 |
| cli-018 | Fixed (RED seen) | `commands::stop_hook::tests::{new_file_in_new_directory_is_scanned, non_ascii_and_spaced_names_are_scanned, deleted_files_are_skipped_and_renames_scan_the_new_path, outside_a_git_work_tree_there_is_nothing_to_scan}`, `commands::git_status::tests::{parses_modified_untracked_and_renamed, ignores_short_and_empty_entries, parses_a_worktree_side_rename}` | 904a5b89, e8ebf087 |
| cli-019 | Fixed (RED seen for the missing-tools case only; the kill-everything case was not run on the old code because it would kill real ax processes) | `tests/unlock.rs` (4 tests), `commands::ax_process::tests::{own_process_is_not_listed, non_ax_process_is_not_ax}` | 6554d24e |
| cli-020 | Fixed (RED seen) | `commands::upgrade::tests::{download_refuses_an_archive_whose_digest_does_not_match_sha256sums, download_refuses_when_sha256sums_is_missing_or_lacks_the_entry, local_archive_is_checked_against_sha256sums_next_to_it, download_accepts_an_archive_that_matches_sha256sums, sha256sums_parsing_accepts_text_and_binary_markers}` | fbccf2f9 |
| cli-021 | Fixed (RED seen) | same digest tests as cli-020 | fbccf2f9 |
| cli-022 | Fixed (RED seen: code 517) | `server_exchange::tests::concurrent_exchanges_queue_instead_of_failing_busy` | 1963a69f |
| cli-023 | Fixed (RED seen) | `server_exchange::tests::{history_of_a_changing_node_is_compacted_without_losing_records, history_above_the_retention_cap_recovers_by_compaction, returning_to_an_earlier_revision_preserves_history_and_publishes_current_value (updated), oversized_union_is_rejected_without_pruning_accepted_data}` | f6db90eb |
| cli-024 | **Needs owner decision** | none; no code changed | none |
| cli-025 | Fixed (RED seen) | `server_http::tests::bootstrap_requests_register_a_replica_grant_only_once` | cad5c32a |
| cli-026 | Fixed (RED seen) | `server_http::tests::{public_ips_are_not_private_bind_targets, wildcard_bind_is_allowed_only_when_every_interface_is_private}` | 58cb77e0, bdef9a26 |
| cli-027 | Fixed (RED seen for the listener decision; the Linux EADDRINUSE cannot be reproduced on macOS) | `server_http::tests::{managed_control_gets_a_second_listener_only_for_a_specific_non_loopback_address, managed_control_on_a_shared_listener_answers_loopback_peers_only, managed_control_routes_read_the_peer_address_over_tcp, managed_control_requires_its_own_secret_and_is_disabled_for_cli_servers}` | 58cb77e0, bdef9a26 |
| cli-028 | Fixed (RED seen) | `server_push::tests::{primary_builds_a_snapshot_only_for_live_replicas_after_a_source_change, fingerprint_follows_project_files_and_the_usage_database}` | a42ebf0c, c1680e2a |
| cli-029 | Fixed (RED seen) | `server_replica::tests::{concurrent_stages_of_one_revision_all_succeed, replica_synchronizations_in_one_process_run_one_at_a_time}` | 39a77216 |
| cli-030 | Fixed (RED seen) | `server_replica::tests::staging_a_new_revision_removes_older_revisions_and_staging_leftovers` | 39a77216 |
| cli-031 | **Not done** (finding holds: `server_snapshot.rs` `export_native` checks `MAX_SNAPSHOT_BYTES` only after building the whole snapshot) | none | none |
| cli-032 | **Not done** (finding holds: `ui/terminal.rs` color detection) | none | none |
| cli-033 | **Not done** (finding holds: `version_check.rs` failed checks are not cached; no overall deadline) | none | none |
| cli-034 | **Not done** (finding holds: `memory_parity.rs`, `cli_parity_turn.rs`, `cli_parity_gaps.rs` do not set `AX_HOME_DIR` / `AX_NO_UPDATE_CHECK`) | none | none |
| cli-035 | **Not done** (finding holds: `costs.rs` dangling " to" label; unknown `--period` / `--strategy` fall back silently) | none | none |
| cli-036 | **Not done** (finding holds: a `costs.rs` test sets env vars across awaits; cli-055 put that test behind `scoped_env`, but the finding was not re-checked or closed) | none | none |
| cli-037 | **Not done** (finding holds: stdin `action` key overrides the typed subcommand in `durable_cmd.rs` / `session_cmd.rs`) | none | none |
| cli-038 | **Not done** (finding holds: `export.rs` undirected Cypher MERGE, unused `god_cutoff`, lossy `plantuml_id`) | none | none |
| cli-039 | **Not done** (finding holds: `init.rs` swallows seeding errors with `.ok()`) | none | none |
| cli-040 | **Not done** (finding holds: `--json` gate paths in `policy.rs` / `mcp.rs` exit 0 on failure) | none | none |
| cli-041 | **Not done** (finding holds: byte slicing in `pricing.rs` `truncate()` and `savings.rs`) | none | none |
| cli-042 | **Not done** (partly holds: ignored `_kind`, `_format`, `_verbose` parameters; the `policy.rs` part no longer applies) | none | none |
| cli-043 | Fixed (RED seen) | `commands::skill_catalog::tests::{http_bytes_rejects_a_declared_oversize_body_without_reading_it, http_bytes_stops_reading_an_endless_body_at_the_limit, well_known_file_urls_stay_on_the_index_origin, well_known_file_fetch_refuses_a_redirect_to_another_origin, well_known_file_fetch_follows_a_same_origin_redirect}` | e53cab30, 0754b4a6, 24bf4e14 |
| cli-044 | Fixed (RED seen: did not compile against the old bool API) | `commands::telemetry::tests::a_prompt_that_cannot_be_shown_records_no_choice` | 917b8204 |
| cli-045 | Fixed (RED seen) | `tests/test_impact_errors.rs::an_unknown_base_ref_is_an_error` | dbe6219d |
| cli-046 | Fixed (RED seen) | `commands::turn_hook::tests::{a_later_session_starting_does_not_lose_the_first_sessions_turn_end, without_a_snapshot_the_turn_end_uses_current_json_for_this_session, ending_the_named_turn_stores_the_redacted_outcome_and_the_file (updated)}` | 9368cc08 |
| cli-047 | Fixed (RED seen) | `commands::turn_hook::tests::{ending_a_turn_scans_the_working_tree_once, dirty_files_fingerprints_large_files_without_hashing_them}` | 9368cc08 |
| cli-048 | Fixed (RED seen) | `tests/uninit_confirm.rs::{without_yes_and_without_a_terminal_nothing_is_deleted, with_yes_the_ax_directory_is_removed, a_hook_that_cannot_be_cleaned_keeps_the_ax_directory}`, `commands::uninit::tests::describe_names_shared_policy_and_memory` | 894b8bb5 |
| cli-049 | Fixed (RED seen) | `commands::validate::tests::{counts_report_every_isolated_symbol_while_lists_stay_capped, orphan_docs_are_found_through_documents_and_references_edges}` | 38a365d8, e698e4c0 |
| cli-050 | Fixed (RED seen) | `commands::upgrade::tests::ide_files_refresh_once_per_ax_version`, `refresh_ide_files_tests::{hooks_helpers_and_removal_commands_skip_the_refresh, regular_commands_refresh}` | 52e6601d |
| cli-051 | Fixed (refactor, no RED possible) | `refresh_ide_files_tests::hidden_helpers_print_no_update_notice`; `--help` text of every session/durable/turn/cache subcommand byte-identical before and after; full ax-cli suite | e60b67bc |
| cli-052 | Fixed (RED seen) | `option_value_tests::{doctor_rejects_unknown_modes, install_rejects_unknown_modes, recall_and_history_reject_unknown_depths, recall_and_history_accept_the_mcp_depths_only}`, `tests/memory_parity.rs::depth_backup_and_feedback_keys_are_shared` | 312effa7, e698e4c0, 9200dffb |
| cli-053 | Fixed (test work and refactor, no RED possible) | `server_apply::tests::{replica_memory_ids_are_stable_and_namespaced, staged_policies_never_overwrite_a_reviewed_pending_item, staged_policies_skip_rules_that_came_from_this_node}`, `commands::stop_hook::tests::{stop_hook_active_short_circuits, no_violations_never_block, violations_block_with_a_capped_reason, scan_caps_files_and_skips_large_or_missing_ones}` | f4c01ed2 |
| cli-054 | Fixed (RED seen) | `server_http::tests::{exchange_rejects_unauthenticated_requests_before_reading_the_body, exchange_bodies_are_capped_and_must_parse, exchange_binds_authenticated_node_and_project_before_retaining_data}` | 37bdaad9 |
| cli-055 | Fixed (RED seen) | `server_http::tests::{three_nodes_exchange_complete_graph_source_memory_logs_and_conversations_over_http, bootstrap_requests_register_a_replica_grant_only_once}`, `commands::costs::tests::run_imports_a_cursor_transcript_before_the_report`, `test_env::tests::guard_restores_previous_values_on_drop` | 90d27d52 |
| cli-056 | Fixed (RED seen) | `server_push::tests::{primary_listens_on_the_http_bind_address_only, a_replayed_datagram_is_rejected, the_replay_guard_stays_bounded}` | 679ae712, b54600a9 |
| cli-057 | Fixed (RED seen) | `server_snapshot::tests::rejects_memory_file_references_that_leave_the_project` | c84e55b1 |
| cli-058 | Fixed (no RED possible: the old code was correct but blocked an executor thread) | `server_http::tests::three_nodes_exchange_complete_graph_source_memory_logs_and_conversations_over_http` | 7452b7df |
| cli-059 | Fixed (RED seen) | `ui::progress::tests::phase_headings_are_whole_lines` | 3422673c |
| cli-060 | Fixed (RED seen) | `ui::update_notice::tests::{closing_rule_matches_the_version_line_width, notice_names_both_versions_and_the_upgrade_command}` | f8704223 |
| cli-061 | Fixed (RED seen) | `version_check::tests::{gh_auth_token_gives_up_on_a_hanging_gh, gh_auth_token_reads_the_token}` | 3ecb9fa7 |
| cli-062 | Fixed (RED seen) | `version_check::tests::prerelease_is_lower_than_its_final_release` | 5668b854 |
| cli-063 | Fixed (RED seen) | `version_check::tests::huge_version_components_compare_without_wrapping` | 5668b854 |
| cli-064 | Fixed (RED seen) | `version_check::tests::{unparseable_latest_is_not_an_update, cdn_latest_must_be_a_release_version}` | 5668b854 |
| cli-065 | Fixed (refactor, no RED possible) | `version_check::tests::{parse_releases_skips_drafts_and_prereleases_and_keeps_assetless_releases, unique_tags_desc_picks_highest_semver}` | ce140027 |
| cli-066 | Fixed (RED seen) | `version_check::tests::{unique_tags_desc_drops_non_semver_tags, unique_tags_desc_orders_many_mixed_tags}` | 5668b854 |
| cli-067 | Fixed | `version_check::tests::{resolver_reports_hidden_releases_without_a_token, resolver_reports_no_releases_with_a_token}` | 9c072fdf |
| cli-068 | Fixed (missing tests added) | `version_check::tests::{resolver_takes_the_highest_github_version_and_skips_the_cdn, resolver_ignores_a_non_version_api_tag, resolver_falls_back_to_the_cdn_when_github_has_nothing, resolver_rejects_an_html_cdn_body, cache_freshness_handles_stale_and_future_timestamps, cache_round_trips_and_ignores_a_corrupt_file}` | 9c072fdf |
| cli-069 | Fixed (test isolation) | `tests/cli_parity_turn.rs` (1 test), `tests/cli_parity_gaps.rs` (3 tests), `tests/memory_parity.rs` (7 tests) | e84647ad |
| cli-070 | Fixed (assertions made able to fail) | `tests/cli_parity_turn.rs`, `tests/cli_parity_gaps.rs` (exact JSON assertions) | e84647ad |
| cli-071 | Fixed (test cleanup guard) | `tests/daemon_lifecycle.rs::{e3_a_newer_proxy_restarts_an_older_daemon_on_its_own_binary, d7_a_proxy_keeps_serving_after_its_daemon_is_killed}` | 5ac59a96 |

Totals: 58 fixed, 1 needs an owner decision (cli-024), 12 not done (cli-031 to cli-042), 0 not a defect.

## Review rounds

| Round | Findings | Fixed | Commit(s) |
|---|---|---|---|
| 1 | 1 | 1 | b54600a9 |
| 2 | 2 | 2 | 9200dffb |
| 3 | 2 | 2 | 71f2e400 |
| 4 | 2 | 2 | e8ebf087, 24bf4e14 |

## Commands run (final check)

All run in `/Users/gary/io/ax-wt/cli` with `CARGO_TARGET_DIR=target-dev`.

| Command | Result |
|---|---|
| `cargo test -p ax-cli` | Pass: 311 passed, 0 failed, 0 ignored across 16 test binaries (260 unit tests in `src/main.rs`, 51 integration tests in 15 files) |
| `cargo clippy -p ax-cli --all-targets -- -D warnings` | Pass: no lint errors. Cargo prints a manifest warning that `windows-sys` is unused; it is used only under `cfg(windows)` in `ui/terminal.rs` and was there before this PR |
| `cargo fmt --all --check` | First run: differences in 5 files this PR changed (`savings.rs`, `skill_catalog.rs`, `turn_hook.rs`, `uninit.rs`, `help_text.rs`). Fixed with `cargo fmt --all` in a96a3147; the second run is clean |

There is no web UI or site build in this PR's area. The site pages changed by this PR are Markdown only and were not built here.

## Open items

- **cli-024 (owner decision):** a bootstrap-token request registers any self-asserted node id. Fixing it needs a protocol change: per-node credentials issued at enrollment. That raises three questions: where replicas and the Command Center pairing flow store them, how already-paired replicas migrate, and whether revoking a node rotates the bootstrap token.
- **cli-031 to cli-042 (not done):** each was re-checked and still holds. They were not started because free disk space was below the 15 GB build threshold at the time.
- cli-048: `ax uninit` still deletes all of `.ax/` after confirmation. A default that keeps `policy/` and `memory/` behind a `--purge` flag was not added.
