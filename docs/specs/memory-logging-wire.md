# Logging page session/turn grouping

Status: Phase 3 Logging wiring. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Parsing a log line that contains `session=conv-1 turn=gen-1` sets `sessionId` to `conv-1` and `turnId` to `gen-1` on the TraceEntry. A line with neither field leaves both null. Grouping the display list with `groupBySessionThenTurn` yields session headers and nested turn groups in the Logging page.

## Known limits

- Keyword/trigger filters and links to memories/file changes/graph history are not in this slice. Backfill of legacy rows is a separate slice.
