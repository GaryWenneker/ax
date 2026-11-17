# Bug-fix lesson extractor

Status: Phase 8 extractor slice. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. Extracting from an episode goal `null panic in parse` and outcome `green after guard` yields a lesson with symptom containing `null`, fix hint non-empty, and kind `bug_fix`. An empty goal yields none.

## Known limits

- Writing the lesson into the vault, red→green file-change correlation, and reconciliation are not in this slice.
