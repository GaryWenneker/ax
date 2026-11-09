# SPEC: Chocolatey package for AX (Windows)

**Path:** `/Users/gary/io/ax/docs/specs/chocolatey-package.md`  
**Tier:** 2 (distribution packaging; public install surface)  
**Spec approval:** approved 2026-11-09 — user replied `approved` with recommended options (package id `ax-agent`, manual `push=true` only, install root `%LOCALAPPDATA%\ax`)

## Goal

Windows users can install AX with Chocolatey after Community moderation:

```text
choco install ax-agent -y
```

Authors work only from macOS. GitHub Actions on `windows-latest` (and optionally `windows-11-arm`) runs `choco pack` / install smoke / `choco push`. No Chocolatey CLI on macOS.

## Non-goals / hard constraints

- Do **not** install Chocolatey on macOS.
- Do **not** add automatic push/PR/schedule triggers (repo rule: workflows stay manual-only until the owner asks otherwise). Workflow is `workflow_dispatch` only.
- Do **not** auto-`choco push` from `release.yml` on first ship — first Community submit is explicit (`push=true` input + secret present). After moderation, a later change may hook release (out of this SPEC unless approved).
- Do not change the existing `install.ps1` / GitHub release asset names (`ax-win32-x64.zip`, `ax-win32-arm64.zip`, `SHA256SUMS`).
- Do not fake CLI traffic or relocate `.ax` / install root semantics without documenting a delta: Chocolatey install should land the same tree as `install.ps1` (`%LOCALAPPDATA%\ax\current\…`) or document why not.

## Package identity (decide at approval)

| Option | Id | Install command | Notes |
|---|---|---|---|
| **A (recommended)** | `ax-agent` | `choco install ax-agent -y` | Matches the pasted plan; avoids short-name collisions with other “ax” products (e.g. pip `axctl`). Community API probe: **no** existing package with this Id. |
| B | `ax` | `choco install ax -y` | Shortest; also free on Community today, but easier to confuse with unrelated tools. |
| C | `getax` | `choco install getax -y` | Matches getax.wenneker.io branding. |

**Default if you pick “recommended”:** A (`ax-agent`).

## Architecture

```text
Mac (Cursor) ──commit──▶ GitHub
                           │
                           ▼ workflow_dispatch chocolatey.yml
                     windows-latest runner
                           │
              download GitHub release assets (or use tag input)
                           │
              stamp version into packaging/chocolatey/<id>/
                           │
              choco pack → .nupkg artifact
                           │
              optional: choco install <id> --source "'.;https://community.chocolatey.org/api/v2/'" -y
                           │
              optional: choco push (CHOCOLATEY_API_KEY) when push=true
                           ▼
              Chocolatey Community (moderation) → users
```

### Package layout (in repo)

```text
packaging/chocolatey/ax-agent/          # or chosen id
  ax-agent.nuspec
  tools/
    chocolateyInstall.ps1               # arch-aware zip from GitHub release + SHA256
    chocolateyUninstall.ps1             # remove install dir + PATH entry
    VERIFICATION.txt                    # checksum provenance
    LICENSE.txt                         # or link/copy of repo LICENSE
```

`chocolateyInstall.ps1` behavior:

1. Detect x64 vs arm64.
2. Download `ax-win32-{arch}.zip` from `https://github.com/GaryWenneker/ax/releases/download/v{version}/…` (override via package params if needed).
3. Verify SHA256 against release `SHA256SUMS` (or embedded checksum updated by the workflow when packing).
4. Extract into `%LOCALAPPDATA%\ax\current\` (same as `install.ps1`), ensure `bin\ax.exe` on user PATH.
5. Smoke: `ax --version` matches package version.

## Workflow

**File:** `.github/workflows/chocolatey.yml`  
**Trigger:** `workflow_dispatch` only.

| Input | Default | Meaning |
|---|---|---|
| `tag` | (required) | e.g. `v8.4.3` — must exist as published GitHub release with Windows zips + SHA256SUMS |
| `push` | `false` | When `true` and `CHOCOLATEY_API_KEY` secret exists, run `choco push` |
| `run_install_test` | `true` | Local `choco install` from packed nupkg on the runner |

Jobs:

1. `pack` on `windows-latest` — download assets for `tag`, generate checksums into tools scripts / VERIFICATION, `choco pack`, upload `.nupkg`.
2. `install-test` (needs pack, if `run_install_test`) — install from local nupkg, assert `ax --version`.
3. `push` (needs pack, if `push`) — `choco push` to Community; fail closed if secret missing.

Optional later (not required for v1): matrix job on `windows-11-arm` for ARM64 install test only.

## Secrets (human, once)

Repo → Settings → Secrets and variables → Actions:

- `CHOCOLATEY_API_KEY` — from community.chocolatey.org account (never commit).

Owner also creates the Chocolatey.org account and claims the package id on first push.

## Acceptance scenarios

```gherkin
Given a published GitHub release tag with ax-win32-x64.zip, ax-win32-arm64.zip, SHA256SUMS
When gh workflow run chocolatey.yml -f tag=vX.Y.Z -f push=false
Then the run uploads a .nupkg artifact
And install-test exits 0 with ax --version matching X.Y.Z

Given CHOCOLATEY_API_KEY is set
When the same workflow runs with push=true
Then choco push succeeds (or Community returns a clear moderation-pending response)
And the package id matches the approved option (A/B/C)

Given Windows user after moderation
When they run choco install <id> -y
Then ax.exe is on PATH and ax --version works
```

## Setup / isolation

- Work on current branch / main PR as usual (no worktree required — packaging + YAML only).
- New deps: none in Cargo/npm.
- Docs: short section in installation docs + pointer that Mac authors use GH Actions, not local choco.
- Website: owner deploys Netlify separately; source doc update only (no Netlify deploy from agent).

## Invariants

- Existing six release assets unchanged.
- Manual-only workflow triggers.
- Checksums verified on install.
- Package must not require MCP/daemon for CLI to start.

## Open decisions for approval

1. Package id: **A `ax-agent` (recommended)** / B `ax` / C `getax`
2. First push mode: **manual `push=true` only (recommended)** vs auto after every `release.yml`
3. Install root: **match `install.ps1` (`%LOCALAPPDATA%\ax`) (recommended)** vs Chocolatey lib path only with shims

Reply with the three choices (or “recommended”) to approve this SPEC; implementation starts after that.
