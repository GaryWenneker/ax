# Evidence: fix/review-index-core

Branch `fix/review-index-core`, based on `9dca60e4`. This PR covers ax-core, ax-db, ax-extraction, ax-resolution, ax-graph, ax-sync, ax-lsp, ax-types, ax-git, ax-tia and ax-utils.

Result: 42 findings fixed, 33 not done, 0 need an owner decision. Every fixed finding has a test that failed before the fix (RED) and passes now, except where the row says otherwise.

## Findings

| Finding | State | Tests that cover it | Commit |
|---|---|---|---|
| index-core-001 | fixed | ax-resolution `frameworks::express::tests::{unbalanced_route_call_does_not_panic, regex_literal_paren_does_not_panic, unbalanced_arrow_body_does_not_panic, balanced_route_call_still_yields_handler_calls}` | 0629dca2 |
| index-core-002 | fixed | ax-resolution `frameworks::react::tests::{route_tag_window_cut_inside_a_character_does_not_panic, data_router_window_cut_inside_a_character_does_not_panic}`, `frameworks::rust::tests::{actix_resource_window_cut_inside_a_character_does_not_panic, actix_next_resource_far_away_cut_inside_a_character_does_not_panic}` | 06340d19 |
| index-core-003 | fixed | ax-sync `watcher::tests::{a_save_during_indexing_stays_pending_after_success, a_failed_batch_stays_pending_and_retries_after_debounce, a_successful_batch_clears_its_entries}` | 16dc4b34 |
| index-core-004 | fixed | ax-core `tests/sync.rs::incremental_index_keeps_project_wide_resolution_stats` | 63830ad5 |
| index-core-005 | fixed | ax-core `tests/sync.rs::{scoped_index_of_a_code_file_skips_project_wide_passes, doc_links_survive_a_scoped_reindex_of_the_code_file, scoped_index_of_a_doc_file_relinks_docs, index_names_each_maintenance_step}`, `read_path_tests::a_read_path_resync_skips_project_wide_passes` | c9536f1a (review fixes 545746cd, 5cc4773a, 85361acc) |
| index-core-006 | fixed | ax-core `okf::tests::{dot_ax_json_is_read_when_ax_json_is_missing, dot_ax_json_is_read_when_ax_json_does_not_parse}` | 2a1c78b0 |
| index-core-007 | fixed | ax-core `okf::tests::{publish_refuses_subdir_that_climbs_out_of_the_wiki_clone, publish_refuses_local_clone_outside_the_project, export_refuses_out_dir_outside_the_project, publish_clones_a_valid_remote_into_the_configured_subdir}` | 7d3ebf55 (review fix 52729ea3) |
| index-core-008 | fixed | ax-core `okf::tests::{publish_refuses_remote_that_looks_like_a_git_option, publish_clones_a_valid_remote_into_the_configured_subdir}` | 7d3ebf55 |
| index-core-009 | fixed | ax-core `workspace::tests::{write_refuses_to_replace_an_unparseable_ax_json, write_keeps_other_settings}` | 5b1e8bf4 |
| index-core-010 | fixed | ax-db `tests/builds.rs::{a_row_left_by_an_exited_newer_build_does_not_block_writers, a_running_newer_build_still_blocks_writers, the_version_orders_builds_before_the_file_time}`, ax-core `tests/build_writers.rs::background_watch_returns_when_a_newer_build_is_registered` | 94f432c3 |
| index-core-011 | fixed | ax-db `tests/integrity.rs::the_unattended_quick_check_leaves_a_damaged_database_alone` (whole file green) | 68035371 (review fixes f7c6b6db, 4e7b7336) |
| index-core-012 | fixed | ax-db `tests/migration_race.rs::{a_second_opener_with_a_stale_version_skips_applied_migrations, concurrent_opens_of_an_old_database_all_succeed}`, `migrations::tests::a_failing_migration_leaves_nothing_half_applied` | 6a0be745 (review fix 525d0b86) |
| index-core-013 | fixed | ax-db `tests/search_filters.rs::exact_name_supplement_respects_kind_language_and_path_filters` | 31351e1e |
| index-core-014 | fixed | ax-db `replication::tests::unchanged_dataset_is_not_reassembled_or_revalidated` | b666d812 |
| index-core-015 | fixed | ax-db `replication::tests::failed_projection_does_not_leave_the_replica_attached` | b666d812 |
| index-core-016 | fixed | ax-extraction `tests/contract_federation.rs::shared_operation_links_per_file_nodes_without_self_loops`, `contracts::tests::{parses_openapi_json, parses_proto_rpc}` | f0527008 (review fix 7b9b1330) |
| index-core-017 | fixed | ax-extraction `contracts::tests::{sniff_reads_only_the_head_so_later_non_utf8_bytes_do_not_hide_openapi, exclude_patterns_use_gitignore_semantics}` | f0527008 |
| index-core-018 | fixed | ax-extraction `languages::common::tests::{yaml_pairs_are_named_by_their_key, xml_elements_are_named_by_their_tag, a_node_without_a_name_never_takes_its_whole_source_as_name, c_functions_are_named_by_their_declarator}` (the C test was not run RED separately) | 36ac645f (review fix 85361acc) |
| index-core-019 | fixed | ax-extraction `languages::kotlin::tests::{extracts_kotlin_classes_objects_interfaces_and_functions, extracts_kotlin_call_references_and_local_call_edges, kts_is_mapped_to_kotlin}` | 1ac93abe |
| index-core-020 | fixed | ax-extraction `tests/deep_nesting.rs::{deeply_nested_javascript_is_extracted_without_overflow, deeply_nested_typescript_is_extracted_without_overflow, deep_call_chains_and_parentheses_are_extracted_without_overflow}` | d95e5500 |
| index-core-021 | fixed | ax-extraction `languages::refs::tests::{member_call_on_another_object_is_not_a_same_file_edge, this_call_still_links_to_the_local_method, rust_receiver_call_is_not_a_same_file_edge}` | b2fc86cc (review fix 8afe7372) |
| index-core-022 | fixed | ax-extraction `languages::rust::tests::{free_fn_is_one_function_and_impl_fn_is_one_method, test_fn_in_a_test_file_stays_a_test}` | b0e6c66e |
| index-core-023 | fixed | ax-extraction `tests/index_files_filters.rs::index_files_applies_the_scan_filters_before_reading`, `tests/source_store_write_path.rs` | 4a7d1dcc (review fix 83120519) |
| index-core-024 | fixed | ax-extraction `tests/index_files_filters.rs::{generated_file_is_not_reindexed_on_every_sync, file_that_becomes_generated_loses_its_old_nodes}` | 4a7d1dcc |
| index-core-025 | fixed | ax-extraction `test_mapper::tests::{helper_far_below_an_early_test_attribute_is_not_a_test, helper_right_after_a_test_fn_is_not_a_test, attribute_directly_above_marks_a_test_far_into_the_file, tokio_and_python_markers_are_detected, it_marker_needs_a_token_boundary}` | 1ac07563, fdecc61b |
| index-core-026 | fixed | ax-git `tests/diff_paths.rs::{renamed_copied_and_non_ascii_paths_are_reported_as_plain_paths, user_prefix_config_does_not_drop_hunks, deleted_file_hunks_are_not_given_to_the_previous_file}`, `diff::tests::{name_status_z_takes_the_new_path_of_renames_and_copies, target_paths_are_unquoted}` | bfc3ab78 (review fix 5c0d8c29) |
| index-core-027 | fixed | ax-git `diff::tests::{hunk_header_with_function_context_is_parsed, parse_hunk_header_works}` | e9841bea |
| index-core-028 | fixed | ax-graph `tests/dead_code.rs::{dead_code_covers_every_node_past_ten_thousand, dead_code_skips_referenced_and_exported_nodes}` | 6e2a947d |
| index-core-029 | fixed | ax-graph `query_utils::tests::test_file_detection_covers_common_layouts`, ax-utils `paths::test_path_tests::recognises_test_directories_and_names` | 243ca854 |
| index-core-030 | fixed | ax-lsp `client::tests::{column_for_name_counts_utf16_units, column_for_name_survives_a_stale_column_inside_a_character, uris_percent_encode_and_decode_utf8, column_for_name_finds_method}` | b997bcb3 |
| index-core-031 | fixed | ax-lsp `enrich::tests::{server_is_resolved_and_started_once_per_batch, work_past_the_deadline_is_left_for_the_next_run}`, `servers::probe_timeout_tests::a_hanging_probe_is_killed_and_counts_as_not_working` (the deadline test was not RED) | fbce8c24 |
| index-core-032 | fixed | ax-lsp `tests::definition_inside_a_method_targets_the_method_not_its_class` | 13677af0 |
| index-core-033 | fixed | ax-lsp `servers::project_bin_tests::project_local_binaries_are_not_run_by_default`, `servers::tests::l3_trusted_node_modules_bin_is_searched_for_node_servers` | 5f6f8ac3 |
| index-core-034 | fixed | ax-resolution `tests/jsx_child_edges.rs` (5 tests: imported, same file, ambiguous, unique global, source store) | 7086f062 |
| index-core-035 | fixed | ax-resolution `tests/framework_gates.rs` (4 tests) | a39b0b2e |
| index-core-036 | fixed | ax-resolution `frameworks::{go,rust,spring}::tests::get_and_post_on_one_path_are_two_routes`, `frameworks::extract::tests::dedup_nodes_keeps_the_first_node_per_id` | cd367047 |
| index-core-037 | fixed | ax-resolution `tests/framework_post_extract.rs::sveltekit_routes_reach_the_graph` | 5d66ecc2 |
| index-core-038 | fixed | ax-resolution `frameworks::rust::tests::{plain_messages_do_not_become_tables, sql_strings_give_one_table_node_per_name}` | 94a4fc3a (review fix 8afe7372) |
| index-core-039 | fixed | ax-resolution `tests/framework_post_extract.rs::scoped_runs_reuse_framework_detection` | f00f0679 |
| index-core-040 | fixed | ax-resolution `frameworks::vue::tests::{non_js_files_get_no_nuxt_nodes, nuxt_middleware_in_js_is_kept}` | ee02c781, ae709100 |
| index-core-041 | fixed | ax-resolution `tests/framework_post_extract.rs::{compiler_macros_are_dropped_without_a_self_edge, vue_auto_imports_are_left_alone_outside_vue_projects}` | dcd0d435 |
| index-core-042 | fixed | ax-resolution `import_resolver::alias_tests::{longest_alias_wins_every_time, alias_matches_only_on_a_segment_boundary, exact_alias_matches_only_itself, every_target_is_tried_in_order, targets_are_relative_to_base_url, cargo_projects_get_no_made_up_alias}` | 250a12f9 |
| index-core-043 | not done | none | none |
| index-core-044 | not done | none | none |
| index-core-045 | not done | none | none |
| index-core-046 | not done | none | none |
| index-core-047 | not done | none | none |
| index-core-048 | not done | none | none |
| index-core-049 | not done | none | none |
| index-core-050 | not done | none | none |
| index-core-051 | not done | none | none |
| index-core-052 | not done | none | none |
| index-core-053 | not done | none | none |
| index-core-054 | not done | none | none |
| index-core-055 | not done | none | none |
| index-core-056 | not done | none | none |
| index-core-057 | not done | none | none |
| index-core-058 | not done | none | none |
| index-core-059 | not done | none | none |
| index-core-060 | not done | none | none |
| index-core-061 | not done | none | none |
| index-core-062 | not done | none | none |
| index-core-063 | not done | none | none |
| index-core-064 | not done | none | none |
| index-core-065 | not done | none | none |
| index-core-066 | not done | none | none |
| index-core-067 | not done | none | none |
| index-core-068 | not done | none | none |
| index-core-069 | not done | none | none |
| index-core-070 | not done | none | none |
| index-core-071 | not done | none | none |
| index-core-072 | not done | none | none |
| index-core-073 | not done | none | none |
| index-core-074 | not done | none | none |
| index-core-075 | not done | none | none |

