# Plan: durable session inside ax

Status: accepted on 2026-10-02. The user rejected a side-by-side Pi Durable harness ("ik wil eigenlijk dat wat in pi zit, gewoon in ax terechtkomt. dus niet twee systemen naast elkaar") and had already accepted immediate execution of the plan.

Pi Durable (https://earendil.com/posts/pi-durable/, 1 October 2026) stays an external project. The ideas that match ax land in ax. There is no adapter package and no second conversation store.

## What lands in ax now

1. `ax_session` action `fork` copies this chat's notes to a new `axs_` id. The reply is `<ax_session_fork parent=P child=C>` plus the child's notes block. The parent row is unchanged. The child's graph cache starts empty.
2. `ax_session` action `handoff` stores the note you send (same fields as `compact`) as a new session and leaves the parent notes readable under the old id. The reply is `<ax_session_handoff parent=P child=C>` plus the child's notes. The graph cache is not copied.
3. Fork of a chat with no notes returns `nothing to fork` and writes nothing. Handoff with an empty note returns an error and writes nothing. Handoff with no readable index returns `index unavailable; retry after ax_sync`.
4. Neither action resets the parent's quiet-turn count.
5. Preflight wraps the notes in `<ax_section name="session">` and the nudge in `<ax_section name="nudge">`, so a host can keep those bytes stable. The tags already inside those sections stay as they are.
6. Seeds name `fork` and `handoff`. `seedVersion` on the five templates that carry the working-context paragraph goes from 3 to 4.

## What does not land in this change

Ax does not call the model, so it does not grow a task runner, a transcript log, background summarization, or a checkpoint of an in-flight model call. Notes and cached graph answers already survive a daemon restart in `usage.db`. Provider cached-input tokens are still decided by whoever builds the model request; the new section tags are what that host can cache.

## Must not change

`ax_context` still builds task context. No `ax_state` tool. Add, update, and compact still refuse an empty fingerprint. Existing session tests keep passing.

## Setup

Same branch and worktree. No new dependency. Mutants stay stopped until asked. Tests: the new unit and integration tests, `cargo test -p ax-usage -p ax-mcp -p ax-policy`, and clippy on those crates.
