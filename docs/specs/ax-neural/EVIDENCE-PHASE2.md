# EVIDENCE — Phase 2 A/B/C retrieval benchmark

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/EVIDENCE-PHASE2.md`  
**Spec:** `/Users/gary/io/ax/docs/specs/ax-neural/SPEC-PHASE2-BENCHMARK.md`  
**Spec approval:** not obtained (autonomous goal run)  
**Source state:** working tree after Phase 2; `cargo test -p ax-neural --lib` → **114 passed** (2026-11-16)

## Spec → test map

| Scenario | Evidence |
|---|---|
| Three arms + locked/heldOut metrics | `benchmark_abc::abc_report_measures_three_arms_*`; `dispatch::benchmark_abc_reports_three_arms_*`; `docs/specs/ax-neural/abc-results.json` |
| Production ranking unchanged | `abc_adapter_does_not_require_disabling_production_ranking`; report `productionRankingUnchanged: true` |
| `SEMANTIC_WEIGHT` stays 0 | report + `scripts/neural-abc.sh` assert |
| No neural-inference claim for lexical | `claimsNeuralInference: false` on arm C |
| Empty relevant → omit P/R/MRR | `precision_and_mrr_examples`; abstention samples omitted (`samples.mrr` = 3 for 4 locked questions) |

## Gauntlet

| Layer | Command / result |
|---|---|
| Unit tests | `cargo test -p ax-neural --lib` → 114 passed |
| Real execution | `./scripts/neural-abc.sh` → `target-dev/neural-abc.json` (copied to `abc-results.json`) |
| Mutation | skipped this phase (metrics helpers + adapters; Tier-2 manual mutants deferred) |
| Memory/tokens | explicitly `not_measured` with reasons — not fabricated |

## Locked family means (measured)

| Arm | R@3 | MRR | NDCG@5 | P@3 |
|---|---|---|---|---|
| A | 0.667 | 0.667 | 0.667 | 0.667 |
| B | 0.667 | 0.667 | 0.667 | 0.667 |
| C | 1.000 | 1.000 | 1.000 | 0.333 |

Held-out: C does not beat B → no `SEMANTIC_WEIGHT` change.
