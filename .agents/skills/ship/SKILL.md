---
name: ship
description: "Bump, rebuild, deploy, and release ax (GaryWenneker/ax). Use when the user says ship, bump, rebuild, deploy, release, cut a release, or publish ax."
alwaysApply: false
priority: 50
enabled: true
status: approved
scope: project
---

# ship — ax release pipeline

> **ABSOLUTE**: Finish this pipeline **in-session**. A ship/bump/release request authorizes commit, tag, and `git push` of `main` and the release tag. Do **not** stop by handing the user push/tag commands. If Auto-review blocks publish, request approval and retry. If GitHub auth fails, report the exact error and keep the release incomplete.

Orchestrates the full ax release workflow. Windows repo root: `C:\gary\ax`. macOS: `/Users/gary/io/ax` (use `scripts/reinstall-cli.sh`; replicate `release-tag.ps1` if `pwsh` is missing).

## When to use

User says **ship**, **bump**, **rebuild**, **deploy**, **release**, or wants a new ax version live locally and on GitHub/getax.

Default bump: **patch** unless the user says minor/major or the diff clearly adds a new subsystem (then prefer **minor**).

## Preconditions

- On branch `main`, `origin` → GitHub `GaryWenneker/ax`
- `ax ship --evaluate` passes (`ax_ship` MCP or shell in DEGRADED mode)
- Feature work **committed** before `release-tag.ps1` (script blocks dirty trees outside release files)
- Cargo output path is **`target-dev/release/ax.exe`** (not `target/debug`)

## Pipeline (run in order)

Copy this checklist and track progress:

```text
- [ ] 1. ax ship evaluate (quality gate)
- [ ] 2. Commit pending feature work on main
- [ ] 3. GitHub release (bump + tag + push + wait for CI)
- [ ] 4. Local rebuild + deploy (rebuild-web)
- [ ] 5. Verify all ax.exe copies + version
- [ ] 6. Restart ax MCP in Cursor
- [ ] 7. GitHub Pages deployment is green
```

### 1. Quality gate

```powershell
cd C:\gary\ax
ax ship --evaluate
```

MCP: `ax_ship({ "mode": "evaluate" })`. Fix failures before continuing.

### 2. Commit feature work

If `git status` shows uncommitted changes outside release mirror files, commit first:

```powershell
cd C:\gary\ax
git add -A
git commit -m "feat: …"   # user-facing summary
git push origin main
```

Do **not** bump version in this commit unless the user asked for a version-only commit.

### 3. GitHub release (bump, tag, CI)

```powershell
cd C:\gary\ax
.\scripts\release-tag.ps1 -Bump patch -Force -Wait
```

Options:

| Flag | Purpose |
|------|---------|
| `-Bump patch\|minor\|major` | Bump all crate + web-ui versions, sync `site/public/releases/latest.txt` |
| `-Force` | Retag current HEAD without prompts |
| `-Wait` | Poll GitHub Actions + verify 6 platform assets + win zip version |
| `-DryRun` | Print plan only |

This commits release mirror files, pushes `main`, creates tag `vX.Y.Z`, triggers `.github/workflows/release.yml`.

After CI (~8–15 min without `-Wait`):

```powershell
irm https://getax.wenneker.io/install.ps1 | iex
```

### 4. Local rebuild + deploy

Command Center embeds `web-ui/dist` at **compile time**. After any web-ui or CLI change:

```powershell
cd C:\gary\ax
.\scripts\rebuild-web.ps1
```

This runs `release-local.ps1` (kill ax → build → sync **three** install paths) and starts `ax web --port 7070`.

**Fast local-only package** (no GitHub):

```powershell
.\scripts\ship-local.ps1 -Bump patch -Upgrade
```

### 5. Verify binary sync (mandatory)

All three paths must match `target-dev\release\ax.exe` (SHA256):

| Path | Role |
|------|------|
| `%USERPROFILE%\.cargo\bin\ax.exe` | PATH / MCP fallback |
| `%LOCALAPPDATA%\ax\current\bin\ax.exe` | Cursor MCP target |
| `%LOCALAPPDATA%\ax\current\ax.exe` | Legacy installer root |

```powershell
$src = "C:\gary\ax\target-dev\release\ax.exe"
$hash = (Get-FileHash $src).Hash
@(
  "$env:USERPROFILE\.cargo\bin\ax.exe",
  "$env:LOCALAPPDATA\ax\current\bin\ax.exe",
  "$env:LOCALAPPDATA\ax\current\ax.exe"
) | ForEach-Object {
  if ((Get-FileHash $_).Hash -ne $hash) { throw "STALE: $_" }
}
ax --version
```

Prefer `.\scripts\release-local.ps1` — it syncs and verifies automatically.

### 6. Post-ship

- **Restart ax MCP** in Cursor (Settings → MCP) so agents use the new binary.
- If site/docs changed outside CI: `.\scripts\deploy-netlify.ps1`

### 7. GitHub Pages must be green

The newest deployment on the `github-pages` environment must be `success`. This is a failed release while that status is `failure`, `error`, `in_progress`, `queued`, `inactive`, or missing. Older rows do not count. Do not finish the pipeline, and do not tell the user the release is done, until the check exits 0.

https://github.com/GaryWenneker/ax/deployments/github-pages

```bash
python3 scripts/github_pages_status.py
```

`release.yml` deploys the docs mirror after the release docs job. `deploy-site.yml` deploys it when that workflow is run. Both fail the workflow unless the newest status is `success`.

If the Pages site does not exist yet (`gh api repos/GaryWenneker/ax/pages` returns 404), enable it, then run **Build docs site** and wait for the check:

```bash
gh api -X POST repos/GaryWenneker/ax/pages -f build_type=workflow
gh workflow run "Build docs site" --repo GaryWenneker/ax --ref main
```

The canonical docs site stays https://getax.wenneker.io. The Pages mirror is https://garywenneker.github.io/ax/ and is built with `SITE_BASE=/ax`.

## Version rules

- **patch** — 4.2.0 → 4.2.1
- **minor** — 4.2.0 → **4.3.0** (not 4.2.1)
- Never pin unreleased versions in docs/install scripts until the GitHub tag exists

## MCP vs shell

| Need | MCP | Shell (when no MCP) |
|------|-----|---------------------|
| Quality gate | `ax_ship({ "mode": "evaluate" })` | `ax ship --evaluate` |
| Re-index after edits | `ax_sync` | `ax sync` |
| Release/tag/push | — | `release-tag.ps1` (no MCP tool) |
| Build/deploy | — | `rebuild-web.ps1`, `release-local.ps1` |

## Troubleshooting

- **Access is denied** copying `ax.exe` — MCP respawned ax; run `release-local.ps1` (kills all ax.exe, rename-away unlock).
- **Stale localhost:7070 UI** — run `rebuild-web.ps1`, hard refresh (Ctrl+Shift+R).
- **release-tag blocks dirty tree** — commit or stash unrelated files; do not use `-AllowDirty` unless intentional.

## Related

- [[finish-full-ship-pipeline]] — finish in-session, including push
- [[release-all-platforms]] — all six assets before latest.txt
- [[docs-with-features]] — docs in the same change
- [[macos-cursor-ax-mcp-binary]] — local rebuild on this Mac