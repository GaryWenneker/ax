# Learning and Prediction Report — Real-World Benchmark

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/LEARNING-AND-PREDICTION-REPORT.md`

## Constraints (frozen)

- Do not promote learning.
- Do not activate prediction in production based on prior short-suite or incomplete real-world matrix.
- Train feedback for arm E may use **train-split tasks only** (20 tasks); held-out (30) never trains.

## Prediction

| Evidence | Result |
|---|---|
| Prior production-validation leave-one-out | Precision 1.0, coverage 0.33, FP 0; ranking P@k≈0.33 |
| Prior 30-task naming suite | Soft success tie; no Neural win |
| Real-world held-out agent matrix (interim) | **17/30** pairs. D vs A net **−2** (0 wins, 2 losses). E vs A net **0**. |
| Calibration on real-world coding tasks | `not_measured` (token/confidence not yet exported per arm) |

**Gate:** prediction value beyond baseline retrieval — **not demonstrated**. On 17 paired tasks, arm D is behind A (net −2).

## Learning

| Evidence | Result |
|---|---|
| Prior learn-validate (fixture) | R@3↑, MRR↓; no auto-promote; restart-stable |
| Real-world train→held-out learning run | Arm E vs A on 17 pairs: net **0** (`bug-mf-02` E wins, `bug-mf-07` A wins). `ref-08` E ties A. |
| Data leakage into held-out | Design forbids; runner uses train-only feedback flag; no held-out train writes observed |

**Gate:** held-out improvement without MRR/NDCG regression — **not met** for promotion.

## Classification

| Component | Class |
|---|---|
| Prediction | `implemented_not_beneficial` (short probes; real-world D net −2 vs A on n=17) |
| Learning ranking-prior | `implemented_not_beneficial` for promotion (fixture MRR↓; real-world E net 0 on n=17) |
