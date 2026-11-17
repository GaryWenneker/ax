# FTS rebuild over search_text

Status: Phase 6 remaining slice. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Schema version 44 recreates `memories_fts` with a `search_text` column and rebuilds content from `memories`.
2. FTS ranking uses weighted `bm25(memories_fts, …)` so the title column outweighs the body column (same order of magnitude as `bm25f_combine` weights).

## Known limits

- Snowball stemming and PMI expansion remain separate Phase 6 modules.
