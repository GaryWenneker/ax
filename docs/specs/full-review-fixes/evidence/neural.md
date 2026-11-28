# Evidence: fix/review-neural (ax-neural, ax-pi)

Base commit: `9dca60e4`. Branch head before this file: `95ec452c`.

## Findings

| Id | State | Tests that cover it | Commit(s) |
|---|---|---|---|
| neural-001 | fixed | `rss::tests::sample_counter_ignores_samples_taken_on_other_threads`, `benchmark_abc::tests::abc_report_samples_rss_once_per_run`, `agent_eval::tests::agent_eval_samples_rss_once_per_run` | 0bd83581 |
| neural-002 | fixed (review round 1) | `promote_reads_the_registered_evidence_and_refuses_caller_claims`, `a_candidate_without_its_own_bakeoff_evidence_is_refused`, `a_candidate_must_beat_the_measured_baseline_and_keep_the_locked_families`, `a_registered_candidate_without_evidence_is_not_promoted`; promote scores the baseline off the runtime thread (round 4) | 196353d2, a778fe4a |
| neural-003 | partly fixed; needs owner decision | `admission_can_be_computed_inside_an_async_runtime`, `a_failed_admission_is_not_cached_and_is_retried`, `async_admission_leaves_a_current_thread_runtime_free`, `the_report_uses_the_admission_it_is_given` | a76fd1cd, 539e9dd0, a778fe4a |
| neural-004 | fixed | `predict::tests::project_edges_are_never_predictions_by_themselves`, `predict::tests::predictions_are_capped`, `predict::tests::graph_edges_mark_relationship_verified` | 1d4739d4, 5ce7b0b2 |
| neural-005 | fixed | `predict::tests::category_match_needs_whole_tokens`, `predict::tests::held_out_evaluation_scores_the_coding_fixture` | 1d4739d4 |
| neural-006 | fixed | `rank::tests::blocks_that_differ_in_short_tokens_or_punctuation_are_not_duplicates`, `rank::tests::duplicates_ignore_only_whitespace_differences`, `rank::tests::duplicate_optional_is_dropped_and_better_overlap_goes_first` | 53981202 |
| neural-007 | fixed | `store::tests::a_hash_backend_never_ranks_semantic_search_even_when_the_lexical_challenger_is_admitted`, `store::tests::ranking_skips_wrong_dim_and_filters_by_model_id`, dispatch search test (model is hash-v1) | 878b02ea |
| neural-008 | fixed | `store::tests::a_failed_promote_leaves_the_previous_version_active`, `store::tests::a_failed_rollback_leaves_the_current_version_active` | d6b98914 |
| neural-009 | fixed | `store::tests::rollback_after_promoting_a_version_again_selects_the_one_in_between`, `store::tests::rollback_never_returns_to_a_version_that_was_rolled_back`, `store::tests::rollback_uses_the_model_history_from_every_project`, `store::tests::rollback_history_matches_the_model_id_exactly` | d6b98914 |
| neural-010 | fixed | `golden::changed_knowledge_for_the_same_task_is_not_served_from_the_cache` | 405ad0d4 |
| neural-011 | fixed | `golden::euro_economics_convert_usd_spend_with_the_configured_rate`, `golden::euro_economics_without_a_rate_stay_in_usd_and_compare_no_budget`, `golden::usd_per_eur_is_read_from_the_budget_settings`, `economics::tests::a_currency_without_a_rate_keeps_the_usd_label`, `a_monthly_budget_in_another_currency_is_left_out_and_says_why`, `a_budget_that_is_compared_has_no_omission_reason` | b9b99f60, 0c9fd23e, 95ec452c |
| neural-012 | fixed | `golden::a_second_run_on_the_same_database_keeps_the_first_runs_rows` | b64d756c |
| neural-013 | fixed | `golden::optimize_mode_never_replays_a_command_that_can_change_state`, `..._a_failed_call`, `..._when_the_repository_state_is_unknown`, `golden::optimize_mode_forgets_cached_reads_after_a_state_changing_call`, `a_repeat_that_could_not_be_replayed_is_not_logged_as_accepted` | 42145efb, 0e2f9d97 |
| neural-014 | fixed | `bakeoff::tests::challenger_admission_is_computed_once_per_process`, `a_failed_admission_is_not_cached_and_is_retried` | 13f70d45, 539e9dd0 |
| neural-015 | fixed | default parallel `cargo test -p ax-neural`; `tests/bench_flags.rs` (moved tests) | 33d37e8b |
| neural-016 | fixed | `bench_flags::doc_comment_affix_against_production` (ignored; passes on frozen input), `code_discovery::tests::independent_graph_discovery` (ignored) | 0553e257 |
| neural-017 | fixed (refactor) | existing `code_aware` and `code_discovery` tests; `independent_graph_discovery` figures identical to the recorded run | c31b06c4 |
| neural-018 | fixed | `code_discovery::tests::fan_out_counts_only_targets_that_are_added`, `a_large_hub_is_scanned_within_the_time_budget`, `a_large_hub_adds_the_same_neighbors_as_a_small_one` | c31b06c4, 72d25b4e |
| neural-019 | fixed | `config::tests::one_invalid_field_keeps_the_other_valid_fields`, `config::tests::unknown_keys_are_reported_with_their_path`, `config::tests::invalid_json_names_the_line_and_column`, `config::tests::fallback_backend_takes_cpu_or_hash_as_cpu_and_nothing_else`, `dispatch::tests::refused_feedback_and_predict_name_the_config_error` | 173aea9d |
| neural-020 | fixed | `dispatch::tests::share_and_predict_report_a_failed_database_read` | 6e159dde |
| neural-021 | fixed | `dispatch::tests::heavy_neural_work_leaves_the_runtime_thread_free`, `rss::tests::rss_reader_returns_positive_on_this_host` | b9513f19 |
| neural-022 | fixed | `dispatch::tests::learn_uses_stored_feedback_only_while_collection_is_on` | 519a6a59 |
| neural-023 | fixed | `dispatch::tests::project_id_is_the_same_for_every_spelling_of_one_folder`, `dispatch::tests::projects_sharing_a_database_keep_their_own_feedback`, `rows_written_under_the_old_raw_project_key_are_kept`, `adopted_project_id_moves_legacy_rows_without_a_dispatch_call`, ax-mcp `predictions_include_feedback_stored_under_a_legacy_key`, `rank_totals_merge_with_totals_stored_under_a_legacy_key`, ax-cli `export_includes_feedback_stored_under_a_legacy_key` | f67b47e7, 78a16727, 107a0884 |
| neural-024 | fixed (refactor) | `fixture::tests::median_takes_the_middle_sample`, `fixture::tests::locked_families_are_the_bake_off_families`, `agent_eval::tests::arm_id_serde_map_keys_match_stable_json_names` | be83689a |
| neural-025 | fixed | `embed_bakeoff::tests::an_embedding_error_marks_the_candidate_failed`, `embed_bakeoff::tests::timed_ranking_embeds_documents_once_and_each_query_once`, `embed_bakeoff::tests::embed_bakeoff_reports_measured_memory`, `bakeoff::tests::baseline_latency_is_measured`, `path_validate::tests::a_ranking_error_fails_the_arm_instead_of_scoring_it` | d36a99d4 |
| neural-026 | fixed | `learning::tests::a_secret_in_the_execution_id_is_rejected`, `learning::tests::wider_secret_shapes_are_rejected`, `learning::tests::oversized_feedback_is_rejected`, `a_bearer_word_without_a_token_in_the_same_field_passes` | d7ec0d9c, d1788d4f, 539e9dd0 |
| neural-027 | fixed | `metrics::tests::precision_divides_by_k_even_for_short_lists` | 59b6b92a |
| neural-028 | fixed | `metrics::tests::paraphrase_recall_is_the_mean_over_all_paraphrase_questions` | 59b6b92a |
| neural-029 | fixed (checksum pinning left to owner) | `onnx_embed::tests::no_home_directory_is_an_error_not_the_current_directory`, `onnx_embed::tests::model_checksum_matches_a_whole_file_hash` | 37587b32 |
| neural-030 | fixed | `onnx_embed::tests::a_failed_load_is_retried_on_the_next_call`, `onnx_embed::tests::a_poisoned_lock_is_recovered` (with `--features neural-ort`) | 37587b32 |
| neural-031 | fixed (refactor) | full `cargo test -p ax-neural`; `query_prior` exact-value tests; clippy with `--features neural-lab` | 554aea32 |
| neural-032 | fixed | `share::tests::checksum_accepts_uppercase_hex_and_a_trailing_newline`, `share::tests::checksum_rejects_a_changed_byte` | 7e95402a |
| neural-033 | fixed (partly holds) | `cargo build -p ax-neural`, lib clippy | b05eadae |
| neural-034 | fixed | `speed::tests::report_carries_basis_and_sample_counts`, `speed::tests::neural_modules_against_the_old_path` (ignored) | b05eadae |
| neural-035 | fixed | `store::tests::top_hits_keeps_at_most_k_and_matches_a_full_stable_sort`, `store::tests::ranking_skips_wrong_dim_and_filters_by_model_id` | ff094175 |
| neural-036 | fixed | `store::tests::list_models_breaks_created_at_ties_by_registration_order_whatever_the_plan`, `runtime::tests::runtime_cpu_embeds_without_network`, ax-pi `store::tests::flush_then_load_round_trips_every_row_kind`, `load_of_a_missing_database_is_empty_and_creates_nothing`, `record_advice_writes_session_call_and_optimization_into_an_existing_database`, `report::tests::eur_display_*`, `read_guard_ids::note_read_guard_records_advice_under_the_current_turn` | e93a6ae2 |
| neural-037 | fixed | `economics::tests::entity_inside_a_longer_word_is_not_a_known_symbol`, `entity_on_identifier_boundaries_still_matches`, `short_entity_names_never_match`, `last_segment_of_a_qualified_entity_is_a_partial_match`, `indexed_path_matches_whole_components_only`, `context::tests::path_invalidation_matches_whole_components_only` | b3a53159 |
| neural-038 | fixed | `events::tests::time_unix_nano_is_converted_to_milliseconds`, `events::tests::huge_output_token_counts_saturate`, `types::tests::usage_counters_saturate_instead_of_overflowing` | 55a3ce1a |
| neural-039 | fixed | `context::tests::the_cache_keeps_a_bounded_number_of_entries_and_drops_the_oldest`, `integration::tests::cached_tool_output_is_bounded_in_bytes`, `integration::tests::unfinished_tools_are_held_up_to_a_limit` | 81c4e9a1, 03417831 |
| neural-040 | fixed | `tool_pairing::interleaved_tools_without_ids_keep_their_own_arguments`, `integration::tests::tools_that_never_end_are_dropped_when_the_session_ends`, `integration::tests::a_turn_end_drops_that_sessions_unfinished_tools_only`, `integration::tests::unfinished_tools_are_held_up_to_a_limit` | 03417831 |
| neural-041 | fixed | `integration::tests::different_entity_lists_never_share_a_context_cache_entry`, `golden.rs` | f84941ff |
| neural-042 | fixed | `integration::tests::session_rows_carry_first_and_last_event_times` | 165714c1 |
| neural-043 | fixed | `report::aggregate_tests::unfiltered_report_is_labelled_as_all_sessions` | 3410bb1b |
| neural-044 | fixed | `report::aggregate_tests::report_lists_each_model_once_in_first_seen_order` | ba7874e7 |
| neural-045 | fixed | `report::aggregate_tests::group_reduction_is_saved_over_current_for_the_whole_group`, `report::aggregate_tests::potential_cost_matches_each_optimization_to_its_tool_call` | 0bd0beb1 |
| neural-046 | fixed | `store::tests::a_flush_that_fails_part_way_writes_nothing` | 589cca7b |
| neural-047 | fixed (dead code removed) | `store::tests::flush_then_load_round_trips_every_row_kind` | fdeaa790 |
| neural-048 | fixed | `store::tests::record_advice_twice_for_the_same_call_keeps_one_row_and_succeeds`, `store::tests::record_advice_that_fails_part_way_writes_nothing`, `integration::read_guard_call_id_tests::read_guard_call_ids_are_distinct_within_one_millisecond` | d5075f9b |
| neural-049 | fixed | `store::tests::load_reads_without_adding_tables_to_a_database_that_has_none`, `store::tests::load_and_the_schema_check_do_not_wait_on_another_writer`, `store::tests::flush_adds_the_tables_a_partial_database_is_missing` | e968432f |
| neural-050 | fixed | `golden::ingesting_events_does_no_database_io_before_stop`, `golden::event_processing_stays_under_five_milliseconds_average` (ignored), `read_guard_ids::the_read_guard_uses_the_current_turn_and_keeps_the_placeholder_without_one` | e94cbc4e |

