# EVIDENCE: finish the brain-scale memory plan

Spec: `docs/specs/brain-memory-finish.md`. Tier 2.

- Spec approval: the plan was approved ("Implement the plan as specified"). Revisions 1 to 3 of the SPEC (items 14 to 24) were found during the work: spec approval not obtained (autonomous run).
- Source state: `20ae5ee71a4ad5a2b94b31fcdff1a224b4736bb8` (this file is added in the next commit; it changes no code).
- Tools: rustc 1.100.0, cargo 1.100.0, clippy 0.1.100, rustfmt 1.11.0, cargo-llvm-cov 0.9.1, cargo-mutants 27.1.0.
- Entry point: `scripts/brain-memory-gauntlet.sh --bench --tool-mutants` (exit 0, one fresh run after the last code edit).

## Behavior to test

| Item | Verified by |
|---|---|
| 1 demoted turn by id and cue on every surface | `memory_parity::a_demoted_turn_is_fetchable_by_id_and_cue_on_every_door`, `brain::a_91_day_turn_leaves_hot_recall_and_stays_fetchable` |
| 2 archive recall: same ids, order, scores | `memory_parity::archive_recall_returns_the_same_ids_and_scores_on_every_door` |
| 3 archive history: same graph changes | `memory_parity::archive_history_lists_the_same_graph_changes_on_every_door` |
| 4 confirm on every surface | `memory_parity::confirm_activates_a_candidate_from_every_door` |
| 5 preflight lists the cue hit first | `memory_parity::preflight_lists_the_cue_hit_first_on_cli_and_mcp`, `brain::a_hyphenated_cue_in_the_prompt_comes_first` |
| 6, 23 one sentence constant per form | `ide_seed::MEMORY_PROCEDURE_SENTENCE`, `cli_only::MEMORY_PROCEDURE_CLI_SENTENCE`; the tests below compare against them |
| 7 MCP sentence on every seeded surface | `ide_seed::tests::every_seeded_surface_teaches_the_memory_procedure` |
| 8, 22 CLI sentence in all ten templates, no MCP config | `cli_only::tests::every_cli_only_template_teaches_the_cli_memory_procedure`, `continue_cli_only_never_writes_mcp_server`, `a_fresh_cli_only_install_verifies_clean` |
| 9 verify names a lost procedure | `ide_seed::tests::verify_names_a_managed_block_that_lost_the_memory_procedure`, `verify_names_a_dedicated_rule_that_lost_the_memory_procedure`, `verify_names_a_drifted_cursor_or_continue_rule_without_the_memory_procedure`; `cli_only::tests::verify_names_a_cli_only_file_that_lost_the_memory_procedure`, `only_the_managed_block_counts_for_the_procedure`, `reversed_markers_fall_back_to_the_whole_file`, `verify_ignores_mcp_mode_files_at_cli_only_paths` |
| 10 user text outside markers kept | `ide_seed::tests::verify_names_a_managed_block_that_lost_the_memory_procedure` (after sync), `cli_only::tests::reinstall_restores_the_procedure_and_keeps_user_text`; real execution below |
| 11 recurring symbol change records again | `brain::a_symbol_that_changes_back_and_forth_keeps_recording`, `a_recurring_change_keeps_the_first_id_and_suffixes_the_rest` |
| 12 100k-row performance | `brain_scale` (ignored; run by `--bench`), numbers below |
| 13 screenshots | `site/public/screenshots/cc-memory-vault{,-dark,-dark-mobile}.png`, `cc-memory-archive{,-dark}.png` (captured by `site/scripts/capture-screenshots.mjs`) |
| 14 each archive memory maps to its own vector row | `brain::archive_vector_points_at_its_own_memory`, `schema_29_rebuilds_a_misdirected_archive_index`, `reindexing_an_archive_memory_keeps_one_vector_row` |
| 15 cosine floor on archive hits | `brain::archive_search_does_not_return_unrelated_rows`, `archive_knn_matches_brute_force_and_skips_other_models` |
| 16 archive ranks without age decay | `brain::archive_word_match_outranks_trigram_lookalikes`, `memory_parity::archive_recall_returns_the_same_ids_and_scores_on_every_door` |
| 17 trigger before title, tag, file, symbol | `brain::a_trigger_that_is_also_a_title_word_stays_a_trigger`, `a_cue_from_two_sources_keeps_the_stronger_source` |
| 18 archive hits resolve through an index | `brain::archive_hits_resolve_through_an_index` (query plan), benchmark |
| 19 concurrent rankings, fusion unchanged | `brain::full_text_finds_a_rare_word_the_vector_misses`, `a_wide_recall_returns_every_match_up_to_the_limit`, the parity score test |
| 20 snippet lifecycle inside markers | `cli_only::tests::reinstalling_marked_snippets_is_idempotent` |
| 21 `ax doctor --mode cli` check | real execution only (below) |
| 24 extra setup files | present: `crates/ax-memory/tests/brain_scale.rs`, `scripts/changed-line-coverage.py`, `scripts/brain-memory-accepted-mutants.txt` |

