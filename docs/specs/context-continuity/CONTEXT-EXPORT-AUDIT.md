# Context export audit

Date: 2026-11-20. This audit records what Ax already persists. It does not change the Logging copy button or the short new-chat prompt.

## What the copy button sends today

The Logging "New-chat prompt" is a short retrieval pointer. `continuityPrompt` writes the session id, checkpoint, mode, target agent, and task, then the sentence "Historical answers are not included." It refuses a blank id or checkpoint, and it refuses a task that contains `BEGIN TRANSCRIPT`.

That text is not the seven-section export. A new chat that receives only this prompt does not receive turn bodies, decisions, or tool results.

## Where turns are stored

`ax_db::hook_turns::record_hook_turn` inserts `agent_sessions` and `agent_turns`. Each turn stores `id`, `session_id`, `sequence` (`MAX(sequence) + 1` for that session), `prompt`, `files`, and `commits`. `finish_hook_turn` later writes `ended_at`, `outcome`, `files`, and `commits`.

Caps applied by the writer, not by the exporter:

- prompt: 8,000 characters (`PROMPT_CAP`)
- outcome: 20,000 characters (`OUTCOME_CAP`)

A prompt of 8,001 characters is stored as 8,000. Re-inserting the same turn id is ignored, so the first prompt wins. A blank session id or turn id is a no-op.

`load_export_input` reads `agent_sessions.project_id` and `agent_turns` ordered by `sequence`, then `id`. An empty session id is an error. A SQL failure is an error. An empty result is a captured empty inventory, not a missing store.

## What is not a turn inventory

Memory `TurnRecord` rows (`id`, `title`, `body`, `files`, kind `turn`) live in a different table. They are not the agent-turn inventory and are not copied as turns.

Working-context notes are conversation notes. There is no decisions table and no rejected-approaches table. The loader marks decisions, constraints, open tasks, known failures, rejected approaches, memories, graph, rules, skills, repository commit, model, agent provider, and agent chat id as `not_captured` unless a caller supplies `Section::Present`. An extraction by a model is not labeled a historical fact.

## Inventory rules the exporter enforces

- `capturedTurns` equals N only when those turns were returned by the query or supplied in the fixture.
- Sequence must start at 1 and be contiguous. A gap blocks `complete`, because a missing number means a turn cannot be ruled out.
- Duplicate turn ids block `complete`.
- A prompt or outcome at the writer cap is `truncated` and blocks `complete`.
- Missing `files`, `commits`, or tool-call refs are `unavailable`, not an empty list.
- `Present` of an empty list is captured-and-empty and does not by itself block `complete`.
- `not_captured` on a mandatory section blocks `complete`.
- Any redacted field blocks `complete`.
- An unresolvable reference blocks `complete`.

## What this audit does not claim

No live session was exported as complete. Cross-chat retrieval and the functional A/B/C comparison have not been run. `equivalence_claimed` stays false.
