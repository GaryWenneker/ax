# Ax Neural — executable spec

Approval. The plan was approved with "Implement the plan as specified" on 2026-11-15. "neural moet verplicht, niet optioneel" made the layer required; that wins over the plan where the plan says off by default. "check the plan and fix issues. add what you forgot" asked for this audit. Every item below comes from the approved plan. Items that need a new decision are listed under Open decisions and are not built.

Tier 2. The preflight hot path changes, so the failure model below applies.

Isolation: none. The Neural change is uncommitted in this working tree and no commit was authorized. A worktree would not contain it.

## Audit findings

| # | Finding | Plan text | Fix |
|---|---|---|---|
| A1 | `evaluate` scored zero predictions and still reported `kind: measured`, `heldOut: true` | Phase 6: score on held-out tasks from the coding fixture, report false positives | Leave-one-out evaluation over the fixture, CLI and MCP |
| A2 | The bake-off baseline was raw hash cosine. The real pipeline abstains on the abstention question; the report said it did not | ADR 002: the challenger must beat hash-v1 plus the locked fusion | Baseline runs the memory bench pipeline on a temporary database |
| A3 | `ax neural graph` always sent an empty edge list. MCP needed the caller to send edges. The pair loop was quadratic in nodes | Phase 4: petgraph and existing edges for frequency, co-occurrence, and neighbor overlap | Read project edges, petgraph, hub cap, feedback co-occurrence |
| A4 | `neuralRank` was added after the budget pass. Lean replies dropped it. Full replies did not count it | Phase 3: a reason string in the existing budget metadata | `contextBudget.neural`, counted by the budget |
| A5 | No predictions reached the preflight | Phase 6: label `predicted` in the inject text | Optional block `neural:predicted` when prediction is on |
| A6 | No token delta or ranking time anywhere | Phase 3 and Phase 8: recorded timing, token delta in Command Center | Per-project rank totals, schema 46 |
| A7 | No test compared a preflight without Neural config to one with `enabled: false` | Phase 1 acceptance | Integration test |
| A8 | CLI and MCP took different fields; `register` and `evaluate` had no MCP tool; the status tool description still said disabled | Same change, same field names | Parity fixes |
| A9 | The share packet always said `local` and ignored `distribution.enabled` | Phase 7 | Configured mode, two keys |
| A10 | No training export, so "secret-shaped text excluded from training exports" had nothing to check | Phase 5 and Phase 8 security pass | CLI-only export that skips secret-shaped rows |
| A11 | Memory ids in feedback were predicted as files | Phase 6: skills, rules, files, and memories | `memory:` context ids predict kind `memory` |
| A12 | CLI used the raw `--path` as project id; MCP used the resolved root | Project isolation | CLI canonicalizes the root |
| A13 | No per-phase spec, no persisted gauntlet or mutation script | Each phase gets an executable spec; evidence must be reproducible | This file, `scripts/neural-gauntlet.sh`, `scripts/neural-mutants.sh` |

## Scenarios

### Phase 1 — required layer

| Scenario | Test |
|---|---|
| No `ax.json`: enabled, ranking on, no config error | `config::tests::missing_file_stays_required` |
| `neural.enabled: false`: still on, error `neural cannot be disabled`, file unchanged | `config::tests::other_keys_are_left_on_disk` |
| A preflight with no `ax.json` and a preflight with `neural.enabled: false` have the same `inject` and `contextBudget` | `neural_preflight::absent_and_disabled_preflights_match` |
| The status tool description says required, not disabled | `tools::tests::neural_tool_descriptions_say_required` |

### Phase 2 — bake-off

| Scenario | Test |
|---|---|
| The baseline is hash-v1 with the locked fusion, scored by `ax_memory::bench::score_coding` on a temporary database. On the coding fixture: named_trigger 1/1, abstention 1/1, delivery_budget 1/1, paraphrase recall@3 0/1 | `bakeoff::tests::baseline_uses_the_fused_pipeline` |
| Query embedding latency is recorded as measured. RSS is recorded as not measured, with the reason | `bakeoff::tests::baseline_latency_is_measured` |
| The bake-off database is a temporary file. `.ax/ax.db` is not opened | `bakeoff::tests::baseline_uses_the_fused_pipeline` asserts `openedLiveDatabase: false` and a temp path |
| Challengers are not built. Winner hash-v1. Nobody admitted | `bakeoff::tests::coding_fixture_keeps_hash_v1_and_admits_nobody` |

