# SPEC: Make global.db usable from ax

**Tier:** 2  
**Spec approval:** not obtained as a separate review (user: "fix wat je mist" after the gap list)  
**Isolation:** current working tree. **No new cargo dependencies** beyond `tempfile` as a crate *dev*-dependency.

Absolute path: `/Users/gary/io/ax/docs/specs/global-db-usable.md`

## Problem

`~/.ax/global.db` had a schema and scripts, but ax could not sync a real `.ax/ax.db` into it. Rust sync was a no-op, the crate was not in the workspace, the schema path was hardcoded, `node_type` CHECKs rejected real ax kinds, and dry-run looked for `project.db`.

## Must survive

- Existing ax.db schema and CLI commands other than `ax global`.
- Site `latest.txt` / GitHub release pins unchanged.
- TypeScript stubs under `src/` are not the product; the product is Rust + CLI.

## Behaviors

### G1 — Init schema without a hardcoded checkout path

Given a temp file path  
When `open_and_init` runs  
Then tables `projects`, `global_nodes`, `global_documents`, `global_edges`, `cross_project_refs`, `shared_knowledge` exist.

### G2 — Sync copies nodes from project ax.db

Given a project sqlite with one node `Foo` kind `function`  
When `sync_project` runs against that `.ax/ax.db`  
Then `global_nodes` has one row for that project with `name = Foo`.

### G3 — Shared knowledge when two projects share a file hash

Given two projects each with a file of the same `content_hash`  
When both are synced  
Then `shared_knowledge` has a row whose `projects` JSON lists both project names.

### G4 — Missing project db is an error, not a silent zero sync

Given a folder with no `.ax/ax.db`  
When `sync_project` runs  
Then it returns an error mentioning `ax.db`.

### G5 — Dry-run prefers ax.db

`scripts/dry-run-migration.js` looks at `.ax/ax.db` first, then `.ax/project.db`.

## CLI

- `ax global init`
- `ax global sync [path]`
- `ax global status`

Override path with `AX_GLOBAL_DB`.

## Out of scope

Command Center Global/Project/Combined UI, watchdog, MCP tools.
