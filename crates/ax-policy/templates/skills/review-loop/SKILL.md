---
name: review-loop
description: Mandatory code-review loop for old-coder work and every code review request. Check which review skills are usable, review the change per stack, fix every finding, and review again until a round has zero findings.
alwaysApply: false
triggers: ["code review", "review", "findings", "old-coder", "gauntlet", "evidence", "pr", "pull request"]
tags: ["old-coder", "review", "quality"]
priority: 85
scope: company
seedVersion: 3
---

# Review loop

The old-coder loop is SPEC → RED → GREEN → REFACTOR → GAUNTLET → **REVIEW LOOP** → EVIDENCE. Run this loop after the gauntlet is green and before EVIDENCE. Also run it whenever the user asks for a code review of their own change. For a colleague's pull request, load `pr-review-comments` instead: findings become proposed comments that the user approves one by one, and you do not fix them.

Every round has four steps.

## 1. Skill check

List the skills this review needs:

- `old-coder`, always.
- `old-coder-api` when an HTTP/JSON surface changed.
- One review skill for each stack the diff touches: `<stack>-review` (for example `rust-review`, `typescript-review`, `react-review`), or a global review skill for that stack (for example `dotnet-code-review`).

Find the stacks from the changed files and `ax.json` `policy.stacks`. Load each skill with `ax_skill({ name })`. Record every skill as:

- `usable`: the call returned a non-empty body.
- `missing`: no skill with that name exists.
- `empty`: the call returned no body, or only a stub.

Only `usable` skills count. Report `missing` and `empty` skills to the user. A stack without a usable review skill still gets reviewed, against the generic checklist below, and EVIDENCE says so.

## 2. Stack review

Review only the change: the diff and the code it directly calls or is called by (use `ax_callers` / `ax_callees`). Check it against every section of each usable stack skill.

Write each finding as:

- an id (`R<round>-<n>`);
- a severity: `blocker`, `major`, or `minor`;
- the location as `file:line`;
- the skill and section it violates;
- one sentence on what is wrong.

Generic checklist, for stacks without a skill:

- errors are handled or propagated, never swallowed;
- names match the surrounding code;
- no dead code or leftover debug output;
- public behavior changed only where the SPEC says so;
- every new branch has a test.

## 3. Process findings

Fix every finding in this round before the next round:

- A behavioral finding follows old-coder: first a RED test that shows the problem, then the fix, then the full suite.
- A style, naming, or documentation finding is fixed directly.
- A finding you believe is wrong is not dropped. Tell the user which finding, and why, and wait for their decision. Only the user can close a disputed finding.

After the fixes, run the gauntlet again. Never weaken a test to close a finding.

## 4. Repeat

If this round had any findings, start a new round at step 1 on the updated diff. The skill check runs again, because the fixes can touch a new stack.

The loop ends after the first round with **zero findings**. There is no round cap: keep going until a round is clean. The work is not done, and EVIDENCE is not written, while any finding is open.

## Production build

When the diff touches a bundled frontend, rule `frontend-production-build` applies. Run that package's production build (`pnpm run build` when that is the script) and record the exit code in EVIDENCE. A passing unit test does not cover an importer that no test loads. `UNRESOLVED_IMPORT` from the bundler is an open finding until the build exits 0.

## EVIDENCE

Add a **Review rounds** table to EVIDENCE:

| Round | Skills (status) | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder` usable, `rust-review` usable | 3 (1 major, 2 minor) | R1-1 … R1-3 |
| 2 | same | 0 | — |

The last row must show 0 findings.
