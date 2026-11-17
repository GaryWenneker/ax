# Rename re-anchor

Status: Phase 7 remaining slice. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Given two node keys whose latest body hashes are equal and a memory anchored only to the old key, `reanchor_renames` moves the anchor to the new key and inserts a `renamed` memory link between a synthetic pair id of the old and new keys when both memories exist — for graph-only renames it updates the anchor row and returns the count of moved anchors.

## Known limits

- Detecting renames from the live indexer change set remains tied to equal body hashes only.