### Phase 3 — ranking

| Scenario | Test |
|---|---|
| `contextBudget.neural` gives the reason for kept optional blocks (`ranked`), for `contextBudget.omitted` (`over-budget`), lists duplicate ids, and their token count | `context_budget::tests::neural_reasons_are_counted_in_the_budget` |
| `contextBudget.tokens` still equals the measured projection with `neural` present | same test |
| A duplicate optional block is dropped and reported with its tokens | `neural_tool::tests::duplicate_block_is_reported_with_its_tokens` |
| Hard-required blocks stay first | `neural_tool::tests::a_project_without_neural_config_still_ranks_and_keeps_hard_required` |
| The preflight reply has `contextBudget.neural` and no top-level `neuralRank` | `neural_preflight::absent_and_disabled_preflights_match` |
| Each preflight adds to the project's rank totals: preflights, duplicate blocks, duplicate tokens, ranking microseconds | `store::tests::rank_totals_accumulate_per_project` |

### Phase 4 — graph suggestions

| Scenario | Test |
|---|---|
| With graph suggestions on and no edges passed, suggestions come from the project `edges` table. No edge row is written | `dispatch::tests::graph_reads_project_edges_and_writes_no_edges` |
| Nodes with more than 64 neighbors are not used as a shared neighbor. The reply counts them | `graph::tests::hub_nodes_are_skipped` |
| Score is neighbor overlap (Jaccard) between 0 and 1. A pair that already has an edge is not suggested | `graph::tests::shared_neighbor_is_predicted_and_existing_edges_are_not_repeated`, `graph::tests::overlap_is_bounded` |
| Two ids that appear together in two or more successful feedback rows are suggested with reason `co-occurrence` | `graph::tests::feedback_co_occurrence_is_suggested` |
| A second run replaces the project's earlier suggestion rows | `store::tests::rerun_replaces_previous_suggestions` |

### Phase 5 — learning

| Scenario | Test |
|---|---|
| Success stays `unverified`; secrets and source text are rejected | existing `learning` tests |
| Promotion needs `explicit`; rollback selects the previous checksum | existing `store` test |
| The training export skips secret-shaped rows and has no project path | `store::tests::training_export_skips_secret_rows_and_project_paths` |
| `export` is not reachable through `ax_neural_*` | `dispatch::tests::export_is_not_an_mcp_command` |
| CLI feedback arguments use the MCP field names | `commands::neural::tests::feedback_args_use_mcp_field_names` |

### Phase 6 — predictions

| Scenario | Test |
|---|---|
| A `memory:` context id is predicted with kind `memory` | `predict::tests::memory_context_ids_are_predicted_as_memories` |
| Leave-one-out evaluation over the coding fixture reports questions, predictions, true and false positives, precision, and coverage, as measured | `predict::tests::held_out_evaluation_scores_the_coding_fixture` |
| The preflight has a `neural:predicted` block only when `prediction.enabled` is true and feedback predicts something | `neural_preflight::predicted_block_appears_only_when_prediction_is_enabled` |
| Status shows the last evaluation precision as measured, and `unavailable` before any run | `dispatch::tests::status_reports_measured_metrics` |

### Phase 7 — sharing

| Scenario | Test |
|---|---|
| The packet reports the configured mode and `sent: false` | `share::tests::packet_reports_the_configured_mode` |
| Model manifests only with `distribution.enabled`; outcome counts only with `share_learning_data` | `share::tests::models_and_aggregates_follow_their_own_keys` |
| Share off exports nothing | existing `share::tests::sharing_off_exports_nothing` |

### Phase 8 — status and Command Center

