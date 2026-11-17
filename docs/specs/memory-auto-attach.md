# Auto-attach memory anchors

Status: Phase 7 remaining slice. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Auto-attaching memory `m1` that lists file `src/a.rs` anchors it to every current graph node version for that path. A memory with no files writes nothing.
2. Marking stale: when an anchor's stored version body_hash differs from the live node's current body fingerprint, `stale` becomes 1. When the node key is gone, `orphan` becomes 1.

## Known limits

- Rename re-anchor links and inject title suffixes remain separate Phase 7 follow-ups.
