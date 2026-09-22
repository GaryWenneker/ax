# SPEC: Policy overview context menus

Tier 2. Spec approval: not obtained (autonomous run). User asked for context menus on Rules/Skills overviews including move to global.db and back.

## API (internal Command Center)

`POST /api/policy/relocate`

```json
{ "kind": "rule" | "skill", "id": "utf8-no-bom", "to": "global" | "project", "projectId": 1 }
```

- `to: "global"`: upsert payload into `global_policy_*` for this workspace’s `projects` row, then delete from this project’s `ax.db` (and files via existing store delete). 404 if not in the project store.
- `to: "project"`: load payload from global.db (`projectId` or this workspace), `save_rule`/`save_skill` into this project, then delete that global row. 404 if missing. 409 if the id already exists in the project store.
- Readonly hub: 403.
- Repeat of the same destination: 200 `{ "ok": true, "already": true }`.

## Sync must not wipe parked copies

`ax global sync` upserts project policy rows into global.db. It must **not** `DELETE FROM global_policy_* WHERE project_id = ?` first, so items that live only in global.db survive.

## List

Global copies whose `projects.path` is this workspace are listed when their `item_id` is **not** already in the project list (`origin: global`). Other projects stay as today.

**P7 — Relocate payload is list-shaped:** `POST /relocate` stores a `PolicyRuleDoc`/`PolicySkillDoc` with nested `frontmatter`. `GET /rules` and `GET /skills` must flatten that to a list row (`id`/`name`, `tags`/`globs`/`triggers` arrays) so the overview does not crash (blank page).

## UI (English)

Right-click a table row:

| Row | Items |
|---|---|
| Project `ax.db` | Open, Edit, Enable or Disable, Move to global.db, Delete |
| Global.db | Move to this project, Delete from global.db |

## Must not

- Match agents against global.db (still project `ax.db` only).
- Dutch labels.

## Setup

No new packages. Isolation: none. Tests: `node --test` menu helper; `cargo test -p ax-global-db`.
