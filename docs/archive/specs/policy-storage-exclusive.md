# SPEC: Exclusive policy storage (files vs database)

Tier 2. Spec approval: not obtained (autonomous run).

## Goal

CLI can set the **default** storage for rules/skills (`files` | `database`) and **move everything** to that store. Choosing a store **removes the other source** for ax policy files (`.agents/` and `.ax/policy/`).

## Commands

```bash
ax policy storage status
ax policy storage database --yes
ax policy storage database --yes --keep-files
ax policy storage files --yes
```

Without `--yes`, print a plan and do not change files or storage.

## Behaviors

### B1 — Default is visible

`ax policy storage status` prints `Policy storage: files|database` from project `ax.json` (else global, else files).

### B2 — Database exclusive

`--yes` on `database`: write default `database`, import all scanned candidates into `ax.db`, then **delete** imported files whose source is `ax-policy` (`.agents/rules|skills`, `.ax/policy/rules|skills`). Do **not** delete `.cursor/` bootstrap files.

### B3 — Keep files opt-out

`--keep-files` with `--yes` on `database` imports but does not delete markdown.

### B4 — Files exclusive

`--yes` on `files`: write default `files`, export DB rules/skills into `.agents/rules` and `.agents/skills`. Database remains an index (files are source of truth).

### B5 — Preview is non-destructive

`ax policy storage database` without `--yes` does not write `ax.json` or delete files.

## Must not

- Delete `.cursor/rules/ax.mdc` / `ax-agent-workflow.mdc`
- Wipe `ax.db` when switching to files

## Setup

Branch: none (Tier 2 in working tree). No new crates. Gauntlet: `cargo test -p ax-policy migrate -- --nocapture` and `bash tools/gauntlet-policy-storage.sh`.
