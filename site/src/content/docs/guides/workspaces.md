---
title: Workspaces (monorepo)
description: Discover and sync multiple ax projects from one root with ax.json members.
---

# Workspaces (monorepo)

ax v4 adds **workspace federation**: one root `ax.json` lists member projects, each with its own `.ax/` index.

## Discover members

```bash
ax init --workspace
```

This scans for:

- Cargo workspace members (`Cargo.toml` `[workspace].members`)
- Nested directories that already contain `.ax/`

It writes a `members` array into root `ax.json` (`.ax.json` is also accepted as an alias when reading):

```json
{
  "members": [
    { "path": "services/api", "name": "api" },
    { "path": "services/billing", "name": "billing" }
  ]
}
```

Then initializes each member.

## Sync / index all members

```bash
ax sync --all
ax index --all
```

Each member keeps an independent SQLite graph under `<member>/.ax/ax.db`.

## Shared policy registries

```bash
ax policy pull https://github.com/acme/ax-org-policy.git
```

Clones into `.ax/policy/vendored/<name>/`, copies rules/skills into the project policy tree, and re-indexes.

### Hierarchical policy merge

On every policy import/index, ax merges layers (later wins on the same rule/skill id):

1. `~/.ax/global_policy/`
2. Workspace root `.ax/policy/` (when root `ax.json` has `members`)
3. Member / project `.ax/policy/`

### API contract federation

During `ax sync` / `ax index`, OpenAPI (YAML/JSON), `.proto`, and `.graphql` / `.gql` files become `Doc` + `Route` nodes with stable ids such as `contract:openapi:GET:/users`. Matching operations across files are linked with inferred `References` edges.

## Shared memory sync

Tag memories with `shared`, then:

```bash
ax remember "Team decision…" --tag shared
ax memory export
# commit .ax/memory/shared.jsonl
ax memory import   # on other machines / after git pull
```

Set `"memorySync": true` in root `ax.json` to install post-commit export and post-merge import hook lines.

## Export graphs

```bash
ax export graph --format graphml --out graph.graphml
ax export graph --format cypher --out import.cypher
```

See the [CLI reference](/reference/cli/) for all formats.

## Roadmap

Monorepo federation, `ship --ci`, plugins, optional ONNX, LSP enrich, and `ax share` land on branch `ax-v4`. See [`docs/ROADMAP.md`](https://github.com/GaryWenneker/ax/blob/ax-v4/docs/ROADMAP.md).