| Scenario | Test |
|---|---|
| With a database, status reports feedback count, token delta, rank latency, prediction precision, and promoted models, each with a kind | `dispatch::tests::status_reports_measured_metrics` |
| Settings shows those values with their kind | `npm run build` plus a browser check |

## Must not

| Constraint | Check |
|---|---|
| Hash-v1 stays the only memory embedding | `embed::tests::the_only_embedding_model_is_hash_v1` |
| Fusion weights unchanged; semantic weight 0 | `semantic_blend_does_not_move_the_locked_fusion_score` |
| No ML runtime crate in the default binary | `scripts/neural-gauntlet.sh` fails if `cargo tree -p ax-cli` names candle, burn, ort, tract, or onnx |
| No `calls` or `references` edge is written | graph tests count `edges` before and after |
| The bake-off does not open `.ax/ax.db` | bake-off test |
| Automatic promotion cannot be turned on | config tests |
| Existing `contextBudget` fields keep their meaning | `crates/ax-mcp/tests/context_budget_contract.rs` |
| No new failing test in the touched crates | baseline: `server::policy_integration::unchanged_catalog_and_memory_titles_are_sent_once_per_session` already failed |

## Failure model for the preflight changes

| Failure | Guard |
|---|---|
| Ranking adds latency | Ranking time is recorded per project and shown in status |
| Reasons push useful blocks out | `contextBudget.neural` is a few fixed keys plus duplicate ids; it is counted, so the budget stays true |
| Rank totals write fails (read-only database) | The write runs after the reply is built and its error is logged; the reply is unchanged |
| Prediction query fails | The predicted block is skipped and a warning is logged; required blocks are unaffected |

## Setup

No new external crate. `ax-neural` adds `petgraph` (already used by `ax-graph`) for the neighbor graph and moves `tempfile` (already used by `ax-db`) from dev to normal dependencies for the bake-off database. New files: `crates/ax-neural/schema_rank.sql`, `crates/ax-db/tests/migration_v46.rs`, `crates/ax-mcp/tests/neural_preflight.rs`, `scripts/neural-gauntlet.sh`, `scripts/neural-mutants.sh`. No commits.

## Revisions during implementation

Added while building, in the order found. Nothing above was removed.

