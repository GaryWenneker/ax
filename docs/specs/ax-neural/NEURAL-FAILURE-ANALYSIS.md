# Neural failure analysis

**Absolute path:** `/Users/gary/io/ax/docs/specs/ax-neural/NEURAL-FAILURE-ANALYSIS.md`  
**Date:** 2026-11-16  
**Scope:** Preliminary held-out disagreements only. These runs are still in progress and are not a promotion sample.

The live benchmark process was not stopped and its tasks, labels, and scoring were not changed. Nothing in this note is a weight, threshold, or model update.

## What the harness actually changed

`scripts/neural-realworld-agent.py` writes `ax.json` `neural` before each arm:

| Arm | `prediction.enabled` | `learning.feedback_collection` |
|---|---|---|
| A production retrieval | false | false |
| B neural ranking | false | false |
| D prediction | true | false |
| E learning | true | true |

A and B receive the same configuration. The arm name `B_neural_ranking` does not select a different retriever, a different `SEMANTIC_WEIGHT`, or an ONNX model. `prediction_active()` in `crates/ax-neural/src/config.rs` returns `prediction_enabled` and is read by neural dispatch and status. Production semantic blend stays 0. Arm C is not in this process; ONNX embedding status remains `not_built`.

## What was not recorded

The stored rows in `docs/specs/ax-neural/REALWORLD-RESULTS.json` do not contain retrieved candidates, provenance, original scores, neural scores, final ranking, selected context, excluded context, mandatory-context checks, prediction contributions, learning contributions, or agent tool-call lists. Those ten fields are **not observed** for this run. They are not inferred here.

What is observed per arm: verify pass/fail, agent process outcome (`ok`, `agent_timeout`, token counts, wall time), a truncated agent text tail, and the session id when the process returned JSON.

## Snapshot used for this note

Taken while the daemon was still running (about 23 of 30 held-out tasks fully paired, current task `sec-10`, repetition 0 only). Counts below are from the checkpoint rows at analysis time and will move as the run continues.

On the 23 complete tasks, arm outcomes split as:

| Arm | Finished and passed | Finished and failed | Timed out and failed | Timed out and passed |
|---|---|---|---|---|
| A | 13 | 8 | 1 | 1 |
| B | 12 | 7 | 4 | 0 |
| D | 11 | 8 | 4 | 0 |
| E | 11 | 7 | 3 | 2 |

A and B both finished on 19 tasks. They disagreed on **one** of those 19 (`bug-mf-02`). The other three A-versus-B disagreements are timeout cuts.

## Discordant tasks

Success means every automated verify step exited as required. The agent did not score itself.

### `bug-mf-02` — only finished disagreement

| Arm | Verify | Agent | Wall time | Input tokens | First verify step |
|---|---|---|---|---|---|
| A | fail | finished | 351 s | 101108 | exact `prediction_active` body missing |
| B | pass | finished | 240 s | 157120 | both steps pass |
| D | fail | finished | 172 s | 90526 | exact body missing |
| E | pass | finished | 293 s | 107842 | both steps pass |

All four text tails describe the same defect: `prediction_active()` returning a constant instead of `self.prediction_enabled`. The first observed divergence is the file content at verify time, not a recorded ranking difference. A and B used the same neural flags, so this pair is not evidence that neural ranking selected different context. It is consistent with agent execution variance on an exact source assertion. Per-arm diffs were not kept; the harness restores the inject after verify.

### `bug-mf-07`, `sec-08` — timeout versus a finished edit

A finished and verify passed. B, D, and E were killed at the 420 s agent limit. Their first verify step still saw the original fixed anchor, and the second step still saw the broken inject marker. The first divergence is the process kill, before a completed edit. No context trace exists, so this is not classified as a context-selection failure.

### `ref-08` — all four arms hit the time limit

Every arm reports `agent_timeout` at about 420 s. A and E had already left the file in the fixed state, so verify passed. B and D still contained the broken marker, so verify failed. The first divergence is whether the edit landed before the kill. A and B again shared the same neural configuration.

## Classification

| Disagreement | First observed divergence | Context-selection failure? |
|---|---|---|
| `bug-mf-02` A/D fail, B/E pass | Exact source string after a finished edit | Not shown. A and B were configured identically. |
| `bug-mf-07`, `sec-08` | 420 s kill on B/D/E while A had finished | Not shown. No candidate list was stored. |
| `ref-08` | 420 s kill; A/E happened to have written the fix | Not shown. Same reason. |

Agent execution variance, especially the hard timeout, explains the preliminary McNemar gaps (B 1 win / 2 losses, D 0 / 2, E 1 / 1 at the earlier 17-task cut, plus the later `sec-08` timeout split). Those gaps are not a measured retrieval-quality difference.

## Later checkpoint, still not a ranking result

A later read of the same running log, merged with `REALWORLD-RESULTS.json` while the daemon was still up, had 27 of 30 tasks paired. `feat-03` was still incomplete. `perf-05` finished and failed on every arm, so it is not a new disagreement. The thin log lines store `taskSuccess` only. Pass counts on those 27 are A 16, B 14, D 13, E 15.

The taskSuccess disagreements are still only `bug-mf-02`, `bug-mf-07`, `ref-08`, and `sec-08`. Against A, that is B 1 win and 3 losses, D 0 wins and 3 losses, E 1 win and 2 losses. A remains ahead. This checkpoint was not used to change a weight or a threshold.

## Later checkpoint, after perf-07

