# Sync file-change capture

Status: Phase 4 sync capture slice. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Capturing path `src/a.rs` when the previous stored hash differs from the new hash writes one `file_change` memory for that path under the given session and turn. Capturing when the hashes are equal writes nothing. A blank path writes nothing.

## Known limits

- Wiring into the live indexer, hunks/zstd, and graph version capture from the sync change set are not in this slice.
