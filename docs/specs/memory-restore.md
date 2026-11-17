# Restore memory from JSONL backup

Status: Phase 4 restore slice. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Restoring a JSONL backup that contains one memory inserts that memory when it is absent. Restoring again does not duplicate it. A tombstoned id is skipped and not restored.

## Known limits

- Cue/vector re-registration beyond remember_with_id defaults, and MCP/CLI restore actions, are not fully covered in this slice beyond the library call.
