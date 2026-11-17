# Evidence

Spec approval: the user said "Implement the plan as specified" on 2026-11-15. That approved the attached plan, including the later phases. A later instruction, "neural moet verplicht, niet optioneel", makes the layer required. `/goal check the plan and fix issues. add what you forgot` asked for the plan audit and a green gauntlet. `docs/specs/memory-hash-only.md` was not edited. The hash-only amendment is only `hash-only-amendment.md`.

Source: git `85fa176a` plus the uncommitted Neural changes. Source-tree hash at gauntlet start and end: `9d23ad18e0ab89e1c86b7dcd6a869475b590018c1ab34bbd4d8b1f71c18cd6fe`. Machine: Darwin arm64, rustc 1.100.0, cargo 1.100.0, cargo-mutants 27.1.0, cargo-llvm-cov 0.9.1, cargo-audit 0.22.2, Node v26.8.2, npm 11.19.1, Python 3.15.1.

Entry point for a fresh run: `bash scripts/neural-gauntlet.sh` (about 30 minutes). Final exit code of that command for this report: **0**.

## Behaviors

Every scenario named in `docs/specs/ax-neural/SPEC.md` maps to a test that exists (`scripts/neural-spec-map.py`: 93 referenced tests). Plan audit findings A1–A13 and R1–R15 are recorded in SPEC. Phase checklist items are checked in `CHECKLIST.md`.

| Behavior | Check |
|---|---|
| Missing `ax.json` keeps Neural required and reports hash-v1 | `config::tests::missing_file_stays_required`, `status::tests::missing_project_status_names_hash_v1` |
| `runtime_built()` matches the empty `neural-runtime` feature | `status::tests::reports_whether_the_empty_neural_runtime_feature_is_on` |
| Invalid JSON and a threshold outside 0..=1 keep Neural on | `invalid_json_stays_required_and_keeps_the_file`, `bad_threshold_stays_required` |
| Other `ax.json` keys stay on disk | `other_keys_are_left_on_disk` |
| Automatic promotion in config is rejected | `automatic_promotion_is_rejected` |
| An unreadable `ax.json` stays required and names the read error | `unreadable_ax_json_stays_required_and_names_the_error` |
| `enabled: false` together with automatic promotion reports both problems | `disabled_request_keeps_the_automatic_promotion_error` |
| Command Center status measures feedback on the open database | `neural::tests::status_measures_feedback_on_the_open_database` |
| Semantic blend weight 0 leaves the locked fusion score unchanged | `semantic_blend_does_not_move_the_locked_fusion_score` |
| hash-v1 and an unadmitted model are not stored | `hash_v1_is_rejected_and_an_unadmitted_model_is_not_stored` |
| Bake-off admits nobody | `coding_fixture_keeps_hash_v1_and_admits_nobody` and `bakeoff-results.json` |
| Hard-required context stays; duplicate optional text is dropped | `hard_required_stays_even_when_it_does_not_match`, `duplicate_optional_is_dropped_and_better_overlap_goes_first` |
| A project with no `ax.json` still ranks and keeps hard-required blocks first | `a_project_without_neural_config_still_ranks_and_keeps_hard_required` |
| Suggestions are `predicted` and do not add graph edges | `shared_neighbor_is_predicted_and_existing_edges_are_not_repeated`, `feedback_is_append_only_and_suggestions_do_not_touch_edges` |
| Feedback success stays `unverified`; secrets and source text are rejected | `success_does_not_mark_context_useful`, `secret_and_source_are_rejected` |
| Promote requires the explicit flag; rollback selects the previous checksum | `promote_requires_the_flag_and_rollback_selects_the_previous_checksum` |
| Predictions under the confidence floor are dropped and labeled `predicted` | `low_confidence_is_dropped_and_the_label_is_predicted` |
| Held-out false positives are counted | `held_out_evaluation_counts_false_positives` |
| Share-off exports nothing; share-on has manifests and counts only | `sharing_off_exports_nothing`, `sharing_on_keeps_manifests_and_counts_only` |
| Schema 45/46 neural tables and rank totals | `schema_45_adds_neural_tables_beside_hash_embeddings`, `migration_v46::*` |
| Memory embeddings stay hash-v1 | `embed::tests::the_only_embedding_model_is_hash_v1` |

## Gauntlet layers (final fresh run)

Command: `bash scripts/neural-gauntlet.sh` — exit **0**, elapsed ~1848 s. Source-tree hash unchanged through the run.