Findings 043 to 075 were not started. Free disk space fell below the 15 GB floor the task sets for starting builds (about 12 GB free at that time). The batch agents checked each one against the current code without building, and every one still holds. Each row's plan is listed under "Open items".

## Review rounds

| Round | Findings | Fixed |
|---|---|---|
| 1 | 5 | 5 |
| 2 | 5 | 4 |
| 3 | 2 | 2 |
| 4 | 2 | 2 |

The batch record gave 0 fixed for round 1. The branch has five round-1 commits (7b9b1330, 83120519, 525d0b86, f7c6b6db, 5c0d8c29), one for each finding, so this table shows 5. Round 2 has three commits (8afe7372, 545746cd, 4e7b7336); 8afe7372 covers two findings. The round 2 finding that was not fixed is not named in the batch record.

## Commands run for this check

Everything ran in `/Users/gary/io/ax-wt/index-core` at commit 1bdf0f89 (the formatting commit).

| Command | Result |
|---|---|
| `cargo fmt --all --check` | reported differences in ax-core `lib.rs` and ax-lsp `enrich.rs`, `lib.rs`, `servers.rs`; fixed with `cargo fmt --all` (commit 1bdf0f89), then clean |
| `cargo test -p ax-utils` | 31 passed, 0 failed |
| `cargo test -p ax-types` | 0 tests, 0 failed |
| `cargo test -p ax-git` | 9 passed, 0 failed |
| `cargo test -p ax-db` | 89 passed, 0 failed |
| `cargo test -p ax-graph` | 15 passed, 0 failed |
| `cargo test -p ax-tia` | 0 tests, 0 failed |
| `cargo test -p ax-sync` | 31 passed, 0 failed |
| `cargo test -p ax-lsp` | 16 passed, 0 failed, 1 ignored |
| `cargo test -p ax-extraction` | 57 passed, 0 failed |
| `cargo test -p ax-resolution` | 72 passed, 0 failed |
| `cargo test -p ax-core` | 68 passed, 0 failed |
| `cargo clippy -p <crate> --all-targets -- -D warnings`, for each of the 11 crates | clean (11 of 11) |
| `cargo check -p ax-web -p ax-cli` (callers of changed OKF APIs) | compiles |
| `npm test` in `crates/ax-web/web-ui` | 91 passed, 0 failed |
| `npm run build` in `crates/ax-web/web-ui` | built |
| `npm test` in `site` | 89 passed, 0 failed (the first run failed 3 Puppeteer tests because `node_modules` was missing; `npm ci` fixed that) |
| `npx astro build` in `site` | 49 pages built |

