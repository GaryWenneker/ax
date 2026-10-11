# SPEC: Policy rules/skills in global.db vs project ax.db

**Tier:** 2  
**Spec approval:** not obtained (autonomous run — user: rules/skills must also be divided under global vs project db)  
**Isolation:** current working tree. No new cargo dependencies.

Absolute path: `/Users/gary/io/ax/docs/specs/policy-global-db.md`

## Problem

Graph nodes already copy into `~/.ax/global.db`. Policy rules and skills stay only in the current project `ax.db`, so Command Center cannot show other projects’ policy.

## Must survive

- `GET /api/policy/rules` and `/skills` keep existing item fields (additive only).
- Matcher/preflight and `ax_skill` merge project `ax.db` with `~/.ax/global.db` (`global_policy_skills`). A **global** row with the same name wins.
- Zip pack / enable / delete on **project** rows still use ax.db. Global rows can now be opened and saved via GET/PUT `?origin=global`.
- Existing `ax global` tests G1–G4, G6.

## Behaviors

### P1 — Sync copies policy tables

Given a project `ax.db` with one `policy_rules` row `id=foo` and one `policy_skills` row `name=bar`  
When `sync_project` runs  
Then `global_policy_rules` / `global_policy_skills` contain those items for that project.

### P2 — Missing policy tables are not an error

Given a project db with nodes but no `policy_rules` table  
When `sync_project` runs  
Then it still succeeds (same as today’s node sync).

### P3 — List API annotates origin

Given Command Center on project A, global.db has policy from A and B  
When `GET /api/policy/rules`  
Then A’s rows have `origin=project`, `selected=true`.  
B’s rows have `origin=global`, `selected=false`, distinct `rowKey`.  
A’s rows are not duplicated from global.

### P4 — UI

Rules and Skills tables show a **DB** column: `Project` vs `Global`. The source project stays in the tooltip, not on the gold chip.  
Filter: All / This project / Other projects.  
Global rows can be opened and saved in Command Center (GET/PUT `?origin=global&projectId=` writes `global.db` only). Enable/storage still apply to this project’s ax.db rows.

### P5 — Move to global.db does not blank the list

Given Skills overview with at least one project skill  
When the user picks **Move to global.db** from the row menu  
Then `POST /api/policy/relocate` stores **flat** list JSON (top-level `name`/`id`, `tags` array)  
And the next `GET /api/policy/skills` still returns every remaining skill plus the moved row as `origin=global`  
And the table still renders (no uncaught `name.trim` / `localeCompare` crash).

### P6 — Edit a global copy

Given a skill (or rule) that exists only in `global.db` for this project’s `projectId`  
When Command Center GET `/api/policy/skills/{name}?origin=global&projectId=`  
Then it returns 200 with frontmatter+body (not 404).  
When PUT the same URL with an updated body  
Then `global_policy_skills` for that project_id is updated and project `ax.db` is unchanged.

### P7 — Agents load global-only skills

Given `azdo-pr-review` exists only in `global.db`  
When `ax policy skill azdo-pr-review` or `ax_skill` runs in another project  
Then the skill body is returned (a **global** row with the same name wins).

- Files: schema SQL, `ax-global-db` sync + list, `ax-web` list handlers + web-ui tables, policy-engine.md.
- Gauntlet: `cargo test -p ax-global-db`, web-ui `npx vitest` for filter helper if added.
