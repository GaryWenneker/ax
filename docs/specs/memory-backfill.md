# Legacy session backfill

Status: Phase 3 backfill scaffolding. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. A turn memory whose body has `Conversation: deadbeef0012` and blank `session_id` receives `session_id` `deadbeef0012` after backfill. A note with blank session and no conversation line receives `session_id` `Unlinked` and `turn_id` `Unlinked`. Rows that already have a session id are left unchanged. The function returns how many rows were updated.

## Known limits

- Tool-call time-window linking and Logging-page navigation from backfilled rows are not in this slice.
