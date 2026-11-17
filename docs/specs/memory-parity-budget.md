# Parity at the reserved budget

Spec approval: not obtained (autonomous run). The approved plan plus "continue" authorized this slice. Isolation: the working tree.

Tier 2.

## Scenarios

1. The parity preflight sets `context.budgetTokens` to 300. The cue hit still comes first on the CLI and on MCP.
2. A demoted turn stays out of preflight at that budget and is still fetched by id and by cue on the library, the CLI, and MCP.
3. Confirm activates a candidate that the test plants. `consolidate` does not create that candidate.

## Evidence

Recorded after the last code edit. Spec approval: not obtained (autonomous run).

- `cargo test -p ax-cli --offline --test memory_parity -- --test-threads=8`: 6 passed, 0 failed.
- `cargo clippy -p ax-cli --offline --test memory_parity -- -D warnings`: exit 0. Pre-existing unused workspace `gix` and `self_update` warnings, plus the pre-existing ax-cli linker warning.

The confirm helper used to ask `consolidate` for a candidate. That function no longer creates one, so the helper now stores the candidate itself. The confirm assertions are unchanged.
