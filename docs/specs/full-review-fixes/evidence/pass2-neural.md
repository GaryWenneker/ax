# Second pass: neural

Branch `fix/review2-neural`, from `origin/main` at `de7f8e0e`. It should cover the neural findings the first pass left open and the owner decisions of 28 November 2026 for neural-002 and neural-003.

**This pass did not run.** The worktree for this branch could not be checked out because the disk was full. No code was changed and no tests were written or run. This file records that, so the open items are not lost. The work still has to be done in a new run.

## Findings

| Id | State | Tests | RED seen |
|---|---|---|---|
| neural-002 | not done (blocked: no worktree) | none | no |
| neural-003 | not done (blocked: no worktree) | none | no |
| neural-029 | open, owner decision still needed (unchanged from pass 1) | none | no |

### What is still to do

- **neural-002 (owner decision, 28 November 2026).** Pass 1 made promote read the bake-off evidence registered with the model version. The owner chose a stricter rule: promotion is allowed only for a model ax can measure itself, which is the installed ONNX model with a matching checksum, run through the bake-off. Any other registered model must be refused with a clear message. The caller-supplied `beatsBaseline` / `regressionsClear` flags must be gone from dispatch, the ax-cli Promote command and the ax-mcp tool schema (pass 1 says they are already removed; check this). Document it on the neural docs page.
- **neural-003 (owner decision, 28 November 2026).** Add a held-out set of paraphrase questions that were not used to write the synonym table. A test measures the lexical challenger against the real measured baseline on that set, with no hard-coded number. If the challenger does not beat the baseline on the held-out questions, it is not admitted in production: search, status, model_list and dispatch fall back to the baseline.
- **neural-029.** Pinning the ONNX model checksum to a known build still needs an owner decision on which build to accept. It may overlap with neural-002, because that rule needs a "matching checksum".

## Review rounds

| Round | Findings |
|---|---|
| 1 | 0 |

Round 1 reviewed this branch's diff against `origin/main`. The diff holds only this file, so there was no code to review. Zero findings here says nothing about the neural code.

## Commands run

None. No build or test ran for this branch.
