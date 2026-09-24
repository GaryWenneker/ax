# SPEC: git hooks that actually run (memory capture), release v5.0.3

**Status:** APPROVED ("Approve, build it and release v5.0.3", 2026-09-23)
**Tier:** 2 (bug fix). The hooks run shell commands after every commit, merge, and checkout, so a wrong fix can break a user's git workflow.

## Problem

`install_git_sync_hooks` (`crates/ax-sync/src/git_hooks.rs`) writes `.git/hooks/post-commit`, `post-merge`, and `post-checkout` with `fs::write`. It writes no `#!/bin/sh` line and never sets the execute bit. On macOS and Linux the files are created as `-rw-r--r--`, and git skips them ("hook was ignored because it's not set as executable"). As a result `ax sync`, `ax ship --evaluate`, and `ax capture-git` have not run after a commit in this repo since 2026-08-26. The newest git memory is from 2026-08-20; 17 commits since then are uncaptured.

Windows is not affected: Git for Windows runs hooks through its own `sh` without checking the execute bit.

## Behaviors (each becomes a test)

| # | Given | When | Then |
|---|---|---|---|
| H1 | no hook file | `install_git_sync_hooks` | file starts with `#!/bin/sh\n`, then the ax lines; on Unix the mode includes `0o755` |
| H2 | ax hook without a shebang, mode `0644` (the broken state) | `install_git_sync_hooks` | `#!/bin/sh` is prepended, existing lines are kept in order, mode includes `0o755` on Unix |
| H3 | user hook with its own shebang (`#!/usr/bin/env bash`) and custom lines | `install_git_sync_hooks` | the user's shebang and lines are kept, missing ax lines are appended, no second shebang, mode includes `0o755` on Unix |
| H4 | hook already complete: shebang, all ax lines, executable | `install_git_sync_hooks` | file bytes and mode are unchanged |
| H5 | hook with every ax line but no shebang and mode `0644` | `install_git_sync_hooks` | repaired as in H2 (today this case is skipped because the content check passes) |
| H6 | broken ax hook (H2 state) | `repair_git_hooks` | repaired as in H2 |
| H7 | hook without any ax line (user-only hook), mode `0644` | `repair_git_hooks` | file and mode unchanged (repair never touches hooks ax did not write) |
| H8 | no `.git/hooks` directory | either function | `Ok(())`, nothing created |
| H9 | MCP stdio server starts for an initialized project | `run_stdio_server` | calls `repair_git_hooks(project_root)` once; an error is logged, never fatal |
| H10 | real repo on macOS | `ax init`, then `git commit` | git runs the hook (no "ignored" hint) and a new `git-<sha>` memory exists |

## Must not change

- The set and order of ax lines per hook (`hook_lines` tests keep passing unchanged).
- `remove_git_sync_hooks` still removes only ax lines. A leftover file containing just the shebang is harmless and is left in place.
- Windows: no permission call (`#[cfg(unix)]`); the shebang is still written (Git for Windows reads it).
- Hooks without ax lines are never modified by the repair path.

## Known limit

A hook runs with the PATH of the program that makes the commit. If `ax` is not on that PATH (some GUI git clients), the hook runs but `ax` is not found. This spec does not change that; it is noted in EVIDENCE.

## Setup plan

- **Isolation:** a branch `fix/git-hook-exec` in this repo. A worktree would lack `target-dev/` and `node_modules`, and a rebuild costs more than it protects for a two-file change.
- **Files:** `crates/ax-sync/src/git_hooks.rs` (code and tests), `crates/ax-mcp/src/server.rs` (one repair call), `docs/specs/git-hook-exec-EVIDENCE.md`, version files for v5.0.3, `site/src/content/docs/getting-started/introduction.md` and `README.md` (what's new).
- **Dependencies:** none new (`std::os::unix::fs::PermissionsExt` is in the standard library).
- **Git:** commit this spec at approval, one commit for the fix, one release commit, tag `v5.0.3`.
- **Local repair after the fix:** run the repaired installer on this repo (via `ax init`, which only adds missing lines, or the MCP restart), then `ax capture-git --limit 100` to backfill the 17 missed commits.

## Revision 1 (2026-09-23): quiet hooks — APPROVED ("Approve Revision 1")

Found during real execution (H10): once the hooks run, every commit prints about 50 lines of `ax ship --evaluate` JSON plus the ONNX model's INFO log lines. You chose "make them quiet". This changes the "Must not change: the set of ax lines" clause above for one line.

| # | Given | When | Then |
|---|---|---|---|
| Q1 | a passing quality gate | `ax ship --evaluate --quiet` | prints nothing on stdout or stderr, exit 0 |
| Q2 | a failing quality gate | `ax ship --evaluate --quiet` | prints one line on stderr, `ax: quality gate failed: <step>, <step>`, exit 0 (a post-commit hook cannot block anyway) |
| Q3 | any command run with `--quiet` | the CLI starts | ax log lines below WARN are not printed (so `ax capture-git --quiet` no longer prints the ONNX INFO lines); without `--quiet` nothing changes |
| Q4 | new hooks | `install_git_sync_hooks` | the ship line is `ax ship --evaluate --quiet` |
| Q5 | an existing ax hook with the exact line `ax ship --evaluate` | `install_git_sync_hooks` or `repair_git_hooks` | that line is replaced in place by `ax ship --evaluate --quiet`; the gate is never listed twice |
| Q6 | a user line that merely contains `ax ship --evaluate` plus other flags | either function | left as it is |

Extra files: `crates/ax-cli/src/main.rs` (the `--quiet` flag on `ship` and the log level), the ship evaluate command file in `crates/ax-cli/src/commands/`, and `site/src/content/docs/reference/cli.md` (document `--quiet`).

## Gauntlet

- `cargo test -p ax-sync -p ax-mcp`, and the full workspace suite against the known baseline (4 known failures: 3 Windows-only, 1 flaky `ax-usage` test).
- `cargo clippy` on the changed crates, no new warnings on changed lines.
- Mutation: hand-written mutants in `git_hooks.rs` (drop the shebang, drop the chmod, repair touches non-ax hooks, H4 rewrites anyway, H5 skip kept), each must be killed.
- Real execution: H10 in a temp repo, plus this repo's next commit.
- Review loop until a round has zero findings, then EVIDENCE.
