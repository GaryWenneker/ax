# Evidence: fix/review-memory (ax-memory, ax-context, ax-reasoning)

Base commit: `9dca60e4`. Spec: `docs/specs/full-review-fixes/SPEC.md`.

## Findings

| Id | State | Tests | Commit |
|---|---|---|---|
| memory-001 | Fixed. Offload is on only with a URL in `~/.ax/config.json` or `AX_OFFLOAD_URL`; a provider key alone is a suggestion (`ax offload set-endpoint`). | ax-reasoning `config::tests::provider_key_alone_does_not_enable_offload`, `init_does_not_activate_offload_from_env_key`, `configured_url_enables_offload`, `env_url_enables_offload` | c25a70c3, f84f3b69 |
| memory-002 | Fixed. Anchor flags are set and cleared by four set-based UPDATEs in one transaction. | ax-memory `tests/anchors.rs::orphan_flag_clears_when_the_node_comes_back`, `stale_flag_follows_the_live_body_hash` | cf43a78b |
| memory-003 | Fixed. A rename needs a gone old key, a matching kind and hash, a newer first version and a unique match both ways. | `tests/anchors.rs::reanchor_leaves_two_live_symbols_with_the_same_signature_alone`, `reanchor_moves_anchors_when_a_symbol_is_renamed`, `reanchor_skips_a_removed_symbol_with_several_candidates` | cf43a78b, 01ba512e, fc668394 |
| memory-004 | Fixed. Restore verifies checksum, row count and every line before writing, and keeps timestamps, confidence, enabled and tier. Backups page through the whole vault and are written atomically. | `tests/restore.rs::restore_keeps_timestamps_confidence_enabled_and_tier`, `restore_refuses_a_backup_whose_checksum_does_not_match`, `restore_writes_nothing_when_a_line_is_malformed`, `restore_refuses_a_backup_without_manifest`; `tests/backup.rs::incremental_backup_holds_only_rows_updated_after_since` | 72100f54, 9571a2e1, aae64ac9, 2576581e |
| memory-005 | Fixed. The function is now `score_dataset_text_stack` and its results carry a `scorer` label. | `bench::tests::public_score_is_labelled_as_the_text_stack` | 81378fdc |
| memory-006 | Fixed. The graph fingerprint no longer holds line numbers; only changed state rows are written. | `tests/brain.rs::graph_episode_ignores_line_shifts`, `graph_episode_accepts_fingerprints_that_still_carry_line_numbers` | 0347f60a |
| memory-007 | Fixed. Graph episode capture runs in a sqlx transaction that rolls back on cancel. | `tests/brain.rs::cancelled_graph_capture_leaves_no_open_transaction` | c0bab4cb |
| memory-008 | Fixed. Folder sync fails without changes on a missing root and keeps memories of files it could not see. | `tests/folder_sync.rs::s7_missing_root_is_an_error_and_keeps_existing_memories`, `s7_a_file_that_grows_past_the_limit_keeps_its_memory`, `s7_unreadable_files_and_folders_keep_their_memories` | 98671cf1 |
| memory-009 | Fixed. `when:` file patterns are globs over the whole path. | `prospective::tests::file_globs_match_the_whole_path`, `plain_file_patterns_match_the_path_or_file_name_exactly`, `non_file_globs_check_the_suffix` | ee9fc842, 6068563b |
| memory-010 | Fixed. Search text normalization is linear and position-independent. | `search_text::tests::a_word_normalizes_the_same_wherever_it_appears`, `a_large_document_normalizes_quickly` | 4d3ca654 |
| memory-011 | Fixed. `renamed` and `corroborates` links are stored; a corroborated pair counts once. | `tests/anchors.rs::reanchor_moves_anchors_when_a_symbol_is_renamed`; `tests/memory_links.rs::corroborating_two_lessons_links_them_and_counts_once`, `only_the_named_link_kinds_are_stored` | 257f5eb5 |
| memory-012 | Fixed. Hot recall uses one constant SQL statement; cosine scoring keeps only the best rows. | `tests/recall_statements.rs::hot_recall_does_not_prepare_a_new_statement_per_hot_set`, `find_similar_keeps_the_best_rows_across_many_candidates` | 3f337550 |
| memory-013 | Fixed. Recall loads hits and access times with one query each; PMI samples a bounded, ordered set of semantic memories. | `tests/pmi_sample.rs::pmi_ignores_turns_docs_and_archive_rows` | e4a3154d, cd8287f2 |
| memory-014 | Fixed. `update()` re-derives search text, origin, trust, instruction flag and anchors. | `tests/update_delete.rs::update_rederives_search_text_instruction_flag_and_anchors` | fdb89967 |
| memory-015 | Fixed. Rebuilding cues keeps explicit triggers. | `tests/update_delete.rs::update_keeps_explicit_triggers`; `tests/purge.rs` | fdb89967 |
| memory-016 | Fixed. `delete()` removes vectors, cues, links, anchors and access rows in one transaction. | `tests/update_delete.rs::delete_removes_cues_links_anchors_access_and_vectors`; `tests/purge.rs` | fdb89967 |
| memory-017 | Fixed. Shared export reads every tagged row without a cap and writes atomically. | `tests/shared_sync.rs::export_includes_shared_memories_older_than_ten_thousand_other_rows`; `tests/turns.rs` | eecbcb21, 92d1bd65 |
| memory-018 | Fixed. Import parses every line first and never replaces a newer local edit. | `tests/shared_sync.rs::import_never_replaces_a_newer_local_edit`, `import_applies_a_newer_shared_row_and_keeps_its_timestamp`, `a_bad_line_aborts_the_import_before_anything_is_written` | eecbcb21 |
| memory-019 | Fixed. Imported rows are untrusted, get derived columns, respect tombstones and kinds, and instruction-like rows wait as candidates. | `tests/shared_sync.rs::imported_rows_are_untrusted_and_get_derived_columns`, `import_never_recreates_a_purged_memory`, `import_rejects_unknown_and_recall_only_kinds`, `import_never_overwrites_a_local_memory_that_is_not_shared`, `an_applied_shared_update_marks_the_row_untrusted`, `instruction_like_imports_wait_as_candidates` | fdae0c66 |
| memory-020 | Fixed. Node versions hash the node's source lines; old signature hashes are upgraded in place. | `tests/sync_capture.rs::a_body_rewrite_with_the_same_signature_makes_the_anchor_stale`, `versions_recorded_with_the_old_signature_hash_are_upgraded_not_flagged` | 474c2285, 693d7dad |
| memory-021 | Fixed. `file_change` rows are recent tier, record each new content, follow turn retention and stay out of prompt cues. | `tests/file_changes.rs::sync_file_changes_are_recent_tier_and_record_every_new_content`, `turn_retention_archives_old_file_changes`, `file_change_cues_do_not_crowd_out_prompt_cue_hits` | 82d40979, a2fc3377, 7cccc9cf |
| memory-022 | Fixed. Redaction covers more key names and token shapes, and git commit titles. | `tests/turns.rs::redacts_common_secret_shapes`, `ordinary_text_is_not_redacted`; `tests/brain.rs::a_git_capture_redacts_secrets_in_the_title` | 4f8b6c10 |
| memory-023 | Fixed. Config writes are locked and atomic; a broken or non-object config is an error and stays untouched. | `config::tests::a_config_that_does_not_parse_is_never_overwritten`, `a_config_whose_root_is_not_an_object_is_an_error_not_a_panic`, `writing_offload_keeps_the_index_section` | 84aca7e6 |
| memory-024 | Fixed. The model comes from env, config, then the provider default; a provider with no default leaves offload off with a `problem`. | `config::tests::a_known_provider_without_a_model_uses_its_default_model`, `a_set_model_wins_over_the_provider_default`, `a_provider_without_a_default_model_needs_one_before_offload_runs` | 92c6bcb0 |
| memory-025 | Fixed. `reasoning_effort` is sent only when set; failures are recorded as `last_error`. | `reasoner::tests::reasoning_effort_is_sent_only_when_configured`, `an_upstream_rejection_is_reported_in_status` | adb23b6b |
| memory-026 | Fixed. Root and home checks compare canonical paths. | ax-context `tests/unsafe_root.rs::home_and_roots_are_refused_however_spelled` | 674ee2ca |
| memory-027 | Fixed. `..` and drive-relative paths are rejected. | `tests/scope_guard.rs::parent_components_escape_the_project_even_in_absolute_paths`, `drive_relative_paths_are_rejected` (Windows only) | c8f4ac52 |
| memory-028 | Fixed. Hunks are cut on a char boundary; an unformattable backup date does not panic. | `turns::hunk_tests::a_large_hunk_is_cut_on_a_char_boundary`, `a_backup_with_an_unformattable_time_does_not_panic` | e206f214 |
| memory-029 | Not done in full. Fixed: every insert path fills the derived columns, and the recall-only kind list lives in one place. Not done: one shared `insert_memory` run in a transaction (needs `stamp_memory`, `register_cues` and `index_archive_vector` generic over an executor). | `tests/derived_columns.rs::every_insert_path_fills_the_derived_columns`; `types::recall_only_tests::the_sql_tuple_matches_the_kind_list` | 012a03b5 |
| memory-030 | Fixed. Every ax-memory test uses `tempfile::tempdir()`. | `tests/temp_dirs.rs::tests_use_tempfile_not_a_hand_built_temp_dir` | b8ee811f |
| memory-031 | Fixed. The foreign-embedding test fails when the vector map is missing. | `tests/brain.rs::a_foreign_embedding_is_not_inserted_into_the_vector_index` | d3e610d5 |
| memory-032 | Fixed. Test git commands ignore the developer's git config and hooks. | `tests/brain.rs`, `tests/history.rs`, `tests/derived_columns.rs` (shared `tests/common/mod.rs`) | 0b66282c |
| memory-033 | Fixed. p95 is a nearest-rank percentile over 50 runs. | `tests/brain_scale.rs::p95_is_the_nearest_rank_value_not_the_slowest_sample` | ae06c8b5 |
| memory-034 | Fixed. The bench test checks where every attached database file lives. | `tests/memory_bench.rs::coding_fixture_covers_every_family_and_is_deterministic`, `write_coding_baseline` (ignored) | f5e75696 |
| memory-035 | Fixed (test-only refactor, no RED possible). Both incident tests share one strict setup. | `tests/query_incident.rs` | 608eabd3 |
| memory-036 | Fixed. The prune tests are named after demotion and p3 checks the tier. | `tests/turns.rs::p3_failed_backup_demotes_nothing`, `prune_demotes_only_old_turn_memories`, `p1_p2_turn_older_than_retention_is_backed_up_then_archived` | c844e0e2 |
| memory-037 | Fixed. A configured key is never sent to a different endpoint from the env. | `config::tests::a_configured_key_is_not_sent_to_an_endpoint_from_the_env`, `a_configured_key_still_pairs_with_the_same_endpoint_or_a_key_only_config`, `a_key_without_an_endpoint_is_never_used` | 8ca93b0f |
| memory-038 | Fixed (missing tests, no RED possible). ax-reasoning now has 36 tests, up from 2. | `config::tests::env_settings_win_over_the_config_file`, `disable_wins_over_every_endpoint`, `init_reports_an_existing_endpoint_without_changing_it`; `reasoner::tests::a_reply_without_an_answer_falls_back_to_the_explore_output`, `a_server_error_is_reported_and_does_not_pause_offload`, `report_style_returns_the_answer_without_the_footer` | 75ee1a0c |
| memory-039 | Fixed. Stripping keeps numbered source, source sections and code fences. | `reasoner::tests::strip_keeps_retrieved_source_verbatim`, `strip_not_shown_block_ends_at_a_blank_line_or_heading`, `strip_collapses_every_run_of_blank_lines`, `strip_removes_ax_hints_outside_source` | caaa4386 |
| memory-040 | Fixed. Each request logs caller, outcome and latency; status shows every setting. | `reasoner::tests::status_reports_every_setting_that_shapes_a_request` | e7f9d5fb |
| memory-041 | Fixed. One shared HTTP client per connect budget. | `reasoner::tests::requests_reuse_one_connection` | d9801d47 |
| memory-042 | Fixed. Connect timeout, positive-only limits, and a per-endpoint back-off after transport failures. | `config::tests::a_zero_or_invalid_timeout_uses_the_default`; `reasoner::tests::an_unreachable_endpoint_is_skipped_for_a_while_after_a_failure`, `a_zero_back_off_retries_at_once`, `a_connect_timeout_starts_the_back_off`, `a_connect_timeout_shorter_than_the_default_starts_the_back_off`, `a_slow_reply_that_times_out_does_not_pause_offload`, `connect_timeout_does_not_overflow_on_huge_request_timeout` | eca660c4, f9072905, 899a9728, 3470da21, 5135d031, cae2eecf |

