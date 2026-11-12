# SPEC: brain-memory logic to 100%

Approved by the request "continue, make it 100%" against the six gaps listed at 74% (17/23).

Isolation: the unfinished memory work already lives in this working tree. A fresh worktree would not contain it.

## Behaviors

1. A git capture and a folder doc are tier `t3`. Hot recall and preflight omit them. `depth=archive` returns them.
2. Archive free-text search adds one hop: a `memory_links` neighbor, a memory that shares a file path, and a memory that shares a `source=symbol` cue. A memory with none of those stays out. The hop does not run on hot search.
3. Passing an open file raises a T2 memory's score by `WEIGHT_GRAPH * 100` when a symbol in that file is on the memory's cue set, even if the memory does not list the file.
4. Storing a memory whose files contain a graph file registers that file's symbol names as cues with `source=symbol`.
5. `consolidate` turns a git memory whose body says decided, decision, or because into a candidate decision linked with `kind=source`. Preflight does not inject the candidate.
6. `reembed_stale` rewrites rows whose `embedding_model` is not the active model, then indexes archive rows. Sync calls it.
7. Existing `git` and `doc` rows at tier `t2` become `t3` in schema version 26.
8. The preflight title block stops at 6000 bytes.

## Must not change

- Cue lookup, id lookup, and hot search still skip the archive.
- Demotion still does not delete.
- No new MCP or CLI flag. Both surfaces use the same search function.
- Product name stays ax. No version pin outside `crates/ax-cli/Cargo.toml`.
