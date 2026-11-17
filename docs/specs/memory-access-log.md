# Memory access log

Status: Phase 6 usage memory slice. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Recording an access for memory `m1` with kind `recall` at time 10 inserts one `memory_access` row. Listing access times for `m1` returns `[10]`. A blank memory id writes nothing.

## Known limits

- Wiring into recall/preflight/guard and useful/wrong feedback are not in this slice.
