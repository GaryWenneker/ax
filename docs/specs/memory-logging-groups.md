# Logging session and turn groups

Status: first slice of Phase 3 Logging page. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Grouping events with session `conv-1` turn `gen-1`, then `conv-1` turn `gen-2`, then another `conv-1` turn `gen-1`, then a blank session and turn, yields two session groups. The first session has turn `gen-1` with two events and turn `gen-2` with one event. The blank ids become session `Unlinked` and turn `Unlinked`.

## Known limits

- Chips, keyword/trigger filters, and links to memories are not in this slice. The helper is pure; wiring into `McpTraceLive` is next.
