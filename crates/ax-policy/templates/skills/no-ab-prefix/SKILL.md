---
name: no-ab-prefix
description: >-
  Verbod op AB#-prefix bij Azure DevOps work items. Gebruik alleen het getal
  (16295) of een volledige AzDO-URL. Activeer bij PR's, commits, Slack,
  bugs, wiki, en alle agent-output.
---

# Geen AB# — work item referenties

> **HARD RULE**: Nooit `AB#` — zie ook `c:\gary\VfPf\.cursor\rules\no-ab-prefix.mdc`.

## Wel

| Context | Voorbeeld |
|---------|-----------|
| Tekst | `16295` |
| PR-titel | `16295 - Fix storing page middleware` |
| Branch | `feature/16295-fix-storing-page-middleware` |
| Commit | `fix(react): skip /storing in middleware 16295` |
| Link | `[Bug titel](https://dev.azure.com/.../16295)` |

## Niet

- ❌ `AB#16295`
- ❌ `AB#16295 - titel`
- ❌ `feature/AB#16295-...`

## Gerelateerde skills

- `pr` — PR aanmaken (AzDO/GitHub)
- `vfpf-git` — branch/commit conventies (monorepo)
