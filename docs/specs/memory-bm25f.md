# BM25F field weights

Status: Phase 6 ranking slice. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Constants are k1=1.2 and b=0.75. A unit title score combines to a higher BM25F total than a unit body score. A zero term frequency scores 0.

## Known limits

- Rebuilding FTS5 over `search_text` with these weights and wiring into `recall` are not in this slice.
