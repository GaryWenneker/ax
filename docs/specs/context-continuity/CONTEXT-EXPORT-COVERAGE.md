# Context export coverage

Command:

```bash
cargo test -p ax-usage --lib context_export
```

The numbers below are the closed three-turn fixture locked by `the_published_fidelity_file_matches_a_fresh_run`. They are not a live session.

```text
Turns: 3 / 3
Decisions: 1 / 1
Rules & Skills: 2 / 2
Unresolved references: 0
Source integrity: Verified
Context relevance: Not yet tested
Cross-chat continuity: Pending
Completeness: complete
Blockers: none
```

`Source integrity: Verified` means the export matches the input it was built from, the fixture here. It does not re-read the store, and it does not mean a live session is complete.

## Other recorded cases

| Case | Completeness | What the test locks |
| --- | --- | --- |
| Open inventory | partial | Turns line is `unverified`. `complete` is rejected by the verifier. |
| Sequence gap | partial | A missing sequence number blocks `complete`. |
| 200 turns | complete | Agent-ready keeps the early decision and the rejected approach. It references prompt 1 and includes prompt 200. |
| Contradictory decisions | partial only for other gaps | Both decision texts stay in the export. |
| Rewritten prompt | verifier error | The document hash no longer matches the source turn. |
| Redacted secret | partial | `sk-secret-value` is absent from the JSON, and no exported hash is computed from it. A copy of a redacted prompt or outcome anywhere in either document fails verification. |
| Unresolved memory | partial | State is `unavailable`. Unresolved count is 1. |
| Missing tool refs | partial | State is `unavailable`. The exporter does not invent a tool result. |
| Stored `agent_turns` | partial | Two stored turns are captured, but the turns line is `unverified` because the store cannot prove the inventory is closed. Decisions are `not captured`. The over-cap prompt is `truncated`. |
| Unknown session id | error | The loader refuses a session with no stored row instead of exporting zero turns. |

A partial export stays partial. The coverage report does not upgrade it.
