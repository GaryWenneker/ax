# Memory scope guard

Status: first slice of Phase 5. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Writing a memory whose files list only paths under the project root is allowed. Writing a memory whose files list includes `/other/project/src/a.rs` is rejected with an error that names the right `projectPath`. An empty files list is allowed.

## Known limits

- MCP client roots, provenance, trust levels, purge, and the single redactor are not in this slice.
