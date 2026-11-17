# AX Neural — Production validation completion audit

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/VALIDATION-COMPLETION-AUDIT.md`  
**Date:** 2026-11-16  
**Goal:** Independently validate whether implemented Neural capabilities deliver measurable production improvements — **not** a new architecture phase.

## Requirements

| # | Requirement | Evidence | Verdict |
|---|---|---|---|
| 1 | Audit existing evidence; reproduce claims; note n/controls/stats/env gaps | `VALIDATION-EVIDENCE-AUDIT.md`; ABC/agent-eval/ort bakeoff reproduced | **done** |
| 2 | Validate ONNX path (load→infer→persist→query→sim→retrieve→fusion→context); compare A–D; keep defaults | `VALIDATION-ONNX-PATH.md`, `path-validate-results.json` | **done** — fusion **inactive_in_production**; defaults unchanged |
| 3 | Held-out prediction: P@3/5, R@3, MRR, accuracy, calibration, FP; predicted/retrieved/used | `VALIDATION-PREDICT.md`, `predict-validate-results.json` | **done** |
| 4 | Learning before/after + restart; no eval-in-train; no auto-promote | `VALIDATION-LEARN.md`, `learn-validate-results.json` | **done** |
| 5 | ≥30 identical-LLM coding tasks across categories; Ax vs Neural | `VALIDATION-AGENTS.md`, `agent-suite-30.json`, `agent-suite-live-results.json` | **done** (30/30 executed) |

## Production-improvement claim

| Claim | Result |
|---|---|
| ONNX should be production-activated | **No** — path works under `neural-ort` validation build; ranking fusion unwired; SEMANTIC_WEIGHT stays 0 |
| Prediction improves agent success | **No** — suite soft 1.0/1.0; strict Neural slightly worse |
| Learning should be promoted | **No** — R@3↑ but MRR↓; `promoted=false` |
| Measurable improvement vs baseline Ax | **Not demonstrated** on this validation set |

## Invariants held

- `semanticWeight=0`
- `neuralEmbeddingInferenceActive=false`
- No automatic model promotion
- No architectural rewrite

## Known limits

- Fixture / suite n small; no statistical CIs
- Agent suite is short naming probes (ceiling effect)
- Restart check is serialize/reload + retrain, not a full daemon restart on every OS
- User objective text truncated after “Use” — sections 1–5 executed as specified above
