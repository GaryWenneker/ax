# Context attribution

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/CONTEXT-ATTRIBUTION-REPORT.md`  
**Date:** 2026-11-16

## Live benchmark

The finished checkpoint does not store candidates, scores, selected context, or tool calls. It does store `agent.error`, elapsed time, and the verify step that was still failing. On the five disagreements, the first split is a 420-second kill, a 23-second `read ETIMEDOUT` on `perf-08` B, or a finished edit of `config.rs` under identical A/B flags (`bug-mf-02`). Retrieved context was not observed, so it cannot be treated as used context, and those outcomes are not attributed to a ranker. The detail is in `NEURAL-FAILURE-ANALYSIS.md`.

## Experimental trace

`select_context` writes a JSON trace when it is called:

| Field | Meaning |
|---|---|
| `id`, `source` | Candidate and whether it came from baseline, neural, or prediction |
| `baselineScore`, `neuralScore` | Scores supplied by the caller. Missing neural scores are not invented. |
| `position`, `tokens` | Final order and token cost |
| `mandatory`, `protected` | Preservation flags |
| `outcome`, `reason` | Accepted, rejected, kept, or fallback |
| `displaced` | Optional baseline ids removed to pay for an addition |
| `event: attribution` | `modelVersion` and `feedbackIds` copied from `RecoveryConfig`. Both default to empty. |

The header does not include a task outcome. The agent has not run yet, and this function does not invent one. It also does not include tool calls. A selected id is not treated as a used id, and the header does not say the model version caused a task result.

## Ablation

No ablation against the live 30 tasks was run.

A separate two-question fixture, not taken from those tasks, compares three orders. Mandatory `rule` stays first. The relevant memory is optional.

| Order | MRR | P@3 | R@3 |
|---|---|---|---|
| Baseline, selector disabled | 0.4167 | 0.333 | 1.0 |
| Optional slots reranked by query-conditioned scores | 0.500 | 0.333 | 1.0 |
| Optional slots reranked by one global frequency prior | 0.4167 | 0.333 | 1.0 |

The query-conditioned scores move the relevant memory ahead of the other optional item on both questions. The global prior fixes one question and breaks the other, so mean MRR does not move. That matches the learning analysis: a frequency table is not query-conditioned. The trace records the new order. It does not show that an agent used the context or finished a coding task.
