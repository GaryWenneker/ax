# Memory FTS reindex and search_text backfill

Status: proof-bar fix for live delivery. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. After a memory titled `Blossom Suno recipe` is written, then removed from `memories_fts` with a delete command, `recall("Blossom Suno")` returns no rows. Running `reindex_memory_fts` backfills empty `search_text` and rebuilds FTS. The same recall then returns that memory id.

## Known limits

- Does not stop live writers. A concurrent insert during rebuild can leave one row briefly stale until the next update trigger fires.