- R1. Preflight mints a new chat id on each call without `session`, so two preflights differed only in that id. `neural_preflight` passes a fixed `session`. No assertion changed.
- R2. Bug: `predict` with an empty task matched every category, so an empty prompt would get predictions from all feedback. Fixed.
- R3. Changed: the rank totals write is awaited after the reply is built, capped at 250 ms, instead of running as a background task. `ax preflight` and `ax turn begin` end their runtime as soon as preflight returns, and the background write was lost in 3 of 3 runs.
- R4. Graph: passed edges give a preview that stores nothing. Without edges, the project's edges are read and the suggestions stored.
- R5. A directory is an ax root when `.ax/ax.db` exists. That is the rule `find_nearest_ax_root` uses for MCP, and now for the CLI.
- R6. Command Center formatting lives in `crates/ax-web/web-ui/src/components/neuralMetrics.ts`, tested by `neuralMetrics.test.ts` under `npm test`. The panel also shows whether semantic search is active.
- R7. Schema version assertions in the migration tests for v17, v18, v19, v20, v23, and v45 moved from 45 to 46, as with each earlier migration. Only the number changed.
- R8. Removed `predict::false_positive_rate` and its only test, `held_out_false_positives_are_counted`. Nothing called it. `evaluate` computes the false-positive rate itself, and `held_out_evaluation_counts_false_positives` and `held_out_evaluation_scores_the_coding_fixture` check that rate at 1.0 and 0.0.
- R9. So the boundaries can be tested: `Graph::neighbor_overlap` takes the pair cap as a parameter (production passes 500,000), and the bake-off median and its millisecond and microsecond conversions are their own functions. `store::register_model` and its eight positional arguments became `store::register` with a `ModelRecord`, so two text fields cannot be swapped by position. No behavior changed.
- R10. Gauntlet files beyond the setup list: `scripts/neural-spec-map.py` checks that every test named in this file exists, `scripts/neural-coverage.py` holds the coverage floors, and `scripts/neural-accepted-mutants.txt` lists the classified `cargo-mutants` survivors.
- R11. More gauntlet files: `scripts/neural-invariants.py` runs the Must-not gates, each against a known-bad input first; `scripts/neural-test-verdict.py` judges each crate's test run; `scripts/neural-real-run.py` runs the built binary in a temporary project. `scripts/neural-baseline-failures.txt` lists the failures that exist without the Neural change. Besides the `ax-mcp` failure in Must not, it has five `ax-memory` failures and one `ax-web` failure from the uncommitted memory full-text work, and one `ax-memory` test that fails now and then under load. The "No new failing test" row is checked against that file.
- R12. Bug in `scripts/neural-spec-map.py`: it split table rows on every `|`, so a cell with an escaped pipe (`\|`) moved the test name out of the last cell, and the name was skipped without an error. A row added during this audit was not counted. Cells now end only at an unescaped pipe. The gauntlet's control row has an escaped pipe after the missing test name; the old script passed it and the fixed one fails it.
- R13. Bug: `status::tests::missing_project_status_names_hash_v1` asserted `runtimeBuilt: false` always. With `--all-features` the empty `neural-runtime` feature makes `runtime_built()` true and the gauntlet failed. The test now asserts `runtimeBuilt` equals `runtime_built()`, and still asserts `mlRuntimeLinked: false`.
- R14. The gauntlet saw a new `ax-cli` failure in `commands::costs::tests::run_imports_a_cursor_transcript_before_the_report`. It is not a Neural file. It is listed in `scripts/neural-baseline-failures.txt` with the dirty ax-usage work as the cause.
- R15. Bug: after R13, `cargo-mutants` still missed `replace runtime_built -> bool with true` because status only asserted `runtimeBuilt == runtime_built()` (equality to itself). Added `status::tests::reports_whether_the_empty_neural_runtime_feature_is_on`, which asserts the absolute `cfg!(feature = "neural-runtime")` value. The accepted equivalent remains `replace … with false` under default features.

### Scenarios added

