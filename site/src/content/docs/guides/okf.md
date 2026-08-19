---
title: Open Knowledge Format (OKF)
description: Export a portable Markdown OKF bundle from the ax graph, with optional Azure DevOps Wiki publish.
---

# Open Knowledge Format (OKF)

ax indexes your codebase into `.ax/ax.db` and serves agents over MCP. You can also project that graph into an **Open Knowledge Format (OKF)** bundle: plain Markdown files with YAML frontmatter, one page per concept, cross-linked via relative links — git-diffable and readable on GitHub or Azure DevOps.

SQLite remains the source of truth. The OKF bundle is a portable export for humans, CI artifacts, and wiki publish.

## Generate an OKF bundle

```bash
ax export okf
ax export okf --out knowledge
ax export concepts   # alias
```

Default output directory: `.ax/knowledge` (or `okf.outDir` in `ax.json`).

Each concept page includes:

- YAML frontmatter (`id`, `type`, `title`, `resource`, `generated`, optional `relationships`)
- Signature (when known)
- Calls / Called by Markdown links

## Configure (`ax.json`)

```json
{
  "okf": {
    "enabled": true,
    "outDir": "knowledge",
    "autoExportOnSync": false,
    "kinds": [],
    "azdoWiki": {
      "enabled": false,
      "remote": "",
      "local": ".ax/wiki-okf",
      "subdir": "okf",
      "commitMessage": "chore: refresh Open Knowledge Format (OKF) bundle"
    }
  }
}
```

| Field | Meaning |
|---|---|
| `outDir` | **Relative** path from the project root where the OKF bundle is written |
| `kinds` | Optional kind filter (`function`, `method`, …); empty = all code symbols |
| `azdoWiki` | Optional git wiki publish target (Azure DevOps Wiki or any git remote) |

## Validate

```bash
ax export okf --check
ax export okf --check --ci
```

Checks for a root `index.md` and dangling relative Markdown links inside the OKF tree. With `--ci`, validation failures exit non-zero.

## Publish to Azure DevOps Wiki (optional)

No Azure DevOps API token is stored in ax. Publishing uses **git** against `okf.azdoWiki.remote` (same pattern as `ax docs-catalog` wiki pull).

1. Set `okf.azdoWiki.enabled` to `true` and `remote` to your wiki git URL.
2. Ensure git credentials work for that remote on your machine or CI agent.
3. Run:

```bash
ax export okf --publish-wiki --dry-run
ax export okf --publish-wiki
ax export okf --publish-wiki --no-push
```

`--dry-run` previews the destination without cloning or pushing. Live publish clones/pulls the wiki, copies the OKF tree into `subdir`, commits, and pushes.

To ship OKF via the **code repo** instead of a wiki, point `okf.outDir` at a tracked folder (for example `knowledge/`) and commit/push normally.

## Command Center

With `ax web` open, go to **Settings → Open Knowledge Format (OKF)**:

1. **Generate OKF bundle** — writes the Markdown tree to `okf.outDir` (same engine as `ax export okf`)
2. **Validate** — checks `index.md` and relative links
3. **Publish to wiki** — shown when `okf.azdoWiki.enabled` is true; confirm with dry-run by default

APIs: `GET /api/okf/config`, `POST /api/okf/export`, `POST /api/okf/validate`, `POST /api/okf/publish`.

## Related

- [Configuration](/getting-started/configuration/) — `ax.json` overview
- [CLI reference](/reference/cli/) — `ax export okf`
- [Command Center](/guides/command-center/) — Settings card
- [Architecture Insights](/guides/architecture-insights/) — communities and `ax report`
