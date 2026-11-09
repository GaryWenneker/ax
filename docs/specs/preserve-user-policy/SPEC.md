# SPEC: Preserve user-customized rules and skills

**Status:** approved (user: "approve", 2026-11-09)  
**Tier:** 2  
**Isolation:** work on current branch (behavior change in ax-policy / ax-cli / ax-global-db)

## Problem

After a first install, users customize seeded rules/skills (e.g. PR comment language). Reinstall, `ax init`, global seed upgrades, or `ax policy sync --fix` overwrite those files with embedded templates.

## Acceptance criteria

### A1 — Global filesystem seed never clobbers existing files

Given `~/.ax/global_policy/rules/<id>.mdc` (or skill `SKILL.md`) already exists with custom content  
When `seed_global_policy()` runs with `force=false` (install/init default)  
Then the file content is unchanged (even if template `seedVersion` is higher or markers differ).

### A2 — Global seed creates missing files

Given the destination file is absent  
When `seed_global_policy()` runs  
Then the embedded template is written.

### A3 — Project sync --fix restores missing only

Given a managed project skill/rule was deleted  
When `sync_instructions(ax, fix=true, force=false)`  
Then the missing file is restored from the template.

Given a managed file exists but was hand-edited (drift)  
When `sync_instructions(ax, fix=true, force=false)`  
Then content is unchanged; the check reports issues / fail (no silent rewrite).

### A4 — Explicit --force restores templates

Given a drifted managed file  
When `sync_instructions(ax, fix=true, force=true)` (CLI: `ax policy sync --fix --force`)  
Then content matches the embedded template again.

### A5 — IDE bootstrap respects the same contract

`sync_ide_bootstrap` / `seed_ide_agent_workflow`: with `force=false`, existing IDE rule files are not overwritten (including “stale” bootstrap). With `force=true`, current upgrade/overwrite behavior remains available.

### A6 — global.db company skills insert-if-absent

Given skill `review-loop` (or other `global_db` seed) already exists in `global.db` with a customized body  
When `store_seeded_skills_in_global_db` runs  
Then the existing payload is not replaced.

### A7 — English product default for review comments

Unset `reviews.commentLanguage` remains English (already true). Seeded `english-only` template must not hardcode “PR comments are Dutch”; it must defer to `reviews.commentLanguage` / `dutch-pr-comments` wording. Docs state create-missing + `--force`.

## Must NOT

- Change unrelated installer self-heal behavior.
- Move or delete `.ax` folders.
- Overwrite without `--force`.

## Setup

- No new dependencies.
- Gauntlet: `cargo test -p ax-policy --lib seed::` and related init/global-db tests; update tests that expected silent overwrite.
- Docs: policy-engine + troubleshooting (English).

## Mapping (to be filled in EVIDENCE)

| Scenario | Test |
|----------|------|
| A1 | `seed_global_keeps_hand_edited_rule` |
| A2 | existing create tests |
| A3 | `sync_fix_restores_missing_*`; `sync_fix_keeps_drifted_*` |
| A4 | `sync_force_restores_drifted_*` |
| A5 | IDE seed tests updated |
| A6 | init store skills test |
| A7 | english-only template assert / review_language unset |