neural-002 was first reported as needing an owner decision. Review round 1 chose the stricter option: promote reads the bake-off evidence registered with that model version, scores hash-v1 in the same call, and refuses a version without evidence or a caller claim. The `--beats-baseline` and `--regressions-clear` CLI flags and MCP parameters are gone.

neural-003 is fixed in part: admission now compares the lexical challenger with hash-v1 measured on the same fixture, off the runtime thread, and only a computed answer is cached. The synonym table is still written for the fixture's single paraphrase question, so a held-out paraphrase set (new fixture data) and the rule for a challenger that fails it remain an owner decision.

## Review rounds

| Round | Findings | Fixed |
|---|---|---|
| 1 | 6 | 6 |
| 2 | 3 | 3 |
| 3 | 2 | 2 |
| 4 | 3 | 3 |

## Commands run (this run)

| Command | Result |
|---|---|
| `cargo test -p ax-neural` | lib 229 passed, 4 ignored; bench_flags 6 passed, 2 ignored; 0 failed |
| `cargo test -p ax-neural --features neural-runtime` | lib 230 passed, 4 ignored; bench_flags 6 passed, 2 ignored; 0 failed |
| `cargo test -p ax-pi` | lib 59, golden 22 (1 ignored), read_guard_ids 2, tool_pairing 1 passed; 0 failed |
| `cargo test -p ax-pi --test golden -- --ignored` | 1 passed |
| `cargo test -p ax-neural --test bench_flags -- --ignored` | 1 passed, 1 failed (`locomo_recall_affix_against_production`: dataset `benchmarks/memory/datasets/locomo10.json` is not checked in) |
| `cargo clippy -p ax-neural --all-targets -- -D warnings` | pass |
| `cargo clippy -p ax-neural --all-targets --features neural-runtime -- -D warnings` | pass |
| `cargo clippy -p ax-neural --all-targets --features neural-lab -- -D warnings` | pass |
| `cargo clippy -p ax-pi --all-targets -- -D warnings` | pass |
| `cargo fmt --all --check` | pass |
| `cargo check -p ax-cli -p ax-mcp -p ax-web --all-targets` (callers changed by this PR) | pass |
| `cargo test -p ax-mcp --lib neural` | 9 passed |
| `cargo test -p ax-mcp --test neural_preflight` | 7 passed |
| `cargo test -p ax-cli neural` | 10 passed |
| `cargo test -p ax-web neural` | 1 passed |

## Open items

- neural-003: a held-out paraphrase set and the rule for a lexical challenger that fails it need an owner decision.
- neural-029: pinning the ONNX model checksum to a known build needs an owner decision on which build to accept.
- `locomo_recall_affix_against_production` (ignored) needs the LoCoMo dataset, which is not checked in; it fails the same way on the base commit.
- Rows stored under a non-canonical project key are adopted on first use per process; there is no one-off migration in ax-db.
