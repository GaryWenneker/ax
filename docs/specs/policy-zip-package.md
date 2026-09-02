# SPEC: Sitecore-style policy zip packages

**Tier:** 3 (portable files + overwrite of team policy)
**Spec approval:** approved 2026-09-02 (user: "approve")

This is not git share (`.agents/` in the repo) and not OneDrive/GitHub sync. It is a **portable zip** of selected rules and skills, composed and restored in Command Center modals.

## Sitecore mapping

| Sitecore | ax |
|----------|-----|
| Package Designer (pick items) | Compose modal: checkboxes for rules + skills |
| Generate package `.zip` | Download `*.ax-policy.zip` |
| Install Package wizard | Restore modal: upload, preview, per-item skip/overwrite |
| `package.xml` | `ax-package.json` manifest |

## Package format

Zip, UTF-8, no BOM. Layout:

```
ax-package.json
rules/<id>.mdc
skills/<name>/SKILL.md
skills/<name>/…          # extra files already in that skill directory
```

`ax-package.json`:

```json
{
  "kind": "ax-policy-package",
  "formatVersion": 1,
  "name": "string",
  "description": "string",
  "createdAt": "ISO-8601",
  "axVersion": "string",
  "rules": [{ "id": "utf8-no-bom", "path": "rules/utf8-no-bom.mdc" }],
  "skills": [{ "name": "startup", "path": "skills/startup/SKILL.md" }]
}
```

Reject restore if `kind` is not `ax-policy-package` or `formatVersion` is unsupported. Zip-slip: only paths under `rules/` and `skills/`; refuse `..`.

## Behaviors

### B1 — Compose modal

Rules page and Skills page each have **Package**. Both open the same `ModalShell` flow: name (required), optional description, grouped checkbox lists of **shareable** rules and skills (not private, not disabled). Empty selection disables Download. Download returns `application/zip`.

### B2 — Pack contents

The zip contains only selected files from `.agents/rules` and `.agents/skills`. Private and inactive paths are never included even if ids are posted.

### B3 — Restore preview

**Restore package** opens `ModalShell`, file input for `.zip`. Preview lists each item as `new` | `conflict` | `invalid`. Default action: `new` → install, `conflict` → **skip**, `invalid` → cannot install.

### B4 — Per-item restore decisions

On conflict the user may set **overwrite** or **skip**. Confirm writes chosen items into `.agents/` (project layer), then policy re-index. Overwrite replaces the on-disk file. Skip leaves the existing file. Invalid items are never written.

### B5 — CLI

`ax policy pack` already has export/import/status/install. Portable zip is:

```bash
ax policy pack zip --out team.ax-policy.zip --name "Team pack" --rules id1,id2 --skills s1,s2
ax policy restore --preview team.ax-policy.zip
ax policy restore team.ax-policy.zip --decisions decisions.json
```

`--decisions` is a JSON object of `"rule:<id>" | "skill:<name>"` → `"overwrite" | "skip"`. Missing conflict keys default to skip (same as the modal).

### B6 — Large modal, select-all, skill descriptions (2026-09-02)

Compose and restore use `ModalShell` size `xl` (≈1120px, tall). Compose has **Select all** / **Select none** per list (rules and skills). Skill and rule rows show **name + description**. Missing rule descriptions are generated from the body (first heading/paragraph) or a humanized id.

### B7 — Visual compare vs local + git-style diff on click

Preview items include `compare`: `new` | `identical` | `changed` | `invalid`. `status` stays `new` | `conflict` | `invalid` (`conflict` = local file exists). Restore lists a compare badge. Clicking a row shows a git-style unified diff (**local** vs **package**). Compose click shows the local description and body.

POST `/api/policy/package/diff` multipart: `package` + `kind` + `id` → `{ compare, unified }`.

## HTTP (internal Command Center)

| Method | Path | Body | Response |
|--------|------|------|----------|
| POST | `/api/policy/package` | JSON `{ name, description?, ruleIds[], skillNames[] }` | `200` zip bytes |
| POST | `/api/policy/package/preview` | multipart zip | `200` `{ name, items[] }` |
| POST | `/api/policy/package/restore` | multipart zip + `decisions` JSON | `200` `{ written, skipped, errors }` |
| POST | `/api/policy/package/diff` | multipart zip + `kind` + `id` | `200` `{ compare, unified }` |

Empty pack or unknown ids: `422`. Bad zip: `400`. Auth: same as other Command Center policy routes (local server, not public).

## Invariants

- Do not pack `.ax/policy-private/` or `.ax/policy-inactive/`.
- Do not change OneDrive/GitHub share.
- Do not add npm/cargo deps beyond the existing `zip` crate.
- UI strings English. Modals use `ModalShell` (blurred backdrop).
- Isolation: none (feature lands on current branch).

## Setup

No new dependencies. Files (planned):

- this spec + EVIDENCE + `tools/gauntlet-policy-zip-package.sh`
- `crates/ax-policy` pack/restore unit tests
- `crates/ax-web` routes + `web-ui` compose/restore modals
- `crates/ax-cli` `policy pack` / `policy restore`
- `site/` policy-engine + `reference/cli.md` + README

## API gates (old-coder-api)

- Public or internal: **internal** (localhost Command Center).
- Existing or greenfield: **greenfield** paths under `/api/policy/package`.
- Boring: POST resource `package` with preview/restore actions (justified: zip is not JSON REST).
- Auth: same as existing `/api/policy/*`.
- Idempotency: restore overwrite is idempotent for the same bytes; optional key not required (low-stakes local file write).
- Blast radius: zip size cap (e.g. 8 MiB) on upload.
- Pagination: N/A (bounded item list in one package).
