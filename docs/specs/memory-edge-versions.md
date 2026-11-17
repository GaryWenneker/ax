# Graph edge versions

Status: next slice of Phase 4 graph history. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Recording edge `a → b` kind `calls` with session `conv-1` and turn `gen-1` inserts one `graph_edge_versions` row at version 1. Recording the same edge again under a later turn inserts version 2 and closes version 1 with `valid_to`. An empty source or target writes nothing.

## Known limits

- Sync capture, as_of queries, the Graph page slider, and zstd body compression are not in this slice.