Must not change:

| Clause | Verified by |
|---|---|
| hot recall, cue lookup, preflight skip the archive | `brain::a_91_day_turn_leaves_hot_recall_and_stays_fetchable`, `git_and_folder_docs_are_archive_and_stay_out_of_preflight` |
| demotion does not delete | `memory_parity::a_demoted_turn_is_fetchable_by_id_and_cue_on_every_door` |
| no new MCP or CLI flag, same field names | the parity suite drives both surfaces with existing flags; `git diff d0dbe830^ HEAD -- crates/ax-mcp crates/ax-cli/src/cli.rs` adds no argument |
| text outside markers never rewritten | item 10 tests |
| existing tests stay green | full suite below |

## Gauntlet (run on `20ae5ee7`)

| Layer | Command | Result |
|---|---|---|
| Format | `rustfmt --check` on the changed files | clean |
| Lint | `cargo clippy -p ax-memory -p ax-policy -p ax-installer -p ax-db -p ax-cli --all-targets -- -D warnings` | 0 warnings |
| Tests | ax-memory, ax-policy, ax-installer, ax-db, ax-cli `memory_parity` | 497 passed, 0 failed, 1 ignored (the benchmark) |
| Suite health | same suites with `--test-threads=1`, then the brain tests again | all green |
| Changed-line coverage | `cargo llvm-cov` + `scripts/changed-line-coverage.py d0dbe830^ --fail-under 99` | 99.7% (395/396); cli_only 181/181, brain 62/62, ide_seed 94/94, store 58/59 |
| Benchmark | `cargo test -p ax-memory --release --test brain_scale -- --ignored` | 100,000 rows seeded in 12.5 s; cue lookup median 53.4 µs (budget 5 ms); archive top-20 median 39.5 ms (budget 50 ms) |
| Manual mutants | M1 to M11 in the gauntlet script | 11/11 killed (each by a named failing test; the script rejects build-only failures) |
| cargo-mutants | `cargo mutants --in-diff` over the changed library source | 67 mutants: 57 caught, 6 unviable, 4 missed, all on the accepted list |
| Real execution | see below | as expected |
| Supply chain | no dependency added | `Cargo.lock` changes only the ax-cli and ax-web versions |

The 1e6-row target is not claimed: it was not measured.

### Accepted survivors (`scripts/brain-memory-accepted-mutants.txt`)

- `cli_only.rs:60` `<` to `<=`: equivalent, because two distinct markers never start at the same index.
- `brain.rs:626` and `store.rs:406` `>` to `>=`: equivalent except at a cosine of exactly 0.1.
- `store.rs:424` overfetch `*` to `+`: a tuning constant. `limit + 4` still fetches at least 20 rows per ranking, and recall depth has no exact expected output. The `/` variant is killed by `a_wide_recall_returns_every_match_up_to_the_limit`.

The first `cargo-mutants` run missed 16. The new tests in `78c91b2f` and `20ae5ee7` kill the other 12. Those tests passed on first run because the behavior already existed. The `cargo-mutants` rerun is the proof that they can fail. For the file-over-symbol cue case, the first version of the test still let the mutant survive, because sorting is stable. The symbol was moved into an earlier file and the mutant was then applied by hand to watch the test fail (`left: Some("symbol")`, `right: Some("file")`).

### Negative controls

- `scripts/brain-memory-gauntlet.sh --control`: a comment-only mutant in `embed.rs` is reported SURVIVED and the script exits 1.
- `scripts/changed-line-coverage.py`: an empty lcov, a changed file missing from the report, and a threshold above the actual coverage each exit 1.
- Accepted-survivor comparison: against the first run's `missed.txt` it prints the unlisted `delete match arm "file" in cue_source_rank`.

### Real execution

- `ax install --mode cli-only` into a temp project writes no MCP config. With the procedure removed from `GEMINI.md`, `ax doctor --mode cli` reports `cli_memory_procedure` degraded, naming `GEMINI.md`, with repair `ax install --mode cli-only`.
- `ax init`, then remove the paragraph from `AGENTS.md`: `ax policy sync` fails with `missing memory procedure`. `ax policy sync --fix` restores it and keeps the user text outside the markers.
- `ax web` on the rebuilt binary serves `index-B9fjKjYq.js`, the bundle named in `dist/index.html`.

## Known limits

- `store.rs:428`, the error branch of `try_join!` in `recall_scoped`, is not run by any test.
- `doctor.rs` is covered by real execution only, not by the coverage gate.
- Overfetch `+` survives, as described above.

## Pre-existing, not changed

- `--features onnx` does not build: `ort` 2.0.0-rc.15 changed `Session::builder` to take an `&Environment`. Release CI does not build onnx.
- A fresh `ax init` fails `ax policy sync` on `.agents/rules/prefer-mcp-ops.mdc` ("missing ax_preflight; missing once-per-turn dedup").
- rustfmt drift across ax-installer outside the changed files; the format layer checks changed files only.
