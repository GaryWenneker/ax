# AX Neural — Learning validation (§4)

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/VALIDATION-LEARN.md`  
**Date:** 2026-11-16  
**Evidence:** `/Users/gary/io/ax/docs/specs/ax-neural/learn-validate-results.json`  
**Also:** `agent-eval-results.json` arm E (LOO ranking-prior)

## Protocol

| Step | Result |
|---|---|
| A. Before (no feedback) | Lexical-overlap baseline on locked eval questions |
| B. Collect feedback | Explicit success rows from **non-eval** fixture questions |
| C. Train candidate | `train_ranking_prior` seed=7, val ratio 0.25 |
| D. Held-out eval | Locked families only; `evalInTrain=false` |
| E. Compare | Deltas below |
| F. Restart | Retrain same seed + JSON reload — weights & ranking identical |

## Gates

- `promoted=false`, `autoPromote=false`
- Eval execution IDs absent from train set
- Production defaults unchanged (`semanticWeight=0`)

## Metrics (locked eval, n=4 questions / 3 scored)

| | P@3 | R@3 | MRR |
|---|---|---|---|
| Before | 0.222 | 0.667 | 0.714 |
| After candidate | 0.333 | **1.000** | 0.500 |
| Δ | **+0.111** | **+0.333** | **−0.214** |

Candidate validation fold inside trainer: `validationScored=0` (split too thin after holding out locked families) — internal signAgreement not measured; held-out ranking above is the authority.

## Verdict

Learning improves recall/precision@3 on this tiny held-out set but **regresses MRR**. Model is stable across simulated restart. **Do not promote.** Not evidence for production ranking change.