| Scenario | Test |
|---|---|
| Evaluation counts false positives and their rate | `predict::tests::held_out_evaluation_counts_false_positives` (passed on its first run; a throwaway mutant that counted every memory prediction as a hit made it fail) |
| A minimum confidence outside 0 to 1 is rejected; the minimum is inclusive | `predict::tests::held_out_evaluation_keeps_the_minimum_inclusive_and_rejects_bad_minimums` |
| An empty task predicts nothing | `predict::tests::an_empty_task_predicts_nothing` |
| A pair seen together in one feedback row only is not suggested | `graph::tests::a_pair_seen_once_is_not_suggested` |
| A node with exactly 64 neighbors is still used | `graph::tests::a_node_at_the_degree_limit_is_used` |
| Passed edges preview and store nothing | `dispatch::tests::passed_edges_are_a_preview_that_stores_nothing` (passed on its first run; a throwaway mutant that stored the preview made it fail) |
| An evaluation is stored and read back | `store::tests::evaluation_is_recorded_and_read_back`, `dispatch::tests::evaluate_runs_the_held_out_fixture` |
| Without rank data, token delta and rank latency are unavailable | `dispatch::tests::status_without_rank_data_is_unavailable` |
| Share follows `distribution.enabled` and `share_learning_data` through dispatch | `dispatch::tests::share_follows_the_distribution_and_share_learning_keys` |
| Semantic stays inactive until a challenger is admitted | `status::tests::semantic_is_inactive_until_a_challenger_is_admitted` |
| Status names hash-v1; `runtimeBuilt` follows the feature; `mlRuntimeLinked` stays false | `status::tests::missing_project_status_names_hash_v1` |
| `runtime_built()` matches the empty `neural-runtime` feature for this build | `status::tests::reports_whether_the_empty_neural_runtime_feature_is_on` (kills `replace runtime_built -> bool with true`; `with false` stays accepted under default features) |
| Preflight records the totals before it returns, so a CLI process keeps them | `neural_preflight::rank_totals_are_written_before_preflight_returns`, `neural_preflight::each_preflight_adds_to_the_rank_totals` |
| `ax_neural_register` and `ax_neural_evaluate` exist; register and feedback take the CLI field names | `tools::tests::neural_register_and_evaluate_are_tools_with_cli_field_names` |
| CLI register, rank, and graph flags take the MCP fields; bad JSON is an error | `commands::neural::tests::register_args_use_mcp_field_names`, `commands::neural::tests::rank_and_graph_take_json_arrays` |
| The CLI root is the nearest canonical directory with `.ax/ax.db`, else the canonical path | `commands::neural::tests::project_root_walks_up_to_the_canonical_ax_root`, `commands::neural::tests::project_root_without_ax_is_the_canonical_path` |
| `ax neural export` writes rows without the project path, and refuses without a database | `commands::neural::tests::export_writes_feedback_without_the_project_path`, `commands::neural::tests::export_without_a_database_is_an_error` |
| A version 45 database gains `neural_rank_total` on open | `migration_v46::a_version_45_database_gains_rank_totals_on_open`, `migration_v46::schema_46_adds_neural_rank_totals` |
| Settings text for token delta, rank latency, precision, and promoted models | `neuralMetrics.test.ts`, 6 tests (4 at first; the 2 below were added later) |
| Settings text reads a missing count as 0 and a missing source as "unknown source"; a checksum of 12 characters or fewer is shown whole; models are separated by a comma | `neuralMetrics.test.ts`: "reads a missing count as 0 and a missing source as unknown", "shows a checksum of 12 characters or fewer whole and separates models with a comma" (found by branch coverage: 5 branches were never taken; passed on their first run; the four `ts-*-default` mutants, `ts-checksum-boundary`, and `ts-models-separator` make them fail) |
| The predicted block is optional: a tight budget omits it | `neural_preflight::the_budget_can_drop_the_predicted_block` (passed on its first run; the `predicted-hard-required` mutant makes it fail) |
| Ranking time is measured, not reported as zero | `neural_tool::tests::rank_time_is_measured` (passed on its first run; the `rank-time-not-measured` mutant makes it fail) |

Found by the first `cargo-mutants` run: the shared entry point that the CLI and the MCP tools both call had no test for search, rank, predict, explain, feedback, or the model commands, and some boundaries had no test. The tests below all passed on their first run, because they cover behavior that already existed. The final mutation runs in EVIDENCE show each one failing for at least one mutant.

