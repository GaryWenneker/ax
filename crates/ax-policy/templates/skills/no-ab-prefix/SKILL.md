---
name: no-ab-prefix
description: >-
  Ban the AB# prefix for Azure DevOps work items. Use only the number
  (16295) or a full Azure DevOps URL. Apply to PRs, commits, Slack, bugs,
  wiki pages, and all agent output.
---

# No AB# — work item references

> **HARD RULE**: never write `AB#`. If the repo also has a `no-ab-prefix` rule, it says the same.

## Do

| Context | Example |
|---------|---------|
| Text | `16295` |
| PR title | `16295 - Fix maintenance page middleware` |
| Branch | `feature/16295-fix-maintenance-page-middleware` |
| Commit | `fix(react): skip /maintenance in middleware 16295` |
| Link | `[Bug title](https://dev.azure.com/.../16295)` |

## Don't

- ❌ `AB#16295`
- ❌ `AB#16295 - title`
- ❌ `feature/AB#16295-...`

## Related skills

- `pr` — create a PR (Azure DevOps / GitHub)
- the repo's git conventions skill, if it has one — branch and commit naming
