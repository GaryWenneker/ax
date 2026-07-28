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

## Per-project policy pack sync

For **bidirectional** team sync inside a project repo (each workspace member keeps its own pack):

```bash
# Exports enabled project/workspace items (opt out with tags local / noshare):
ax policy pack export
# commit .ax/policy/shared/
ax policy pack import
```

Set `"policySync": true` in that project's `ax.json` for post-commit export / post-merge import hooks. Optional `"policy": { "requireReview": true }` stages imports under `.ax/policy/pending/` until `ax policy review approve`. See [Policy Engine](/guides/policy-engine/#per-project-pack-sync).

### Hierarchical policy merge

On every policy import/index, ax merges layers (later wins on the same rule/skill id). Each item is stamped with a **scope** (`company`, `workspace`, `project`, `private_user`, `private_project`) in both files and database storage modes:

1. `~/.ax/global_policy/` — **company** (UI label; path kept for compatibility)
2. Workspace root `.ax/policy/` — **workspace** (when root `ax.json` has `members`)
3. Member / project `.ax/policy/` — **project**
4. `~/.ax/private_policy/` — **private_user** (never packed / never git-synced)
5. `<project>/.ax/policy-private/` — **private_project** (gitignored via `.ax/.gitignore`)

Company and private scopes are never exported by `ax policy pack export`.

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

## Further reading

Monorepo federation, `ship --ci`, plugins, optional ONNX, LSP enrich, and `ax share` shipped in **v4.0.0**. See [`docs/ROADMAP.md`](https://github.com/GaryWenneker/ax/blob/main/docs/ROADMAP.md).
