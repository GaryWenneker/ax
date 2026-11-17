# AX Neural — Prediction validation (§3)

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/VALIDATION-PREDICT.md`  
**Date:** 2026-11-16  
**Evidence:** `/Users/gary/io/ax/docs/specs/ax-neural/predict-validate-results.json`  
**Also:** `agent-eval-results.json` arm `D_ax_neural_prediction`  
**Method:** leave-one-out on coding fixture (19 questions; 15 with non-empty gold). Minimum confidence 0.75.  
**Note:** Fixture families are shared training signal — not fully independent coding tasks. No live agent “used” trace; gold `relevant` is the use proxy.

## Metrics

| Metric | Value | n |
|---|---|---|
| Prediction precision | **1.0** | 5 predictions |
| False positives | **0** (rate 0.0) | 5 |
| Coverage (questions with ≥1 hit / expected) | **0.333** | 5/15 |
| Precision@3 | 0.333 | 15 scored |
| Precision@5 | 0.333 | 15 |
| Recall@3 | 0.333 | 15 |
| MRR | 0.333 | 15 |

## Confidence calibration

Only the **[0.9, 1.0]** bucket is populated (n=5); empirical accuracy **1.0**. Lower buckets empty at minConfidence=0.75 — calibration curve is **under-powered** (single bucket).

## Predicted vs retrieved vs used

| Role | Definition | Count |
|---|---|---|
| Predicted | leave-one-out memory predictions | 5 ids |
| Retrieved | hash-v1 cosine top-5 | 95 ids (5×19) |
| Used | fixture gold relevant | 15 ids |
| Predicted ∩ used | | **5** |
| Retrieved ∩ used | | **11** |
| Predicted ∉ retrieved top-5 | | **1** |

Retrieval covers more gold items than prediction; prediction is sparse but precise when it fires.

## Context-as-ranking proxy (agent-eval arm D)

Using predictions as ranked context on locked families: **P@3=0, R@3=0, MRR=0**. Prediction accuracy ≠ useful ranking substitute for context delivery on this fixture.

## Verdict

Held-out prediction is precise and FP-free but low-coverage; ranking metrics ~0.33; calibration not established across confidence range. Defaults stay off (`prediction.enabled` default false).
