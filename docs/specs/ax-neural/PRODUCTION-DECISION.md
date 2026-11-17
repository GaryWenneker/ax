# Production Decision — Real-World Benchmark

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/PRODUCTION-DECISION.md`  
**Date:** 2026-11-16

## Frozen conclusions (unchanged)

These remain in force regardless of this benchmark’s incomplete agent matrix:

1. **ONNX production activation is not justified.**
2. **Prediction has not demonstrated improved agent success.**
3. **Learning must not be promoted.**
4. **No measurable production improvement has been demonstrated.**

Production defaults stay unchanged (`SEMANTIC_WEIGHT=0`, hash-v1 memory column, no auto-promote).

## Gate checklist

### ONNX activation

| Gate | Status |
|---|---|
| Verified end-to-end production integration | **Fail** (inactive / arm C blocked) |
| Improvement on held-out retrieval | **Not shown** (no ONNX arm data) |
| No unacceptable locked-family regression | Prior lexical held-out caution stands |
| Latency / memory overhead measured for production path | Bake-off only; not production path |
| Successful fallback validation | N/A while inactive |

**Decision:** **Do not activate.** Class: `rejected_for_production`

### Prediction activation

| Gate | Status |
|---|---|
| Value beyond baseline retrieval on realistic tasks | Interim **17/30** pairs: D vs A net **−2**; B net **−1**; E net **0**. Under-powered, **do not activate** |
| Acceptable FP rate | Fixture: FP=0 (narrow) |
| Calibrated confidence | Under-powered |
| No negative impact on mandatory context | Ranking still required; prediction default off |

**Decision:** **Do not activate.** Class: `implemented_not_beneficial` / `not_sufficiently_evaluated`

### Learning promotion

| Gate | Status |
|---|---|
| Held-out improvement | Fixture MRR **regressed** |
| No data leakage | Design enforces train/heldOut; real-world run pending |
| No material MRR/NDCG regression | **Failed** on fixture |
| Reproducible after restart | Fixture: yes |
| Successful rollback | Promote never done |

**Decision:** **Do not promote.** Class: `implemented_not_beneficial`

## Component summary

| Component | Classification |
|---|---|
| Hash-v1 + locked fusion (production) | `production_validated` as **baseline** (not as a Neural upgrade) |
| Neural ranking (lock_required) | `production_validated` as required infra; **upgrade benefit** `not_sufficiently_evaluated` |
| Lexical challenger | `experimentally_promising` on some recall metrics; not a Neural embed |
| ONNX MiniLM | `rejected_for_production` |
| Prediction | `implemented_not_beneficial` (short + interim real-world null on n=6 pairs) |
| Learning ranking-prior | `implemented_not_beneficial` for promotion (fixture MRR↓; interim real-world null) |

## Final statement

This benchmark’s objective is discovery, not passage. Current evidence does **not** support shipping Neural upgrades. Incomplete agent-matrix cells are reported as `not_measured` / `not_sufficiently_evaluated`, not as successes.
