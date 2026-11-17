# Baseline-preserving reranker

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/BASELINE-PRESERVING-RERANKER.md`  
**Date:** 2026-11-16  
**Code:** `crates/ax-neural/src/recovery.rs` (`select_context`)

Production `rank::plan` does not call this function. `RecoveryConfig::default().enabled` is false. A disabled call returns the baseline ids and records `experimental recovery disabled`.

## When the flag is on

- Mandatory and protected baseline items stay. Optional baseline items may be reordered by neural score inside the optional slots only.
- A neural candidate from the same project may be added only when it has a neural score. If the token sum would exceed the baseline token total (or `token_budget` when set), an unprotected optional item is removed only when its score is strictly lower, and its id is recorded in `displaced` after the add fits.
- A protected candidate from another project aborts the experimental selection and restores the baseline. That fallback does not mean the coding task succeeded.
- A prediction is a separate gate, described in `PREDICTION-CALIBRATION.md`.

## What this run did not change

The live agent benchmark still uses the existing production retriever. This module was not linked into preflight, and `SEMANTIC_WEIGHT` is still 0.

## Unit tests

`CARGO_TARGET_DIR=/tmp/ax-recovery-target cargo test -p ax-neural --lib recovery` finished with **10 passed, 0 failed** after the attribution header. The one-thread `ax-neural` library suite was **160 passed, 0 failed**. One of those tests is the two-question ablation where query-specific scores raise MRR and one global prior does not. The same isolated target earlier ran `query_prior` after the live tree no longer contained the `perf-05` break: **5 passed, 0 failed**, including the locked-family relationship result of 0 moves and MRR 0.714.

An earlier one-thread run, before these two tests, was 155 passed. That same binary with 8 threads failed `abc_report_samples_rss_once_per_run` (expected 1 process RSS sample, observed 2). That test passed alone. The counter is process-global, so a parallel test can increment it. The test was not weakened, and that failure is not a recovery result. The 8-thread suite was not rerun after the benefit gate.

## Relationship expansion

`query_prior::measure_relationship_expansion` builds links only from shared file paths and shared trigger tokens. Question labels are not used. On the three locked-family questions the graph has 2 adjacency entries. The lexical top hit had no neighbor outside the top 3, so nothing moved. Mean reciprocal rank stayed 0.714. The edge rule was not widened after that result.

## Remaining limits

These are in the experimental function only. Production ranking does not call it. The held-out daemon has exited, and `perf-10` no longer has a broken anchor.

`fit_optional` commits a removal only after the token sum fits. An optional item is removed only when its score is strictly below the incoming score. A neural candidate with no neural score is rejected and recorded as `missing neural score`. A prediction whose confidence does not beat the optional item it would replace is rejected and recorded as `expected benefit does not justify displacement`.

The only full fallback is still a project-isolation breach on protected context. A rejected prediction does not undo an earlier rerank of optional slots. Latency is not measured. Callers that have not finished an independent calibration study still pass `calibrated_on_independent_validation: false`, so those proposals are rejected. This gate was not fit on the 30-task held-out run, and it does not show a coding-task improvement.