| Scenario | Test |
|---|---|
| The rank tool maps `hard_required`, `high_value`, and a missing class (`optional`), and drops duplicates | `dispatch::tests::rank_tool_maps_class_names_and_drops_duplicates` |
| Feedback through the shared entry point stores every field under the CLI and MCP names | `dispatch::tests::feedback_tool_stores_the_cli_field_names` |
| Feedback needs collection on, a database, and its required fields | `dispatch::tests::feedback_needs_collection_on_a_database_and_the_required_fields` |
| The predict tool is off until `prediction.enabled`, needs a database, and labels each prediction `predicted` | `dispatch::tests::predict_tool_labels_predictions_from_this_projects_feedback` |
| Explain reports hash-v1, semantic weight 0, and the locked fusion weights; search falls back to hash-v1 | `dispatch::tests::explain_and_search_report_hash_v1` |
| Model commands without a database | `dispatch::tests::model_commands_without_a_database` |
| Register stores every tool field; a missing checksum is an error | `dispatch::tests::register_stores_every_tool_field` |
| Promote needs `explicit`; promote and rollback move the promoted checksum | `dispatch::tests::promote_and_rollback_move_the_promoted_checksum` |
| Two projects that share a database keep their own feedback and predictions | `dispatch::tests::projects_sharing_a_database_keep_their_own_feedback` |
| The graph command says when more edges exist than it read | `dispatch::tests::project_edges_say_when_more_edges_exist_than_the_limit` |
| The benchmark command runs on a temporary database | `dispatch::tests::benchmark_runs_the_coding_fixture_on_a_temporary_database` |
| Bake-off timings are in milliseconds and microseconds; the median is the middle sample | `bakeoff::tests::baseline_timings_are_in_their_units`, `bakeoff::tests::durations_convert_to_milliseconds_and_microseconds`, `bakeoff::tests::median_takes_the_middle_sample` |
| Named trigger needs the top result, paraphrase a hit in the top 3, abstention nothing expected and nothing returned | `bakeoff::tests::family_rules_check_the_top_result_the_top_three_and_abstention` |
| Empty ids and self-loops are ignored; a suggested pair names the smaller id first | `graph::tests::empty_ids_and_self_loops_are_ignored_and_pairs_name_the_smaller_id_first` |
| Candidate pairs past the cap are dropped and reported as truncated | `graph::tests::candidate_pairs_past_the_cap_are_truncated` |
| A feedback row of 64 ids is used; a row of 65 is skipped | `graph::tests::a_feedback_row_at_the_degree_limit_is_used_and_a_wider_one_is_not` |
| An id repeated in one feedback row counts once; an empty id never counts | `predict::tests::an_id_repeated_in_one_row_counts_once_and_empty_ids_never` |
| The MCP tool handler routes `ax_neural_*` calls to the shared entry point; `ax_neural_export` is refused there | `neural_preflight::neural_tools_are_answered_through_the_mcp_tool_handler` (found by coverage: the routing lines never ran; the `neural-tools-unrouted` mutant fails it with "unknown tool: ax_neural_feedback") |
| A semantic score of 0 leaves the fused score unchanged; it never turns it into NaN | `dispatch::tests::a_zero_semantic_score_leaves_the_fused_score_unchanged` (found by the whole-crate run; with `/` for `*` the blend returns NaN) |
| A rank totals row with 0 preflights, which the schema allows, leaves token delta and rank latency unavailable instead of dividing by zero | `dispatch::tests::a_rank_row_with_zero_preflights_is_unavailable_and_divides_nothing` (found by the second run; with `>= 0` it panics with "attempt to divide by zero") |
| Each `ax neural` subcommand sends its MCP command and argument names; `export` has no MCP command | `neural_request_tests::each_subcommand_sends_the_mcp_command_and_argument_names` (found by the audit: the CLI mapping had no test; passed on its first run; the six `cli-*` mutants make it fail) |
| Model commands built by the CLI work through the shared entry point: register, promote, model-status, rollback, and models | `neural_request_tests::model_commands_built_by_the_cli_work_through_the_shared_dispatcher` (passed on its first run; `cli-model-status-command`, `cli-promote-flag`, and `cli-rollback-field` make it fail) |

Found by the third whole-crate run, in which 36 of 364 mutants survived. The tests below all passed on their first run because they cover behavior that already existed, except the last one, which failed first and found a bug. The final mutation run in EVIDENCE shows each one failing for its mutants.

| Scenario | Test |
|---|---|
| `ranking.enabled: false` keeps ranking on and reports `neural cannot be disabled` | `config::tests::ranking_turned_off_stays_on_and_says_so` |
| `runtime.fallback_backend` takes `cpu` and `hash`, both stored as `cpu`, with no config error; `gpu` is an error | `config::tests::fallback_backend_takes_cpu_or_hash_as_cpu_and_nothing_else` |
| `prediction.minimum_confidence` 0.6 is used with no config error; 1.5 is refused and 0.75 stays | `config::tests::minimum_confidence_is_set_between_0_and_1_and_refused_outside` |
| Retry count and token counts may be 0; each one negative on its own is refused | `learning::tests::counts_may_be_zero_but_not_negative` |
| Relevance is the share of query words found in the block, words of two letters or fewer are ignored, and relevance counts as measured only when there is a query | `rank::tests::relevance_is_the_share_of_query_words_found_in_the_block`, `rank::tests::words_of_two_letters_or_fewer_are_ignored`, `rank::tests::relevance_is_measured_only_when_there_is_a_query` |
| An admitted challenger's vector is stored and counted; search still answers from hash-v1 and does not scan the memory column | `store::tests::an_admitted_vector_is_stored_and_counted_while_search_stays_on_hash_v1` |
| An audit row records the wall-clock time of its action | `store::tests::an_audit_row_records_the_time_of_its_action` |
| Share sends the outcome counts of this project's feedback only, and no raw feedback | `dispatch::tests::share_counts_the_outcomes_of_this_projects_feedback_only` |
| The graph command suggests context ids that succeeded together in stored feedback; failed rows do not count | `dispatch::tests::graph_suggests_context_ids_that_succeed_together` |
| Versions registered in the same second keep their registration order: the model list shows 1, 2, 10, and rollback from 10 selects 2 | `store::tests::versions_registered_in_one_second_keep_their_registration_order` (failed first: rollback selected 1, because with equal seconds the primary-key index returns versions in text order; fixed with a `rowid` tiebreak) |