`feat-03` is in the train split. It is not one of the 30 held-out tasks, so a row for it does not count toward this matrix. After `perf-07`, 28 of those 30 tasks are paired. `perf-07` passed on A, B, D, and E, so it is not a disagreement. Pass counts on the 28 are A 17, B 15, D 14, E 16. The four disagreements above are unchanged. `perf-08` and `perf-10` were still incomplete, and `perf-08` had already replaced the production anchor in `crates/ax-neural/src/lib.rs`. That file was left unchanged. This checkpoint was not used to change a weight or a threshold.

## Later checkpoint, after perf-08

`perf-08` is now paired. A, D, and E passed. B failed. The thin log stores `taskSuccess` only, so this B loss is not classified as a timeout and no candidate list, score, or tool-call trace was stored. Pass counts on the 29 paired tasks are A 18, B 15, D 15, E 17. Against A, that is B 1 win and 4 losses, D 0 wins and 3 losses, E 1 win and 2 losses. `perf-10` was already broken in `crates/ax-neural/src/rss.rs` when this was written, so that file was left unchanged. This checkpoint was not used to change a weight or a threshold.

## Later checkpoint, repetition 0 complete

The daemon exited 0. Its command was `--reps 1 --seed 29 --skip-done`. `perf-10` is paired, and A, B, D, and E all passed, so it is not a disagreement. The harness restored `crates/ax-neural/src/rss.rs`. All 30 held-out tasks are paired for repetition 0. This command did not start repetition 1.

Pass counts are A 19, B 16, D 16, and E 18. Failures are A 11, B 14, D 14, and E 12.

Against A, the disagreements are unchanged by `perf-10`:

| Arm | Wins | Losses | Tasks |
|---|---|---|---|
| B | 1 | 4 | Win `bug-mf-02`. Losses `bug-mf-07`, `ref-08`, `sec-08`, `perf-08`. |
| D | 0 | 3 | Losses `bug-mf-07`, `ref-08`, `sec-08`. |
| E | 1 | 2 | Win `bug-mf-02`. Losses `bug-mf-07`, `sec-08`. |

The thin log still stores only `taskSuccess`. The full rows in `REALWORLD-RESULTS.json` are read below. This checkpoint was not used to change a weight, a threshold, or a model.

## Stored agent rows for the five disagreements

The checkpoint rows have `agent.error`, `agent.stderrTail`, `elapsedMillis`, and `verify.steps`. They do not have retrieved candidates, baseline scores, neural scores, or a tool-call list. No files-changed list is stored. The verify step names the file that was still broken. These rows were not used to change a weight.

| Task | Arm | Success | Elapsed | Agent record | Verify still failing |
|---|---|---|---|---|---|
| `bug-mf-02` | A | false | 351 s | finished, `ok` true, no error | `crates/ax-neural/src/config.rs` |
| `bug-mf-02` | B | true | 240 s | finished, `ok` true, no error | none |
| `bug-mf-02` | D | false | 172 s | finished, `ok` true, no error | `crates/ax-neural/src/config.rs` |
| `bug-mf-02` | E | true | 293 s | finished, `ok` true, no error | none |
| `bug-mf-07` | A | true | 399 s | finished, `ok` true | none |
| `bug-mf-07` | B, D, E | false | 420 s | `agent_timeout`, stderr `timed out after 420s` | `crates/ax-mcp/src/server.rs` |
| `ref-08` | A | true | 420 s | `agent_timeout` | none. The file was already fixed when the kill landed. |
| `ref-08` | B, D | false | 420 s | `agent_timeout` | `crates/ax-mcp/src/context_budget.rs` |
| `ref-08` | E | true | 420 s | `agent_timeout` | none. Same pattern as A. |
| `sec-08` | A | true | 343 s | finished, `ok` true | none |
| `sec-08` | B, D, E | false | 420 s | `agent_timeout` | `crates/ax-mcp/src/remote/oauth.rs` |
| `perf-08` | A, D, E | true | 252 s, 251 s, 333 s | finished, `ok` true | none |
| `perf-08` | B | false | 23 s | `non-json agent output`, stderr `Error: [unavailable] read ETIMEDOUT` | `crates/ax-neural/src/lib.rs` |

`perf-08` B is not a 420-second kill. The agent process returned a non-JSON transport error at about 23 seconds, and the production anchor in `lib.rs` was still broken. A and B were configured with the same retrieval flags, so this is not a measured ranking difference.

`ref-08` diverges at the kill. All four arms hit `agent_timeout`. A and E had written the fix before the kill, so verify passed. B and D had not, so verify still saw the broken anchor. The harness does not keep the per-arm diff. Nothing in the row shows a different candidate list.

`bug-mf-02` is the finished-edit disagreement. All four arms returned `ok` with no error. A failed the same `config.rs` check that B passed. A and B do not differ in the neural flags the runner writes.

## What this does not justify

- Changing `SEMANTIC_WEIGHT` or production defaults.
- Tuning thresholds on these held-out tasks.
- Treating arm B as a neural reranker result.
- Claiming prediction or learning caused the losses. D and E do flip flags. The stored rows show 420-second kills, one finished-edit disagreement under identical A/B flags (`bug-mf-02`), and one transport failure (`perf-08` B, `read ETIMEDOUT` at 23 seconds). No prediction or learning contribution was logged.

## Next measurement, without touching this run

A later experimental trace, behind a flag the current process does not read, has to record candidate source, baseline score, neural score, keep/drop reason, mandatory-context preservation, prediction accept/reject reason, and whether the agent actually opened the file. Until that trace exists, context-selection quality on this benchmark stays **not measured**.
