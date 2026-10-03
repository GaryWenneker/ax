# Evidence: durable session inside ax

Source: branch `feat/conversation-context-cache`, HEAD `585fe35`, plus the uncommitted diff in this worktree (`/Users/gary/io/ax-ctxcache`). Spec: `docs/specs/ax-durable-session.md`.

Spec approval: the user said to write the plan and execute it immediately ("creëer een plan en ik accepteer het vooraf, dus je moet het meteen uitvoeren"), then replaced the two-system design with "ik wil eigenlijk dat wat in pi zit, gewoon in ax terechtkomt. dus niet twee systemen naast elkaar." This file is that corrected plan. The user did not read it before the code, so this is not a separate approval of the written spec.

Mutants were not run. The user stopped them until asked.

## Behaviors

| Behavior | Test |
|---|---|
| Fork copies notes, parent stays, empty fork errors, handoff starts from the note, empty note and empty index error, a bad child id errors, handoff of an empty parent does not fill the parent | `fork_copies_the_notes_and_handoff_starts_a_new_session_from_the_note` |
| Fork at 200 snapshots keeps parent and child and stays at 200 | `fork_at_the_cap_keeps_the_parent_and_the_child_inside_the_cap` |
| MCP: empty fork errors, child has the notes, child graph call misses, parent graph call hits, handoff leaves the parent notes, both preflights wrap the notes in `<ax_section name="session">` | `fork_copies_notes_not_the_graph_cache_and_handoff_keeps_the_parent` |
| Fork and handoff do not restart the parent's quiet-turn count | `fork_and_handoff_do_not_restart_the_quiet_turn_count` |
| The five working-context templates are seedVersion 4 and name fork and handoff | `conversation_cache_templates_bump_seed_version` |

RED, seen before the implementation: the first unit test failed with `not implemented`. The cap test failed with `left: 201 right: 200` before the parent was written back and the cap applied again.

The quiet-turn test passed on the first run because fork and handoff were already outside the write matcher. A throwaway change that added `fork` to that matcher failed the test (`fork and handoff leave the parent's count`). The matcher was restored. That is one negative control, not a mutation suite.

## Gauntlet (one run after the last code edit)

Commands, from `/Users/gary/io/ax-ctxcache`, with `CARGO_TARGET_DIR=/Users/gary/io/ax/target-dev`:

- `cargo test -p ax-usage -p ax-mcp -p ax-policy -- --test-threads=8`
  - ax-mcp lib: 140 passed, 0 failed
  - ax-mcp other targets: 1 passed, 2 passed, 0 failed
  - ax-policy: 234 passed, 0 failed
  - ax-usage: 148 passed, 0 failed
- `cargo clippy -p ax-usage -p ax-mcp -p ax-policy -- -D warnings` — finished, no errors

Versions: rustc 1.98.0, cargo 1.98.0.

Skipped: cargo-mutants and `scripts/session-layer-gauntlet.sh` (user stopped mutants). Changed-line coverage (not in this spec's setup). Property tests (no new parser). Supply-chain audit (no new dependency). Live provider cached-input measurement (ax does not call the model).

## Limits

Ax still does not call the model, so there is no task checkpoint, no background summary, and no provider cached-input count. Notes and cached graph answers already live in `usage.db`. A bad child id on handoff uses the same error text as fork: `fork needs a new session id`.
