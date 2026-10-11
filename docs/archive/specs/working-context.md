# Working context

A conversation already reuses raw graph replies (`[ax cache hit]`, `ax_expand`) and lists them in
`<ax_session_context>`. That does not tell a later turn what was decided. This layer stores the
small working snapshot the agent maintains, and preflight repeats it every turn.

Spec approval: not obtained. The request was to add the sound delta; this text was not separately approved.

## Rejected

- A second session id and `ax session create`. The conversation id from `~/.ax/active-cursor-session` already scopes state.
- Folding this into `ax_context`. That tool builds a one-shot task context from the graph.
- Calling a model to compact. The agent writes the shorter snapshot; ax only enforces the size.
- Storing full agent outputs. Raw tool bodies stay in the existing caches.
- Deleting notes when the index changes. Notes are decisions, not query results. A changed index marks them stale.
- Talking to a model provider's prompt cache. A small stable snapshot is what such a cache can reuse; ax does not call the provider.

## Behaviors

- `ax_session` with no action, or `action: get`, returns the snapshot for this conversation and project. An empty one says to call `add`.
- `action: add` appends unique trimmed strings to `facts`, `files`, `symbols`, `decisions`, or `open_questions`, and sets `objective` when that key is present.
- `action: update` replaces only the sections whose keys are present. An empty array clears that section.
- `action: compact` replaces the whole snapshot. All six keys are required. It records the current index fingerprint even when the text is unchanged.
- `action: clear` deletes the snapshot.
- An entry over 200 characters, an objective over 300 characters, a newline inside an entry, more than 12 entries in one section, or a rendered block over 800 tokens is rejected and the stored snapshot is unchanged.
- The rendered block starts with `<ax_working_context hash=<16 hex> stale=true|false>`. `hash` is the SHA-256 of the objective and the five lists. The same text always has the same hash.
- `stale=true` when the stored index fingerprint differs from the current one. `add` and `update` refresh the fingerprint only when the hash changes. `compact` always refreshes it.
- Preflight appends the block on every turn when it is non-empty. It does not append the empty hint.
- `AX_CONTEXT_CACHE=off` (or `0`) makes `ax_session` fail and preflight omit the block.
- `ax_session` is not a reuse-cacheable graph tool.

## Must not

- Must not change `ax_context`'s task-only input.
- Must not cache `ax_session` replies as graph reuse hits.
- Must not write the snapshot anywhere except the usage database.
