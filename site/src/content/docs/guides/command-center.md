---
title: Command Center
description: Git-aware quality gates, test-impact analysis, and draft PRs from ax ship.
---

**ax v2.0.15+** ships a **Command Center** — a local git watcher, quality-gate pipeline, SSE dashboard, and optional draft PR integration (Azure DevOps or GitHub).

Run it after `ax init`; configuration lives in `.ax/ship.toml` (seeded automatically on init when missing).

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

Default port: `7070` (override with `--port` or `[ship].web_port`).

## Git hooks

After `ax init`, post-commit hooks run `ax sync --quiet` and `ax ship --evaluate` so the graph and ship state stay current.

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
