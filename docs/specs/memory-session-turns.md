# Sessions and turns

Status: scenarios 1 through 12 are in the code. Spec approval: not a separate review. The plan was started with "go", so this file is the contract the tests lock in. Confidence is lower than a spec reviewed before code.

Scenario 1 writes the current turn file. Scenarios 2 through 5 record the turn and fill outcome, files, and commits. Scenario 6 stamps activity. Scenario 7 stamps `remember` and `save_turn`. Scenario 8 replaces the read-guard turn id. Scenario 9 replaces MCP context placeholders. Scenarios 10 and 11 add link kinds and `follows`. Scenario 12 adds `MemoryFilter` and `recall_filtered`. CLI/MCP filter wiring, Logging page, and backfill are still open. Schema is 36.

## Scenario

1. A `beforeSubmitPrompt` payload with `conversation_id` `conv-1` and `generation_id` `gen-1` writes `<project>/.ax/turns/current.json` containing `session_id` `conv-1` and `turn_id` `gen-1`. When the payload also has `session_id` `sess-1`, `session_id` in the file is `sess-1`, which is the id this hook already records. A blank or missing `generation_id` writes no file. An event other than `beforeSubmitPrompt` writes no file.
2. Recording session `conv-1` and turn `gen-1` with prompt `hello` inserts one `agent_sessions` row and one `agent_turns` row. The turn's sequence is 1 and its prompt is `hello`. Recording `gen-1` again does not add a row and does not replace the prompt. Recording `gen-2` in the same session stores sequence 2. A turn in another session starts again at sequence 1. A blank turn id writes nothing. The stored prompt is cut at 8,000 characters.
3. The `beforeSubmitPrompt` hook stores the prompt after `redact_secrets`. The text `use key sk-abcdefghijklmnopqrstuvwx now` is stored as `use key [redacted] now`.
4. Finishing turn `gen-1` stores outcome `done`, files `["src/a.rs"]`, and commits `["abc"]`, and sets `ended_at`. Finishing an id that has no row changes nothing. An outcome is cut at 20,000 characters. A later finish with no outcome keeps `done` and replaces the file and commit lists.
5. When a turn ends for conversation `conv-1` and `.ax/turns/current.json` names that session and turn `gen-1`, the row receives the redacted reply and the files the turn record already computed. A current file for a different session is left unchanged.
6. An activity event with no session and no turn, while that file names `conv-1` and `gen-1`, stores those ids. An event that already names session `kept` keeps `kept` and does not take turn `gen-1`. With no current file, both stay empty.
7. A memory saved while that file names `conv-1` and `gen-1` stores those ids on the memory row. A memory saved with no current file stores empty session and turn ids. The columns are `session_id` and `turn_id`. `remember` and `save_turn` are the writers in this scenario.
8. The read guard's recorded turn id is `gen-1`, and its session id is `conv-1`, when the database's sibling `turns/current.json` names those ids. With no current file, the session stays the caller's id and the turn id stays `read-guard`.
9. An MCP context request uses session `conv-1` and turn `gen-1` when the project has that current file. With no file, the session stays `mcp` and the turn stays `context`.
10. Linking memory `a` to `b` with kind `follows` inserts one `memory_links` row. Linking again does not add a second row. Kind `nope` returns an error and inserts nothing. The allowed kinds are exactly `in_session`, `follows`, `produced`, `changed`, `touches`, `committed_as`, `derived_from`, `supersedes`, and `source`.
11. After turn memories `turn-old` and `turn-new` are both stamped with session `conv-1`, and `turn-old` has the earlier `created_at`, saving `turn-new` writes a `follows` link from `turn-new` to `turn-old`. A turn with no session writes no link.
12. `MemoryFilter` accepts the keys `session`, `turn`, `keyword`, `trigger`, `file`, `symbol`, `kind`, `since`, and `until`. Recalling with session `conv-1` returns only memories stamped with that session. Recalling with turn `gen-2` excludes a memory stamped `gen-1`. An empty filter keeps the unfiltered recall order.

## Known limits

- The hook writes the file and the rows only after it has resolved a project root from `cwd`. A payload with no project root does not create a directory.
- The Logging page, backfill, and the remaining link writers (`produced`, `changed`, and the rest) are not in this slice. CLI and MCP surfaces get the same filter keys in the next slice. A prompt and an outcome are redacted with the existing secret redactor, not a new one.
