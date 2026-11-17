# Normalized search_text

Status: first slice of Phase 6. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Normalizing `parseHttpResponse_from_path` yields tokens that include `parse`, `http`, `response`, `from`, and `path` (camelCase and snake_case split). Paths like `src/FooBar.rs` contribute `src`, `foo`, `bar`, and `rs`. Uppercase folds to lowercase. Empty input yields an empty string.

## Known limits

- Snowball stemming, BM25F FTS rebuild, lexicon, and ACT-R are not in this slice.
