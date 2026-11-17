# File change memories

Status: first slice of Phase 4. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Saving file changes for paths `src/a.rs` and `src/b.rs` under session `conv-1` and turn `gen-1` inserts two memories with kind `file_change`. Each memory's `files` list holds exactly that path. Both rows carry session `conv-1` and turn `gen-1`. An empty path list writes nothing.

## Known limits

- Hunks, line counts, before/after hashes, zstd, binary handling, and sync capture are not in this slice.
- The stop hook wires this next; the library API is the contract here.
