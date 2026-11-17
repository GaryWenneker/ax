# Indexer sync capture wiring

Status: Phase 4 remaining slice. Spec approval: not a separate review. The plan was started with "go".

## Scenario

1. After an incremental sync with affected paths, `finalize_after_extract` captures one `file_change` memory per path under the current session/turn from `.ax/turns/current.json` when that file exists; without current.json it still writes using empty session/turn ids only when a session is present — otherwise it uses `sync` / `index` placeholders so history is not lost.
2. For each affected path that still has nodes, one graph node version is recorded when the body's hash differs from the latest stored version.
3. Capturing the same path twice in the same session/turn does not duplicate the file_change memory (INSERT OR IGNORE).

## Known limits

- Hunk bodies with zstd in file_change rows and rename detection by body hash remain follow-ups inside Phase 4.
