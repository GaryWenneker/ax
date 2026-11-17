# ACT-R base-level activation

Status: Phase 6 ranking slice. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. With decay 0.5, a single access 1 second ago scores higher base-level activation than a single access 1 day ago. An empty access history returns negative infinity.

## Known limits

- `memory_access` log writes, replacing `updated_at` decay in recall, and useful/wrong feedback are not in this slice.