Total for the Rust crates: 388 passed, 0 failed, 1 ignored.

## Open items

- index-core-043 to index-core-075 are not done. They are not started because of the disk floor. The plan for each is in the batch record. The short version:
  - 043: normalize `.` and `..` in `resolve_to_indexed_file`.
  - 044: the name matcher must filter by kind and handle ambiguity.
  - 045: `strip_comments` needs a stateful scanner that skips strings and keeps newlines.
  - 046 and 047: git hooks must use `git rev-parse --git-path hooks` and never write after a failed read.
  - 048: the watcher must set `degraded` and honor its options.
  - 049 and 050: tia must count changed test files and more symbol kinds, and must quote runner hints.
  - 051 and 053: `file_lock` must stop retrying on errors that are not contention, and needs an async wait for the ax-core callers.
  - 052: kill only ax processes on the port.
  - 054: cache the global policy skill rows.
  - 055: OKF page names must not collide, stale pages must be removed, and link targets escaped.
  - 056: the ax-core tests must redirect `AX_GLOBAL_DB`.
  - 057: write graph history in one transaction.
  - 058: hash backups by streaming and prune old ones.
  - 059: checkpoints must not leak `busy_timeout = 0` into the pool.
  - 060: apply the path filters in SQL before `LIMIT`.
  - 061: get doc extensions from `Path::extension`.
  - 062: run `clear_all` in one transaction.
  - 063: stop scanning the blob directory once per chunk.
  - 064: use `try_get("history_rows")`.
  - 065: the migration test rewinds must drop what they add.
  - 066: node ids must use a stable hash.
  - 067: move the persist loop into a shared helper (pure move).
  - 068: git refs and paths need `--end-of-options` and `--`.
  - 069: cache the NestJS regexes.
  - 070 and 071: quote hook paths safely, and remove only ax-managed hook lines.
  - 072: the watcher must build its ignore matcher once and read nested ignore files.
  - 073: the debounce env test must not race the other tests.
  - 074: tia needs one BFS without the extra queries.
  - 075: the RSS probe must not block or treat 0 as a reading.
- index-core-012 leaves one choice for the owner: whether to refuse to open a database whose stored schema version is newer than this binary. Refusing would break opening the database again after a downgrade.
- `crates/ax-web/web-ui/dist` has uncommitted generated build output in the worktree (a new asset hash). It is not part of this branch's commits.
- The site check ran `npx astro build` and not `npm run build`. The full script also writes version and Azure template files and renders OG images, and none of that depends on this PR's doc edits.
