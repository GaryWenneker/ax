# Ax context efficiency benchmark

Host: Cursor only. Model for both coding runs: **grok-4.7**, context 256k, reasoning high, fast off, maxMode false. Same task: add `Ledger::void` to `bench-ledger`. Same starting tree, copied from `seed/` into `run-01` and `run-02`. Parent ax changes were not reset.

Provider input and output tokens are **not available**. Both composers store `tokenCount.inputTokens` 0 and `tokenCount.outputTokens` 0, and neither has `promptTokenBreakdown`. The subagent transcripts have no usage object. Savings percentages are not reported.

`cargo test --offline` was re-run after the agents finished. Both runs: 1 apply test + 3 void tests, 0 failed.

## Results

| Test | Variant | Input tokens | Output tokens | Total | Input saving | Total saving | Quality | Tests |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 1 | No Ax |  |  |  | baseline | baseline | 9/10 | 4 passed, 0 failed |
| 2 | Ax |  |  |  |  |  | 10/10 | 4 passed, 0 failed |
| 3 | Ax optimized |  |  |  |  |  |  | not run |

What was measured instead:

| Test | Duration | Bubbles | Ax tool calls | Other tool calls | Files changed |
| --- | ---: | ---: | ---: | ---: | --- |
| 1 No Ax | 141 s | 48 | 0 | 29 | Cargo.toml, src/ledger.rs, src/lib.rs, tests/void.rs, Cargo.lock |
| 2 Ax | 317 s | 78 | 15 | 25 | src/ledger.rs, src/lib.rs, tests/void.rs, Cargo.lock |

Ax calls on test 2: `ax_explore` 5, `ax_node` 3, `ax_guard` 3, `ax_preflight` 1, `ax_skill` 1, `ax_session` 1, `ax_sync` 1. Test 1's transcript has no `ax_*` call. Cursor still injected the workspace rules into the process; the agent was instructed not to call Ax and the transcript shows it did not.

Times are the first and last bubble `createdAt` on the composer (`13:14:09.406Z`–`13:16:30.646Z` and `13:14:12.717Z`–`13:19:30.003Z`).

### Test 3 turns

| Turn | Normal context | Optimized context | Reduction | Output | Quality state |
| ---: | ---: | ---: | ---: | ---: | --- |
|  |  |  |  |  | not run |

## Quality

| Arm | Functional | Tests | Code | Context retention | Total |
| --- | ---: | ---: | ---: | ---: | ---: |
| No Ax | 4 | 2 | 1 | 2 | 9 |
| Ax | 4 | 2 | 2 | 2 | 10 |

Both implementations void an open entry, reject an unknown id, reject a second void, and leave the other entry in the balance. Test 1 also added a `[[test]]` entry and a lockfile. Test 2 did not need the manifest edit; Cargo still ran `tests/void.rs`. That is why code quality is 1 versus 2. Neither arm dropped a required case.

## Conclusion

1. **How many tokens does Ax save versus no Ax?** Not computed. The counters Cursor writes for these two composers are 0, and the context snapshot is missing.
2. **How many extra tokens does Ax itself cost?** Not computed. The Ax arm made 15 Ax calls and ran 317 s versus 141 s. That is time and tool calls, not tokens.
3. **How much context can be removed safely?** Not measured. Both arms scored at least 8/10, but there is no optimized-context arm.
4. **What must stay?** Objective, constraints, decisions, files, symbols, implementation state, bugs, tests, open questions, facts (`ax_session`).
5. **What can be fetched again?** File bodies and old tool output (`ax_explore`, `ax_node`, `ax_expand`, `ax_durable`).
6. **What can be dropped?** Duplicate tool output and repeated graph replies. Not re-measured on billed tokens.
7. **Where quality falls:** unknown.
8. **Strategy already in the product:** working snapshot, `known_context`, `session`, re-query instead of replaying the transcript.
9. **Needed for a token result:** Cursor must store non-zero per-request input and output on the composer. These two runs did not get `promptTokenBreakdown` at all (the parent chat does; the subagents do not).
10. **Not needed:** another savings estimator. The behavioral comparison already ran on the existing tools.

Raw JSON: `test-01-baseline.json`, `test-02-ax.json`, `test-03-ax-optimized.json`, `cursor-telemetry.json`.
