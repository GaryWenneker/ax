# Evidence: call edges through Rust macro arguments, and history by commit subject

Spec: `docs/specs/macro-calls-history-subject.md`. Spec approval: not obtained.

- Date of the run: 2026-11-27.
- Source state: commit `cb48c4a2` plus the changes of the commit that carries this report
  (the run saw them as uncommitted files; nothing was edited after it).
- Toolchain: rustc 1.100.0, Python 3.15.2, macOS.
- Rerun: `bash tools/gauntlet-graph-history.sh` from the repository root.

All numbers come from that one run.

## Scenarios

| Scenario | Test |
|---|---|
| M1 call in a `println!` argument, with same-file edge | `rust::macro_call_tests::m1_…` |
| M2 nested macros and call arguments | `m2_…` |
| M3 method and path calls | `m3_…` |
| M4 plain tokens, macro names, `fn` definitions, `[..]` and `(..)` without a callee | `m4_…` |
| M5 enclosing function and line | `m5_…` |
| H5 topic lists the commit, any letter case | `tests/history.rs::h5_…` |
| H6 a missing word excludes the commit | `h6_…` |
| H7 `since` and `limit` | `h7_…` |
| H8 no significant word, or no repository | `h8_…` |
| Both former known gaps, through the real binary | e2e rows "callers through a macro argument" and "history by topic word", full and slim |

RED before the fix: M1, M2, M3, M5, H5 and H7 failed. M4, H6 and H8 describe what must
not happen and passed before the fix; the mutants below show they can fail.

## Gauntlet

| Layer | Command | Result |
|---|---|---|
| Tests | `cargo test -p ax-extraction -p ax-memory -p ax-core -p ax-mcp -p ax-cli` with an empty `HOME` | 60 suites ok, 707 tests passed, 0 failed |
| Lint | `cargo clippy` (same crates) `--all-targets -- -D warnings` | clean |
| Format | `rustfmt --check` on the 5 Rust files this spec changed | clean |
| Mutation | `tools/graph_history_mutants.py` | 10 of 10 killed; the no-op control survived, as it must |
| Release build | `cargo build --release -p ax-cli` | ok, 165,718,592 bytes |
| End-to-end, full build | `tools/ax_agent_e2e.py target-dev/release/ax` | 49 passed, 0 failed, 0 known gaps |
| End-to-end, slim build | `cargo build --profile release-slim -p ax-cli --no-default-features --features "minimal-grammars mimalloc"`, then the same script | ok, 31,409,568 bytes; 49 passed, 0 failed, 0 known gaps |
| Coverage | skipped: no coverage tool is configured in this repository |
| Property tests | skipped: none written |

## Upgrade of an existing index (follow-up run)

- Date of the run: 2026-11-28, after the merge (main at `73ca661b`).
- Project: a local clone of this repository at `73ca661b`, with an empty `HOME`.
- Old build: the release binary from before this change. New build: the release binary of
  the run above.
- Steps: `ax init` with the old build, then `ax sync` with the new build on the same
  `.ax`, then `ax sync` again. Edge counts come from the `edges` table.

| | Old build | After `ax sync` with the new build |
|---|---|---|
| Code files indexed | 1,468 | 1,468 (all re-indexed) |
| Nodes | 16,728 | 16,728 |
| `calls` edges | 59,794 | 68,964 (+9,170, about 15%) |
| `contains`, `covers`, `documents`, `references` edges | 20,327 | 20,327 |
| Unresolved references | 82,525 | 88,459 (+5,934) |
| Wall time | 3 min 41 s (`ax init`) | 3 min 41 s (`ax sync`) |

- The first `ax sync` with the new build re-indexed every file without a `--force`; the
  version bump works as an upgrade path. The second `ax sync` reported "Already up to
  date" in 0.1 s.
- Of the 68,964 `calls` edges after the upgrade, 20,959 are `extracted` and 48,005
  `inferred` (matched by name). The split of the 9,170 new edges was not measured.

## Not verified

- Review loop: not run. No independent review round was made on this change.
- Precision of the new call edges: no sample of the 9,170 added edges was checked for
  wrong targets.

## Known limits (from the spec)

- Macros are not expanded; calls a macro generates are not found.
- A call with a turbofish inside a macro argument is not found.
- Topic commits must contain every significant word; there is no relevance ranking.
