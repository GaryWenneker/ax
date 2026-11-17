# Full memory JSONL backup

Status: Phase 4 backup slice. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Writing a full backup for a vault that has one remembered note creates `.ax/backups/memory/full-<timestamp>.jsonl` with one JSON object that includes that memory's id and title, plus a sibling `.manifest.json` with `count` 1 and a blake3 checksum of the JSONL bytes. An empty vault still writes the files with `count` 0.

## Known limits

- zstd compression, incremental daily snapshots, depth=backup recall, and restore are not in this slice.
