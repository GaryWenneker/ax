# Graph history

Status: first slice of Phase 4 graph history. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Recording node version `kept` with body `pub fn kept() {}`, session `conv-1`, and turn `gen-1` inserts one `graph_node_versions` row at version 1 and one `graph_body_blobs` row for that body's hash. Recording the same body again under a later turn inserts version 2 that reuses the same body hash and does not duplicate the blob. An empty node key writes nothing.

## Known limits

- Edge versions, as_of queries, the Graph page slider, and sync capture are not in this slice.
