# Memory benchmark evidence

Spec: `docs/specs/memory-bench.md` (10 behaviors).
Spec approval: not obtained as a separate review. The deterministic-memory plan was accepted with "go", and this turn continued that plan.
Tier: 2.
Isolation: none. The runner uses a temporary database.
Source: git `85fa176a9b4b6ce5e33687bee268767d87731c60` plus uncommitted files. `dirty: true` in the results JSON. This is not a clean-main baseline.
Toolchain: rustc 1.100.0, cargo 1.100.0. No new packages.

## Behavior map

| Spec | Check |
|---|---|
| 1 recall@k | `bench::tests::recall_at_counts_hits_inside_k` |
| 2 NDCG and abstention | `bench::tests::ndcg_rewards_the_top_rank_and_abstention` |
| 3 BM25, omissions, id tie-break | `bench::tests::bm25_omits_a_miss_and_breaks_ties_by_id` |
| 4 same query, same rank | same BM25 test calls the function twice |
| 5 sixteen families | `coding_fixture_covers_every_family_and_is_deterministic` |
| 6 temp database, stable scores, cue wiring, block_bytes | same test; delivery `block_bytes` is 854 in the results file |
| 7 backup-only is not inserted | same test |
| 8 public shapes, samples are not the datasets | `public_shape_samples_are_parsed_by_the_unit_fixtures` and `shape_samples_are_not_reported_as_the_public_sets` |
| 9 script writes JSON; default test does not; missing dataset is skipped | `a_missing_dataset_is_skipped`; default `memory_bench` left `write_coding_baseline` ignored; `./scripts/memory-bench.sh` wrote the results file |
| 10 commit name and `mechanisms: not_in_this_slice` | results JSON |

Must not open the live vault: the only database the runner opens is `tempdir()/ax.db`. No test points at `.ax/ax.db`.

## Gauntlet

Command: `cargo test -p ax-memory --offline -- --test-threads=8`

Result: 117 passed, 0 failed, 2 ignored. The ignored tests are `cue_and_archive_search_stay_inside_budget_at_100k_rows` (crate marks it release-only) and `write_coding_baseline` (writes the results file only from the script).

`cargo clippy -p ax-memory --all-targets --offline -- -D warnings` exited 0. The workspace still warns that `gix` and `self_update` are unused. That warning is outside this slice.

`rustfmt --edition 2021 --check` on `bench.rs`, `lib.rs`, `brain.rs`, and `tests/memory_bench.rs` printed `FMT_OK`.

`./scripts/memory-bench.sh` printed `wrote /Users/gary/io/ax/benchmarks/memory/results/85fa176a9b4b6ce5e33687bee268767d87731c60.json`. Public datasets: `skipped`. Shape samples: recall@5 1.0, source `shape-sample`.

## Mutants

Each change was restored before the suite above.

| Mutant | Result |
|---|---|
| `recall_at` returns `Some(1.0)` | `recall_at_counts_hits_inside_k` failed: left `Some(1.0)`, right `Some(0.0)` |
| every fixture row is inserted | coding test failed: `a backup-only memory is ground truth and is not written` |
| cue channel returns no rows | coding test failed: left `None`, right `Some("hash-vectors")` |

## Skipped

- Coverage with a fail-under gate: not run. There is no llvm-cov threshold wired for this slice.
- Property tests: not added. The BM25 checks are examples.
- Mutation tool: none in the repo. Three hand mutants above.
- Workspace suite outside `ax-memory`: not run. `brain.rs` changed one clippy borrow (`bind(model)`), and the ax-memory suite includes the graph tests (32 passed, including `graph_capture_rolls_back_as_one_transaction`).
- Real LongMemEval-S and LoCoMo: files were not present and were not downloaded.
- Mechanism probes: not in this slice.
- Independent verification: not performed.

## Baseline this file records

Coding recall@5 on the fixture: named trigger 1, decision why 1, Dutch prompt 0, paraphrase 0, knowledge update 1 (latest first), stale capture returns the stale row (NDCG 0), rename 0, as-of 1 but the newer row is also returned, abstention is empty, delivery 1 inside a **225-byte** preflight block (refreshed this session in `docs/specs/bench-out/coding-baseline.json`), boilerplate and foreign notes are returned, multi-hop 0, both archive questions 1, backup-only 0, file path/symbol/keyword 1.

Public sets: LongMemEval-S and LoCoMo remain `skipped` (no local paths; harness does not download). Shape samples score recall@5 = 1.0 each and are not the public-set delta.

These numbers describe the current code. They are not a quality bar.

## Public sets (this session)

Re-ran `./scripts/memory-bench.sh` after the live-delivery ranking fixes.

| Dataset | Status | Notes |
|---|---|---|
| LongMemEval-S | skipped | `AX_LONGMEMEVAL_PATH` unset; harness must not download |
| LoCoMo | skipped | `AX_LOCOMO_PATH` unset |
| longmemeval_sample / locomo_sample | scored | recall@5 = 1.0, `source: shape-sample` — not the public-set delta |

Result file: `benchmarks/memory/results/85fa176a9b4b6ce5e33687bee268767d87731c60.json` (`public.longmemeval.status=skipped`, `public.locomo.status=skipped`).

Spotlight/home cache search found only the bundled shape samples. The +3pt vs BM25 public-set gate remains open.

## Public-set delta (locked ranking) — 2026-11-15

Datasets are local under `benchmarks/memory/datasets/` (gitignored). The harness never downloads.

| Set | Questions | ax recall@5 | BM25 recall@5 | Δ recall@5 | Gate (≥+0.03) |
|---|---|---|---|---|---|
| LongMemEval-S | 500 | 0.9416 | 0.9081 | **+0.0335** | **PASS** |
| LoCoMo | 1982 | 0.5226 | 0.4407 | **+0.0818** | **PASS** |

Locked ax ranking (`crates/ax-memory/src/bench.rs::ax_text_rank`):

- Dual stem + raw BM25 mix (stem weight **0.60**), k1=1.8, b=0.2, coverage=1.0, stopword filter, lexicon on stem path
- Soft sibling boost after mix: w=0.04, cap=0.35, rare-term df ≤ 10% of collection, seeds=2

Artifact: `docs/specs/bench-out/coding-baseline.json` (commit `85fa176a`, dirty=True).
