---
title: Command Center
description: Git-aware quality gates, test-impact analysis, and draft PRs from ax ship.
---

**ax v2.1.0+** ships a **Command Center** — a local git watcher, quality-gate pipeline, SSE dashboard, and optional draft PR integration (Azure DevOps or GitHub).

Run it after `ax init`; configuration lives in `.ax/ship.toml` (seeded automatically on init when missing).

![Command Center — Ship dashboard with pipeline, quality gate, SonarQube project cards, run log, changed files, and test impact](/screenshots/cc-ship-full.png)

## Quick start

```bash
ax init                          # creates .ax/ship.toml if missing
# Edit .ax/ship.toml — set org/project/repo_id for your AzDO repo

ax ship --evaluate               # one-shot quality gate (index → diff → TIA → tests → policy)
ax ship --watch --port 7070      # dashboard + git watcher at http://localhost:7070
ax ship --draft --title "feat: …"  # draft PR after quality gate passes (needs PAT)
```

## Commands

| Command | Purpose |
|---|---|
| `ax diff --base main` | Symbol-level git diff vs base branch (`--json`) |
| `ax test-impact --base main` | Git diff + reverse reachability to test functions |
| `ax affected [files…]` | Find test files affected by changed sources (`--stdin`, `--depth`, `--filter`) |
| `ax ship --evaluate` | Run the full quality-gate pipeline once |
| `ax ship --watch` | Start web dashboard + git event watcher |
| `ax ship --draft` | Create a draft PR via configured remote provider |

## Configuration (`.ax/ship.toml`)

Seeded by `ax init` when the file does not exist. Never overwritten on re-init.

```toml
[ship]
target_branch = "main"
web_port = 7070

[quality_gate]
steps = ["index", "tia", "tests", "sonar", "policy"]

[quality_gate.tests]
runner = "cargo test"

[remote]
provider = "azure_devops"   # or "github"

[remote.azure_devops]
org = "your-org"
project = "your-project"
repo_id = "your-repo-uuid"  # AzDO → Project Settings → Repositories → Repository ID
token_env = "AZDO_PAT"

[sonar]
enabled = false
host = "http://localhost:9000"
project_key = "your-project"
token_env = "SONAR_TOKEN"
scanner_path = "sonar-scanner"
podman_container = "sonarqube"
```

Set the PAT in your environment before draft PR or review commands:

```powershell
# Windows — persistent
[System.Environment]::SetEnvironmentVariable('AZDO_PAT', 'your-pat', 'User')
```

For GitHub, uncomment `[remote.github]` and set `GITHUB_TOKEN`.

## Command Center pages (`ax web`)

| Page | Purpose |
|---|---|
| **Ship** | Quality-gate pipeline, SSE logs, git events |
| **SonarQube** | Proxied dashboard with auto-login and dark theme |
| **Memory** | Browse, search, compose memories (modal composer); capture from git |
| **Savings** | Token and dollar savings from graph queries (real BPE counts, dollar estimates) |
| **Agent** | Terminal with MCP wired in (when enabled in Settings) |
| **Policy** | View-first rule and skill editors |

Open with `ax web --open` or `ax ship --watch --open`.

### Project browser

![Project browser — browse, filter, and switch between ax projects](/screenshots/cc-project-browser.png)

The workspace picker in the Command Center status bar opens a **project browser** modal. From there you can:

- See **recent ax projects** with quick-switch buttons
- Browse your file system for directories containing `.ax/`
- Filter by name or show ax-initialized projects only
- Create new folders
- **Initialize** a folder with `ax init` directly from the browser
- Switch the active workspace without restarting the server

### Modal-based forms

![New memory modal — kind selector, title, body, and save action](/screenshots/cc-modal-composer.png)

Create and edit flows in the Command Center (new memory, new agent profile, profile editing) open as **centered modals with a blurred backdrop** — never as inline page sections. The shared `ModalShell` component handles Escape-to-close, backdrop click, and scroll lock.