Totals: 41 fixed, 1 not done in full (memory-029), 0 not a defect.

## Review rounds

| Round | Findings | Fixed |
|---|---|---|
| 1 | 5 | 5 |
| 2 | 6 | 6 |
| 3 | 3 | 3 |
| 4 | 1 | 1 |

## Final checks (this run)

| Command | Result |
|---|---|
| `cargo test -p ax-memory` | pass: 239 passed, 0 failed, 3 ignored (35 test binaries) |
| `cargo test -p ax-context` | pass: 46 passed, 0 failed |
| `cargo test -p ax-reasoning` | first run: 35 passed, 1 failed (`a_connect_timeout_shorter_than_the_default_starts_the_back_off`, a timing flake). Fixed in cae2eecf; after that 6 runs (5 of the lib, 1 full) of 36 passed, 0 failed |
| `cargo clippy -p ax-memory --all-targets -- -D warnings` | pass |
| `cargo clippy -p ax-context --all-targets -- -D warnings` | pass |
| `cargo clippy -p ax-reasoning --all-targets -- -D warnings` | pass |
| `cargo fmt --all --check` | pass |
| `cargo check -p ax-agent -p ax-mcp -p ax-cli` (callers changed by this PR) | pass |
| `site`: `npm ci`, `npm test` | pass: 89 passed, 0 failed |
| `site`: `npm run build` | pass |

