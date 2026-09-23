# SPEC: Review loop attached to old-coder (seeded globally)

Status: APPROVED rev 2 ("Approve, build it", 2026-09-23). Decisions (2026-09-23): no round cap; attach through the ax-owned `old-coder-mandatory` rule plus a new `review-loop` skill, leaving the vendored old-coder skill untouched; upgrade existing machines with a `seedVersion` bump that overwrites the seeded rule file; level CRITICAL, inside `old-coder-mandatory`.
Tier: 2. Policy text plus seeding code. It changes what every agent on every machine does after `ax install` / `ax init`.
Isolation: the existing checkout, as with the previous policy changes. Veto if you want a branch.
New dependencies: none.

## Problem

old-coder ends with GAUNTLET → EVIDENCE. There is no required code-review step, so stack review skills (`rust-review`, `typescript-review`, `dotnet-code-review`, …) are only used when someone asks. When a review does run, nothing makes the agent fix the findings and review again.

## Workflow (what agents must do)

The old-coder loop becomes: SPEC → RED → GREEN → REFACTOR → GAUNTLET → **REVIEW LOOP** → EVIDENCE. The review loop also runs whenever the user asks for a code review.

Each round:

1. **Skill check.** List the skills this review needs:
   - `old-coder`;
   - `old-coder-api` when HTTP/JSON surfaces changed;
   - one review skill per stack touched by the diff, e.g. `<stack>-review`, or a matching global review skill such as `dotnet-code-review`.

   Load each one with `ax_skill`. A skill counts as usable only when it returns a non-empty body. Record every skill as `usable`, `missing`, or `empty`. A missing stack skill is reported, and that stack gets the generic review checklist in the skill.
2. **Stack review.** Review only the changed code, against each usable stack skill. Every finding has:
   - an id;
   - a severity;
   - a `file:line`;
   - the skill section it comes from.
3. **Process findings.**
   - A behavioral finding starts with a RED test, then the fix.
   - Other findings are fixed directly.
   - A finding the agent believes is wrong is not dropped. It goes to the human with the reason.
   - After the fixes, the gauntlet runs again.
4. **Repeat.** If the round had findings, run steps 1–3 again on the new diff. The loop ends after the first round with zero findings.
5. **No round cap.** The loop continues until a round has zero findings. The only thing that pauses it is a disputed finding (step 3), which waits for the human's decision. The work is not done, and EVIDENCE is not written, while any finding is open.

EVIDENCE gets a "Review rounds" table: round, skills used (with status), findings, and what was fixed.

## Behaviors (tests)

- **R1:** The seeded `old-coder-mandatory` rule names the review loop in its required workflow and points to `ax_skill({ name: "review-loop" })`.
- **R2:** `seed_global_policy` writes `~/.ax/global_policy/skills/review-loop/SKILL.md`. `seed_cursor_skills` writes `~/.cursor/skills/review-loop/SKILL.md`.
- **R3:** The review-loop skill text contains the four steps (skill check, stack review, process findings, repeat until zero findings) and says there is no round cap. A test pins the key phrases so an edit cannot silently drop a step.
- **R4 Upgrade:** An existing `~/.ax/global_policy/rules/old-coder-mandatory.mdc` with a lower or missing `seedVersion` is overwritten on the next seed, including hand edits. A file with the current `seedVersion` is left alone, so a second seed changes nothing. The same applies to the seeded skill files.
- **R7 Level:** The review-loop requirement is part of the CRITICAL `old-coder-mandatory` rule, not a separate WARNING rule.
- **R5 global.db:** `ax install` / `ax init` also store the review-loop skill as a machine skill in `~/.ax/global.db` (`upsert_machine_skill`, which exists but has no caller yet). The global row then wins in every project. The call is idempotent: the same row is updated, not duplicated. A global.db failure does not fail install; it prints a note.
- **R6:** `ax_skill("review-loop")` returns the body in a project that has no local copy.

## Must not

- Change the vendored upstream old-coder `SKILL.md`.
- Overwrite a global rule file that already has the current `seedVersion`.
- Fail `ax install` / `ax init` when global.db is unavailable.

## Setup plan

- **New:**
  - `crates/ax-policy/templates/skills/review-loop/SKILL.md`
  - this spec, plus its EVIDENCE file
- **Edited:**
  - `crates/ax-policy/templates/rules/old-coder-mandatory.mdc`
  - `crates/ax-policy/src/seed.rs` (bundle, `seedVersion` upgrade, tests)
  - the global.db upsert on install/init: `crates/ax-cli/src/installer/mod.rs` and `crates/ax-cli/src/commands/init.rs`
  - docs: `site/src/content/docs/guides/policy-engine.md`
- **Real execution:** run `ax install --yes --target cursor`, then check `~/.ax/global_policy`, `~/.cursor/skills`, and the `global.db` row, and call `ax_skill("review-loop")`.
- **Git:** no commits unless you ask.
