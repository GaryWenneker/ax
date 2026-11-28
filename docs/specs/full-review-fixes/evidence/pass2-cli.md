# Evidence: second pass, fix/review2-cli (ax-cli)

Base: `origin/main` at `de7f8e0e`. Scope: `crates/ax-cli`, the items left open by the first pass (`evidence/cli.md`).

**No code was changed in this pass.** The worktree `/Users/gary/io/ax-wt/p2-cli` could not be created because the disk was full (160 MiB free on the data volume). No build or test could run. This commit only records the state, so the open items stay visible.

## Worklist findings

| Finding | State | RED seen | Test(s) | Commit |
|---|---|---|---|---|
| cli-024 | Not done: owner decision made (per-node credential at enrollment, bootstrap token for enrollment only, re-pair required, revoke invalidates, docs on the replication page), not implemented | n/a | n/a | n/a |
| cli-031 | Not done (disk full) | n/a | n/a | n/a |
| cli-032 | Not done (disk full) | n/a | n/a | n/a |
| cli-033 | Not done (disk full) | n/a | n/a | n/a |
| cli-034 | Not done (disk full) | n/a | n/a | n/a |
| cli-035 | Not done (disk full) | n/a | n/a | n/a |
| cli-036 | Not done (disk full) | n/a | n/a | n/a |
| cli-037 | Not done (disk full) | n/a | n/a | n/a |
| cli-038 | Not done (disk full) | n/a | n/a | n/a |
| cli-039 | Not done (disk full) | n/a | n/a | n/a |
| cli-040 | Not done (disk full) | n/a | n/a | n/a |
| cli-041 | Not done (disk full) | n/a | n/a | n/a |
| cli-042 | Not done (disk full) | n/a | n/a | n/a |

Also open for ax-cli: neural-002 asks to remove the caller-supplied `beatsBaseline` / `regressionsClear` flags from the ax-cli `Promote` command. Not done here.

## Review rounds

| Round | Findings |
|---|---|
| 1 | 0 |

Round 1 had no code diff to review, so zero findings says nothing about the open items above.

## Commands run

None. No build, test, clippy or fmt run was possible without a worktree.

## Behavior changes

None. No site docs page changed.
