# Memory anchors to graph versions

Status: first slice of Phase 7. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Anchoring memory `m1` to node key `kept` at version 1 inserts one `memory_anchors` row. Anchoring the same pair again does not duplicate. A blank memory id or blank node key writes nothing.

## Known limits

- Auto-attach from turn files, stale/orphan flags, rename links, and `as_of` recall are not in this slice.
