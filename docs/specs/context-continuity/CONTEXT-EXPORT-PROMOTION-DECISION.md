# Context export promotion decision

Decision: not claimed.

The exporter and the deterministic verifier exist in `crates/ax-usage/src/context_export.rs`. They compare an export with the records that were supplied. They can mark a closed fixture `complete`. They cannot mark a live session `complete` while decisions, rules, skills, and the agent identity are `not_captured`.

Promotion to "a new chat has the same context" requires:

1. A named session whose turn inventory is closed.
2. Mandatory sections present, not invented.
3. The cross-chat answer key in `CROSS-CHAT-CONTEXT-RESULTS.json` scored above the not-performed state.
4. The functional comparison distinguishing the export from a new chat that did not receive it.

None of those four have been recorded. `equivalence_claimed` stays false. The Logging copy button still sends the short retrieval prompt.
