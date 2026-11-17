# SPEC — Open recall

Status: the scenarios below are implemented and covered by tests.
Tier: 2.
Isolation: none. The changes are inside the existing memory tests.
Approval: the deterministic-memory plan was approved with "go". This file was not reviewed on its own. Evidence records `spec approval: not obtained (autonomous run)`.

This slice does not re-embed the live vault and does not change who may write an embedding.

## Scenario: a trigger behind non-injectable rows is still returned

Given eight turn rows and one decision, each with the trigger cue `zulu-cue`, and the turn ids sort before the decision id.
When `prompt_cues` is asked for one hit on the prompt `please zulu-cue now`.
Then the only hit is the decision `z-hot`.

## Scenario: a keyword hit is not hidden by its embedding model

Given a decision whose body contains `zephyrword` and whose `embedding_model` is `minilm`.
When hot recall searches for `zephyrword`.
Then that decision is in the hits.

The same rule applies in the archive. Two archived rows with the same words are both returned when one of them is labeled `other-model`. The vector index still refuses to store that foreign row.

## Scenario: an unmatched prompt injects nothing

Given one turn and one decision, and the prompt `zzzz` shares no token with either.
When preflight asks for up to three memories.
Then the result is empty.

## Scenario: a turn or commit is not turned into a decision

Given a git memory whose body says `We decided to keep sqlite-vec because it stays local.`
When consolidation runs.
Then it creates no decision and no `source` link.
An existing decision with the same body stays `active`, and no `supersedes` link is written.
Schema 31 marks an existing consolidation candidate `rejected` and renames a `supersedes` link to `revoked_supersedes`. Confirm restores both. A decision that was not a consolidation candidate stays active.

## Scenario: a short memory block keeps its reserved slice

Given a required block that already uses more than the budget, and a memories block whose text is `hash vectors`.
When context is selected with a budget of 1 token.
Then both blocks are kept. The reserve is 300 tokens unless `context.memoryTokens` says otherwise.
A memories block longer than the reserve is not forced in.

## Scenario: automatic cues skip change verbs, kinds, and primitive types

Given a decision titled `Updated the function string parser`.
When its cues are registered.
Then `parser` is a cue and `updated`, `function`, and `string` are not.
An explicit trigger is still stored even when the word is one of those.

## Scenario: a graph change does not cue the other symbols in the file

Given `flush` and `enqueue` are both symbols in `src/queue.rs`, and only `flush` changes.
When the graph episode is written.
Then that episode's cues include `flush` and do not include `enqueue`.

## Scenario: an automatic cue that is already on 32 memories is not added again

Given 32 memories already have the automatic cue `commonword`.
When another memory is titled `commonword finale`.
Then that memory does not gain the cue `commonword`.
An explicit trigger of the same word is still stored.

## Scenario: a rare title cue outranks a common title cue, and a trigger still comes first

Given eight memories titled with `commonword`, one memory titled `rareword keeper`, and one memory whose trigger is `commonword`.
When prompt cues are asked for `rareword commonword`.
Then the trigger memory is first, and the `rareword` memory comes before the `commonword` memories.

## Scenario: gitignored paths stay out of the index and the pending list

Given `.gitignore` lists `secret.rs` and `target-dev/`, and those files exist next to `src/keep.rs`.
When files are scanned, `secret.rs` and `target-dev/out.rs` are absent and `src/keep.rs` is present.
A pending-path check rejects `secret.rs` and `target-dev/out.rs` and keeps `src/keep.rs`.

## Must not

- Do not delete or rewrite rows in the user's `.ax/ax.db`.
- Do not drop an archive row from archive search only because its model name differs. A row that was not placed in the vector index stays absent.
- Do not stop the project daemon, MCP, or `ax web`.
