# SPEC — Phase 4: Predictive intelligence

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/SPEC-PHASE4-PREDICT.md`  
**Depends on:** Phase 3  
**Tier:** 2  
**Spec approval:** not obtained (autonomous goal run)

## Goal

Every prediction includes: item, confidence, explanation, supporting evidence, and relationship status (`verified` | `inferred`). Predict memories, skills, rules, files, graph relationships, and historical execution patterns from project-isolated feedback. Evaluate against actual context usage (leave-one-out fixture + live feedback when enabled). Default remains **disabled** (`neural.prediction.enabled`).

## Scenarios

1. **Given** matching feedback rows, **when** `predict` runs, **then** each `Prediction` has non-empty `explanation`, non-empty `evidence`, and `relationship` in {`inferred`,`verified`}.
2. **Given** co-occurrence only (no graph edge), **when** predicting, **then** `relationship` is `inferred`.
3. **Given** a graph edge that matches a predicted relationship, **when** `predict_with_edges` runs, **then** that item’s `relationship` is `verified`.
4. **Given** context ids `memory:…` and bare paths, **when** predicting, **then** kinds are `memory` and `file` respectively; skills/rules keep their kinds; repeated task co-occurrence may emit `pattern`.
5. **Given** leave-one-out on the coding fixture, **when** `evaluate_fixture` runs, **then** precision/coverage remain measured (regression armor); predictions still carry the new fields.
6. **Given** prediction disabled in config, **when** `ax neural predict` runs, **then** `applied: false` (unchanged default).

## Must not

- Enable prediction by default
- Claim neural inference for frequency co-occurrence
- Cross-project feedback leakage