| Layer | Result |
|---|---|
| Spec map | 93 referenced tests exist; control fails a missing test |
| Invariants | 9/9 ok (each with a negative control first) |
| rustfmt | clean on Neural files |
| Supply chain | 0 vulnerabilities in the Neural dependency tree; unsound warnings only for `rustix` / `tokio` already in the tree; control fails a planted vulnerable crate |
| web-ui | 20/20 node tests; `neuralMetrics.ts` 100% lines, branches, functions; tsc clean |
| clippy | 0 warnings in Neural files (default and `--all-features`) |
| Tests | ax-neural 97/97; ax-neural `--all-features` 97/97; ax-db 67/67; ax-mcp 231 pass / 1 baseline fail; ax-web 205/205; ax-cli 170/170; ax-memory 163 pass / 5 baseline fail. No **new** failures vs `scripts/neural-baseline-failures.txt`. Note: three listed baseline tests passed this run (`h1_symbol_name_resolves_to_its_file`, `b12_obsidian_new_note_flow_ends_in_rule`, `commands::costs::tests::run_imports_a_cursor_transcript_before_the_report`) |
| Suite health | Neural tests green at 1, 3, and 16 threads × 3 rounds |
| Coverage | Neural production files at or above floors in `scripts/neural-coverage.py` (e.g. status 56/56, lib 6/6, graph/rank/learning/share 100%); Neural regions of shared files checked |
| Real execution | Built binary in a temp project: init, feedback, predict, preflight, status, export, evaluate, benchmark, share, register, promote (refused without flags / allowed with), rollback, models — every step checked; control fails a stub binary |
| Manual mutation | 42/42 killed (`scripts/neural-mutants.sh`); comment-only control survives and fails the run |
| cargo-mutants | 359 tested: 339 caught, 5 missed, 15 unviable. Missed set equals `scripts/neural-accepted-mutants.txt` exactly (including `runtime_built -> false`; `runtime_built -> true` is now caught by `reports_whether_the_empty_neural_runtime_feature_is_on`) |

## Review rounds

Skills: `old-coder` usable, `old-coder-api` usable, `rust-review` usable, `typescript-review` usable, `javascript-review` usable, `react-review` usable. None missing or empty.

| Round | Findings | Fixed |
|---|---|---|
| 1 | 7 (2 major, 5 minor) | R1-1 web status ignored the open database; R1-2 an unreadable `ax.json` reported no error; R1-3 a disable request hid the automatic-promotion error; R1-4 CLI and web module docs still said optional; R1-5 a status body without `metrics` threw in Settings; R1-6 a failed feedback count was discarded with no comment; R1-7 tests were still named `disabled` |
| 2 | 0 | — |
| 3 (after R15 mutant kill) | 0 | Absolute `runtime_built` assertion only; no production behavior change |

## Bake-off (measured)

Recorded in `docs/specs/ax-neural/bakeoff-results.json`. Winner `hash-v1`, `challengerAdmitted` false. onnx-runtime-cpu and candle-cpu `not_built`. No row written to `neural_embedding`. Fusion weights unchanged. `SEMANTIC_WEIGHT` is 0.

## Skipped layers

- Windows x64 and Linux x64: not run. See `PLATFORM.md`.
- Live browser re-check of Settings after the review fix: blocked earlier when `ax web` did not bind port 7070; covered instead by `status_measures_feedback_on_the_open_database` and the web-ui node suite.
- Second CodeGraph compare-matrix copy under the Recall tree: that path is not on this machine. The ax copy has an ax-only Neural row. There is no CodeGraph counterpart.
- Open decisions from SPEC (real ONNX/Candle runtime, replication Dataset neural artifact, promote that re-runs the bake-off): not built; listed as open, not as forgotten plan acceptance.

## Limits

- `promote` trusts the caller's `beatsBaseline` and `regressionsClear` booleans once `explicit` is true. It does not re-score the fixture.
- GNN code is not in the binary.
- Query latency and RAM for ONNX and Candle were not measured because those runtimes were not built.
- Directive capture for "neural moet verplicht, niet optioneel" waits on explicit user confirmation before `ax_policy_capture`; behavior is already required in code and config.

## Requirement audit (goal)

| Requirement | Evidence | Status |
|---|---|---|
| Plan phases 0–8 acceptance | `CHECKLIST.md` all checked; SPEC scenarios + gauntlet green | proven |
| A1–A13 audit findings fixed | SPEC audit table + mapped tests exist | proven |
| Gauntlet green | `scripts/neural-gauntlet.sh` exit 0; hash `9d23ad18…` | proven |
| No ML runtime in default binary | invariants + clippy all-features | proven |
| hash-v1 only memory embed; no second model | bake-off + memory embed test | proven |
| Neural required (not optional) | config/status/preflight tests | proven |
| Feedback/share/prediction/auto-promotion defaults | learning/share/predict/config tests; auto-promotion refused | proven |
| EVIDENCE refreshed from final run | this file | proven |
| Forgotten work (R15 true-mutant) | new status test; `true` mutant caught; accepted list still 5 | proven |

## Master plan audit (2026-11-16)

Full section-by-section matrix: `docs/specs/ax-neural/PLAN-AUDIT.md`.

Spot checks this audit: `missing_file_stays_required`, `reports_whether_the_empty_neural_runtime_feature_is_on`, `coding_fixture_keeps_hash_v1` — all pass; `bakeoff-results.json` winner `hash-v1`, challengers `not_built`; no `candle`/`burn`/`ort`/`tract`/`onnx` package in `cargo tree -p ax-cli`.

| Master-plan claim | Result |
|---|---|
| Adapted incremental build (CHECKLIST 0–8) | proven met (with required + bake-off deltas) |
| §20 production-ready end state | proven **not** met — open decisions listed in PLAN-AUDIT / SPEC |
| Forgotten in-scope production code | none found; audit artifact added |
