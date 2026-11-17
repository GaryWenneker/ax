# Memory provenance and trust

Status: Phase 5 provenance slice. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Remembering a note with source `mcp` stores `trust_level` `trusted` and `origin` `mcp`. Remembering a doc with source `import` or kind `doc` stores `trust_level` `untrusted`. An empty id is never written.

## Known limits

- Instruction-like flagging and quoted injection formatting are not in this slice. Author and commit stamping from git are next.
