# Memory purge with tombstone

Status: Phase 5 purge slice. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Purging memory id `keep-me` that has a cue and a `follows` link removes the memories row, its cues, and its links, and inserts one `memory_tombstones` row for `keep-me`. A second purge of the same id returns false and does not add another tombstone. Purging an unknown id returns false and writes no tombstone.

## Known limits

- Backup entry and export scrubbing are not in this slice. Restore refusing a tombstoned id is the next slice.