Found by the fourth whole-crate run, in which 6 of 359 mutants survived. Five are equivalent and are listed with their reasons in `scripts/neural-accepted-mutants.txt`. The sixth was a gap:

| Scenario | Test |
|---|---|
| A query whose words all have two letters or fewer gives each block relevance 0, still measured, not NaN | `rank::tests::a_query_of_only_short_words_scores_every_block_zero` (passed on its first run; with `&&` for `\|\|` in `overlap` it fails with `left: NaN`) |

Removed as dead code: `rank::apply_budget` with its test, and `learning::feedback_json`. Neither had a caller. The over-budget reason in the reply comes from `contextBudget.neural`, which `context_budget::tests::neural_reasons_are_counted_in_the_budget` covers.

### Failure model, revised

| Failure | Guard |
|---|---|
| Rank totals write fails or the database is busy | Replaces the earlier row. The write is awaited after the reply is built and capped at 250 ms. An error or timeout is logged and the reply is unchanged |

### Known limits found during the work

- A task category matches a prompt by substring, so category `api` matches `rapid`. Not changed.
- The `server::policy_integration` tests run preflights against this repository's live `.ax/ax.db` when it exists. They did before this change. Each of those preflights now also adds to this project's rank totals.
- On this machine, `ice_preflight::preflight_returns_exact_indexed_manifest_line` fails. The global rules in the home directory make the required context 12,404 tokens against the 12,000-token compact limit, so the source block is dropped. With an empty `HOME`, the whole `ax-mcp` suite has only the known baseline failure. `contextBudget.neural` adds about 30 tokens and does not decide it.
- `secret_shaped` matches eleven spellings. See `SECURITY.md`.
- The `applied: false` reply of the rank tool and the early return in `maybe_reorder` cannot run through `load()`, which always turns ranking on. They stay as the plan's fallback to the unchanged budget.
- The model list has no tiebreak for versions registered in the same second. It shows them in registration order because SQLite reads the whole table in `rowid` order; `versions_registered_in_one_second_keep_their_registration_order` would catch a change.
- Rollback selects the previously registered version, as `SECURITY.md` says. That version may never have been promoted. See Open decisions.

## Open decisions (not built)

- A real ONNX Runtime or Candle challenger that **beats** hash-v1 on paraphrase (ADR 002). The pure-Rust CPU backend behind `neural-runtime` (ADR 003) must not be admitted as a semantic challenger. Gauntlet invariants still forbid candle/burn/ort/tract/onnx even under `--all-features`.
- Agent A/B: no-Ax vs Ax+Neural task success (external harness). `scripts/neural-ab.sh` measures C only; B is N/A while Neural is required.
- CUDA / DirectML / Core ML / Vulkan / WebGPU backends.
- A central Neural service that dials out (share packets still `sent: false`; hybrid/central expose a `syncPlan` only).

## Closed from earlier open list

- Promote re-runs the coding bake-off; caller booleans ignored (`promotion_ignores_caller_bakeoff_claims`).
- Rollback selects the previously **promoted** version (`rollback_selects_previous_promoted_not_merely_previous_registered`).
- Optional `Dataset.neural` field (`dataset_neural_field_is_optional`); older peers deserialize without it.
- Linux x86_64 / aarch64 and Windows x86_64 `cargo check` for `ax-neural` and `ax-cli` (see `PLATFORM.md`).
