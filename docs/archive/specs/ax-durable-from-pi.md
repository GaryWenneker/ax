# Plan: Pi durable records inside ax

Status: executing. The user asked to scan `/Users/gary/io/pi` and land hooks, conversation storage, document storage, compaction, handoff, forks, durable tasks, and checkpoint recovery inside ax when they are not already there.

Source of the behavior: `packages/durable` (Pico5). Ax keeps one store, `usage.db`. There is no second harness and no model call.

## Already in ax

`ax_session` notes, note fork, note handoff, the quiet-turn nudge, and the graph reply cache.

## What this adds

`ax_durable` on the same session id:

- `append` / `read` / `search` — transcript entries (`user`, `assistant`, `tool`, `system`, `reset`, `hook`). A tool call is appended when the chat already has a transcript.
- `compact` — stores the summary the caller writes. `read` hides entries before it. `search` still returns them.
- `fork` — new id, parent entries up to `at`, documents copied, entries not copied.
- `handoff` — new id and a reset note. The parent transcript stays.
- `doc_put` / `doc_get` — one JSON document per kind.
- `task_start` / `task_checkpoint` / `task_resume` / `task_finish` — the checkpoint is what `task_resume` returns after the process reopens the database. A finished task does not resume.
- `hook` — a name recorded as a transcript entry when that event runs.

## Not in this change

Ax does not call the model, so it does not run Pi's summarizer, generation task, or tool replay. Documents are the latest JSON value, not Chord deltas. Bodies are capped at 8000 bytes. Search returns at most 50 hits.