## Changes outside the owned crates

- `crates/ax-cli/src/commands/init.rs`: `ax init` prints the `ax offload set-endpoint` hint (memory-001).
- `crates/ax-mcp/src/tools.rs`: offload hint and the project-relative guard path for prospective globs (memory-001, review round 2).
- `crates/ax-agent/src/chat.rs`: chat error shows the last offload error (memory-025).
- Site docs: `getting-started/configuration.md`, `guides/memory.md`, `guides/obsidian-vault.md`, `guides/workspaces.md`, `reference/cli.md`.
- `docs/archive/specs/*-EVIDENCE.md`: renamed test names (memory-036).

## Open items

- memory-029: one shared `insert_memory(executor, NewMemory)` in a transaction is not done. `folder_sync::is_recall_only` still covers only `turn` and `doc`, since widening it changes what is shared.
- memory-004: backups are still not wired to a command, and `Depth::Backup` stays. Whether to wire or remove them is a product choice.
- memory-005: `bench.rs` is not behind a feature and the `AX_MEMORY_BENCH_*` switches stay in `store.rs`, because ax-neural uses `ax_memory::bench` in production code.
- memory-012: embedding blobs are still read on every recall. A cache needs a shared write counter, which needs an ax-db migration.
- memory-018: import writes are not one SQL transaction; a database error partway can leave earlier rows applied (parse errors cannot).
- memory-020: version writes are not batched in one transaction; `record_node_version` lives in ax-db.
- memory-033: the scheduled CI job for wall-clock budgets is not wired; the budgets are a manual release check.
