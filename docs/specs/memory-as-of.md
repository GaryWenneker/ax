# Graph history as_of

Status: Phase 4/7 as_of slice. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. After recording node `kept` at valid_from 10 and again at 20, `node_as_of(kept, 15)` returns version 1 and `node_as_of(kept, 25)` returns version 2. A time before any version returns none.

## Known limits

- MCP/CLI `as_of` flags and Graph page slider are not in this slice.