## Quality gate pipeline

When you run `ax ship --evaluate` (or when git hooks trigger evaluation), ax runs:

1. **index** — incremental sync
2. **diff** — changed files and dirty symbols vs `target_branch`
3. **tia** — test-impact analysis via `Covers` edges in the graph
4. **tests** — runs impacted tests (when any are found)
5. **sonar** — optional SonarQube scan + quality gate (when enabled)
6. **policy** — business-rule and breaking-change warnings

Results stream to the dashboard via SSE (`/api/ship/events`).

## Dashboard

`ax ship --watch --open` serves the ax web UI with a **Command Center** tab:

- Live pipeline step status
- Changed files and impacted tests
- Quality-gate summary
- Draft PR action (when remote is configured)
- **SonarQube project cards** — each discovered git repository appears as a card with status, project key, and a per-project **Scan** button. A **Scan all** button triggers all projects at once. Inline logs stream progress during scans.

Default port: `7070` (override with `--port` or `[ship].web_port`).

The same `ax web` UI includes **Memory**, **Savings**, **SonarQube**, and **Agent** pages — see the Command Center pages table above. Savings shows estimated context-token and dollar savings from MCP graph queries. See [`ax savings`](/reference/cli/#ax-savings) and [Token savings](/guides/token-savings/).

![Settings — pipeline config, AI agents, theme, pull request integration](/screenshots/cc-settings.png)

Open **Settings** in the sidebar (or from Command Center) to manage `.ax/ship.toml`:

- **SonarQube** — auto-detect Podman/Docker, one-click install & start, admin auto-login, dark theme
- **Command Center** — target branch, test runner, Azure DevOps / GitHub remote
- **Interface** — theme chooser (accent/palette presets applied live), toggle Savings and Agent pages in the sidebar

### SonarQube proxy

![SonarQube embedded in the Command Center with forced dark theme](/screenshots/cc-sonarqube-dark.png)

The SonarQube page reverse-proxies your local SonarQube instance through the Command Center. The proxy automatically:

- Injects admin credentials (no login screen)
- Forces **dark theme** via CSS overrides, localStorage/sessionStorage keys, user-preference API patching, and a MutationObserver that prevents theme resets
- Rewrites asset URLs and API paths so SonarQube works behind the `/api/ship/sonar/ui` prefix
- Caches credentials per session (no health-check probe per request)
- Falls back to `127.0.0.1` / `localhost` when the configured hostname is unreachable

## Git hooks

After `ax init`, git hooks keep the project current:

- **post-commit** — `ax sync --quiet`, `ax ship --evaluate`, `ax capture-git --limit 1 --quiet` (stores non-trivial commit messages as memories)
- **post-merge** — same sync/evaluate plus `ax capture-git --limit 20 --quiet` for merged commits
- **post-checkout** — sync and evaluate only

## Stale-run recovery

If the server restarts while an evaluation is in progress (e.g. `ax web` crashes mid-scan), the in-progress run log is finalized as failed on startup. This prevents the dashboard from showing a phantom "evaluating" state after a restart.

## Test impact vs affected

| Tool | What it traces |
|---|---|
| `ax affected` | Import/file dependency → test **files** |
| `ax test-impact` | Git diff → dirty symbols → reverse BFS on `Covers` edges → test **functions** |

Use `ax test-impact` when tests are mapped in the graph (Rust `#[test]`, Vitest/Jest, pytest patterns). Use `ax affected` for file-level CI filtering when graph coverage is incomplete.

## Azure DevOps (default)

`provider = "azure_devops"` is the default. You need:

- `org`, `project`, `repo_id` in `.ax/ship.toml`
- `AZDO_PAT` (or custom `token_env`) with Code (read & write) scope

Draft PRs call the AzDO REST API; the local git remote can be GitHub or AzDO — the remote provider only affects where PRs are created.
