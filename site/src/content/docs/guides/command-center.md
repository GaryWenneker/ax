---
title: Command Center
description: Git-aware quality gates, test-impact analysis, and draft PRs from ax ship.
---

**ax v2.1.0+** ships a **Command Center** — a local git watcher, quality-gate pipeline, SSE dashboard, and optional draft PR integration (Azure DevOps or GitHub).

Run it after `ax init`; configuration lives in `.ax/ship.toml` (seeded automatically on init when missing).

Prefer a native GPU window instead of the browser? Use `ax desktop` — see the [Desktop Client](/guides/desktop-client/) guide (same `/api` surface with an embedded `ax-web` server).

Prefer the IDE shell? **[Takumi 匠](/guides/takumi/)** is a Code-OSS fork that hosts Command Center in a webview. Append `?embed=1` (or `?takumi=1`) to hide the browser titlebar when the UI is embedded.

![Command Center — quality gate pipeline with completed evaluation, pipeline steps (Index, TIA, Tests, Sonar, Policy), branch overview, and SonarQube status](/screenshots/cc-ship-full.png)

## Quick start

```bash
ax init                          # creates .ax/ship.toml if missing
# Edit .ax/ship.toml — set org/project/repo_id for your AzDO repo

ax ship --evaluate               # one-shot quality gate (index → diff → TIA → tests → policy)
ax ship --ci                     # headless CI: JSON on stdout, exit 1 if gate failed
ax ship --evaluate --auto-commit --revert-on-fail  # opt-in checkpoint for this run only
ax ship --watch --port 7070      # dashboard + git watcher at http://localhost:7070
ax ship --draft --title "feat: …"  # draft PR after quality gate passes (needs PAT)
ax share --open                  # LAN share with token (read-only) — see Share guide
```

## Commands

| Command | Purpose |
|---|---|
| `ax diff --base main` | Symbol-level git diff vs base branch (`--json`) |
| `ax test-impact --base main` | Git diff + reverse reachability to test functions |
| `ax affected [files…]` | Find test files affected by changed sources (`--stdin`, `--depth`, `--filter`) |
| `ax ship --evaluate` | Run the full quality-gate pipeline once |
| `ax ship --ci` | Headless CI mode — JSON report, non-zero exit on failure |
| `ax ship --watch` | Start web dashboard + git event watcher |
| `ax ship --draft` | Create a draft PR via configured remote provider |
| `ax share` | Share Command Center on the LAN with a token |

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

[ui]
show_savings = true
show_agent_terminal = true
# Verbose MCP traces → Cursor Output (stderr). Off by default.
verbose_mcp = false
# IANA timezone for Logging Date/time (e.g. "Europe/Amsterdam").
# Empty / "local" = browser local timezone in Command Center.
timezone = ""

[auto_commit]
# Opt-in Aider-style checkpointing — disabled by default. When enabled,
# uncommitted working-tree changes are committed before the gate runs (so
# diff/TIA/Sonar evaluate them as part of history).
enabled = false
message = "ax: auto-checkpoint before quality gate"
# On failure, undo *only* that checkpoint commit via `git reset --mixed`
# (never --hard) — file contents stay on disk, just uncommitted again.
revert_on_fail = false
```

### Auto-commit / rollback (`[auto_commit]`)

Off by default — a deliberate automation feature, not a silent behavior change. With `enabled = true`, every `ax ship --evaluate` commits the current uncommitted diff as a checkpoint before running index/diff/TIA/tests/Sonar/policy, across every git root under this workspace. If the gate passes, the checkpoint stays. If it fails and `revert_on_fail = true`, ax undoes *that exact commit* with `git reset --mixed HEAD~1` — never `--hard` — so the edits remain on disk, uncommitted, for the agent or user to fix and re-run. Rollback refuses if `HEAD` has moved since the checkpoint (e.g. something else was committed in between), rather than resetting the wrong commit.

Override for a single run without touching `ship.toml`:

```bash
ax ship --evaluate --auto-commit                  # force-enable for this run
ax ship --evaluate --auto-commit --revert-on-fail # + rollback on failure
```

The report's `auto_commit` field (`ax ship --evaluate` JSON output) reports `status`: `clean` (nothing to commit), `committed`, `kept-failing` (failed but `revert_on_fail` is off), `reverted`, or `revert-failed`.

Set the PAT in your environment before draft PR or review commands:

```powershell
# Windows — persistent
[System.Environment]::SetEnvironmentVariable('AZDO_PAT', 'your-pat', 'User')
```

For GitHub, uncomment `[remote.github]` and set `GITHUB_TOKEN`.

## Command Center pages (`ax web`)

Settings-style pages (Ship, Savings, Memory, Settings, Policy, …) use a **left-aligned content column** that scales by viewport: 720px → 800px → 960px → **1024px** (XXL). The column’s left edge aligns with the main pane next to the sidebar (not centered). Full-bleed pages (Files, Agent terminal, SonarQube, and open detail blades) stay unconstrained.

**Mobile (≤899px):** hamburger drawer (aligned with the shell breakpoint), status bar keeps Project / Logging / Activity (Activity and Logging open as full-width sheets above the dock — Logging sheet has kind filters, scroll-to-newest, quality), Logging table scrolls horizontally instead of crushing columns, project browser does not autofocus the filter, modals become bottom sheets. Graph and Agent are still best on desktop. Agents verify with `.\scripts\web-ui-mobile-smoke.ps1` (Playwright Pixel 5 + screenshots).

| Page | Purpose |
|---|---|
| **Graph** | Interactive knowledge graph — Leiden communities, god-node tour, suggested questions; optional **Domain** view from `.ax/domain-graph.json` |
| **Nodes** | Symbol table with kind/file filters and detail blade |
| **Search** | FTS over indexed symbols |
| **Ship** | Quality-gate pipeline, SSE logs, git events |
| **Files** | Indexed file tree — folders expand lazily; root-level files (`README.md`, `Cargo.toml`, …) show as files, not folders |
| **SonarQube** | Proxied dashboard with auto-login and dark theme |
| **Memory** | Browse, search, compose memories (modal composer); capture from git |
| **Savings** | Token and dollar savings from graph queries; activity heatmap, trends, tool audit |
| **Prices** | Daily OpenRouter model $/MTok catalog and history |
| **Agent** | Terminal with MCP wired in (when enabled in Settings) |
| **Logging** | Fullscreen table of the **active project** MCP verbose stream (**newest at top**; **Scroll to new** when you leave the top); **filters** by kind chips (Inbound/Outbound/Preview/Error/Internal/Enrich), **Has query** chip (JSON payloads with a top-level `query` — badge + blue row mark), **date** dropdown (`YYYY-MM-DD`), tool dropdown, and text search — also click status-bar in/out/prev/err or Buffer breakdown rows to toggle kinds; click a Kind badge or Tool cell in the table to filter; **Date / time** column shows full calendar day + clock; **fluid columns** (Date/time / Kind / Tool / Summary / Meta) that rebalance on narrow screens; **error rows** show the tool name in danger red; log text is **blurred while offline / reconnecting**; theme-colored status bar shows in/out/prev/err/event counts (muted danger tint when offline) and a **project switcher**; **Q** quality chip opens a metrics slide-out (correlation, enrichment, findings, token waste) with **Copy fixpack** for an agent-ready Markdown brief; auditor softens untimed whole-session Read/Grep and attaches enrich side-channels to preflight; keyboard nav (↑↓ Enter Esc, j/k, b back); tap or Enter for a fixed-size Call Inspector with pretty-printed VS-style JSON/XML and formatted key=value fields |
| **Policy** | View-first rule/skill editors with **Scope** (company → private); master-detail blade (slide-in on open, content reveal when switching rows); resizable list / metadata / source / preview columns (drag grips; double-click resets); Rules/Skills list **label autocomplete** (tag chips, multi-label AND filter, click table tags to toggle); **Rules and Skills grouped** by a shared catalog (collapsible folders with **Collapse all** / **Expand all**; **Groups** multiselect filter; empty groups hidden on the list but still assignable in the editor); Markdown **source** overlay and textarea share font-size / line-height so caret and selection match visible glyphs; tags required on save; layer filters; **History** (hash-on-change revisions, Restore); **Package** / **Restore package** (large zip modal: select all, skill descriptions, one-line compare plus Reject/Accept, git-style local vs package diff, blake3 content hashes); **Sync** (pack export/import + `policySync` hooks); **Review** queue for staged imports |

![MCP Logging — live verbose stream with kind filters, project switcher, and Call Inspector](/screenshots/cc-logging.png)

![MCP Quality — correlation, enrichment, tool mix, findings, and Copy fixpack](/screenshots/cc-mcp-quality.png)

Full walkthrough: [MCP Logging & Quality](/guides/mcp-quality/).

Open with `ax web --open` or `ax ship --watch --open`. If a newer GitHub release exists, `ax web` / `ax desktop` print an update notice on stderr **before** the UI stays running (`ax upgrade` to install).

If pages render but stay empty / forever loading, close extra Command Center tabs and hard-refresh (`Ctrl+Shift+R`). Multiple long-lived SSE streams used to exhaust the browser’s ~6 HTTP/1.1 sockets per host; the UI now shares one EventSource per URL per tab. You can also hit `http://127.0.0.1:PORT/api/reset-client-cache` once if a stale service worker is involved.

The title bar uses the same **azure CSS waves** as the marketing site (soft fade under the crest, matching the cinematic bokeh orbs). Waves sit in a reserved hang band under the chrome; the Logging overlay is portaled below that band so **Back / Newest · To new / Full / Clear** stay fully visible and clickable (never clipped under the titlebar).

```bash
ax web --open
ax mcp audit                  # same quality engine as the Q chip
ax savings hook install       # Cursor sessionStart → model + session tags
```

### Project browser

![Project browser — browse your disk for indexed ax projects, filter, initialize, and switch workspace](/screenshots/cc-project-browser.png)

The workspace picker in the Command Center status bar opens a **project browser** modal. From there you can:

- See **recent ax projects** with quick-switch buttons
- Browse your file system (drive roots on Windows, `/` on Unix) — **all folders**, including empty ones
- Filter by name or show ax-initialized projects only
- Create new folders
- **Initialize** a folder with `ax init` directly from the browser
- Switch the active workspace without restarting the server

Browse is limited to configured `browse_roots` plus home, common project dirs, and filesystem roots. Extra roots can be added in workspace config.

### Modal-based forms

![New memory modal — centered modal with blurred backdrop, title, kind selector, body field, and save action](/screenshots/cc-modal-composer.png)

Create and edit flows in the Command Center (new memory, new agent profile, profile editing) open as **centered modals with a blurred backdrop** — never as inline page sections. The shared `ModalShell` component handles Escape-to-close, backdrop click, and scroll lock.

## Quality gate pipeline

When you run `ax ship --evaluate` (or when git hooks trigger evaluation), ax runs:

1. **index** — incremental sync
2. **diff** — changed files and dirty symbols vs `target_branch`
3. **tia** — test-impact analysis via `Covers` edges in the graph
4. **tests** — runs impacted tests (when any are found)
5. **sonar** — optional SonarQube scan + quality gate (when enabled). If SonarQube is offline but a Podman container already exists, the gate **always tries to start it** (up to **3** `podman start` retries, then waits for the API) before failing.
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

The same `ax web` UI includes **Memory**, **Savings**, **Prices**, **SonarQube**, and **Agent** pages — see the Command Center pages table above. Savings shows estimated context-token and dollar savings from MCP graph queries. Prices tracks daily model rates that feed those dollar estimates. See [`ax savings`](/reference/cli/#ax-savings), [`ax pricing`](/reference/cli/#ax-pricing), and [Token savings](/guides/token-savings/).

![Settings — AI agents with terminal mode and profiles, pipeline config, and account profiles](/screenshots/cc-settings.png)

Open **Settings** in the sidebar (or from Command Center) to manage `.ax/ship.toml`:

- **SonarQube** — auto-detect Podman/Docker, one-click install & start, admin auto-login, dark theme
- **Command Center** — target branch, test runner, Azure DevOps / GitHub remote
- **Interface** — theme chooser (default **ax Mint** `#3ee4b2`; presets include **macOS**, charcoal chrome with `#64d2ff` accent). The **status bar** uses that accent fill with **WCAG AA** ink (`4.5:1`): macOS gets **dark** letters on the light blue, not white-on-blue. Accent/palette apply live. Toggle Savings and Agent pages in the sidebar, **Verbose MCP logging** (writes `[ui] verbose_mcp = true` to `.ax/ship.toml` for the **active** project; records traces to `<project>/.ax/mcp-verbose-YYYY-MM-DD.log`; off by default; never alters tool responses; reconnect ax MCP after enabling), and **Timezone** for Logging Date/time and **daily log rotation** (IANA, e.g. `Europe/Amsterdam`; empty/`local` = host local; timestamps inside files stay UTC)
- **Logging** — live MCP verbose stream (**newest at the top**; loads older days on scroll; **Scroll to new** jumps back to the live top). Enable recording under **Settings → Interface**; this page does not include the on/off switch
- **Sharing** — live share status card (badge, port, copy URL), How to share, Enable PWA / Install / show hint again
- **Open Knowledge Format (OKF)** — **Generate OKF bundle** / **Validate** (and optional wiki publish) via `/api/okf/*`; writes Markdown under `okf.outDir` from `ax.json` — see [OKF](/guides/okf/)
- **Plugins** — live extractor table from `GET /api/plugins` (name, mode, extensions, entry) with Refresh
- **Embeddings** — memory embed backend (`hash` / `onnx` / …), onnx Cargo feature, model + tokenizer paths from `GET /api/memory/embed-status` with Re-probe

### SonarQube proxy

![SonarQube dashboard reverse-proxied inside the Command Center with dark theme, project list, and quality gate filters](/screenshots/cc-sonarqube-dark.png)

The SonarQube page reverse-proxies your local SonarQube instance through the Command Center. The proxy automatically:

- Injects admin credentials (no login screen)
- Forces **dark theme** via CSS overrides, localStorage/sessionStorage keys, user-preference API patching, and a MutationObserver that prevents theme resets
- Rewrites asset URLs and **API paths** (`/api/…`) so SonarQube works behind the `/api/ship/sonar/ui` prefix — including an early `fetch`/`XHR` patch, because Sonar’s axios treats leading-slash URLs as host-absolute and ignores `data-base-url`
- Scopes `Set-Cookie` `Path=/` to the proxy prefix (needed for HTTPS Cloudflare tunnels)
- Strips tunnel `Origin` / `Referer` / `X-Forwarded-*` / `CF-*` request headers that confuse Sonar CSRF
- Caches credentials per session (no health-check probe per request)
- Falls back to `127.0.0.1` / `localhost` when the configured hostname is unreachable

Works the same on `http://127.0.0.1:7070` and via a Cloudflare tunnel to that port — the browser always talks to ax-web; Sonar stays on localhost on the host.

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

## CI mode (`ax ship --ci`)

Headless evaluation for pipelines: prints the quality-gate JSON on stdout, a one-line summary on stderr, and exits **1** when the gate fails. Impacted tests are executed via cargo / pytest / jest / vitest / go runners when the TIA step is enabled.

```bash
ax sync
ax ship --ci
```

Workflow snippets: [GitHub](https://github.com/GaryWenneker/ax/blob/main/docs/examples/github-actions-ship.yml) · [GitLab](https://github.com/GaryWenneker/ax/blob/main/docs/examples/gitlab-ci-ship.yml) · [Azure Pipelines](https://github.com/GaryWenneker/ax/blob/main/docs/examples/azure-pipelines-ship.yml).

See also [Share Command Center](/guides/share/) for LAN/PWA collaboration.

## Azure DevOps (default)

`provider = "azure_devops"` is the default. You need:

- `org`, `project`, `repo_id` in `.ax/ship.toml`
- `AZDO_PAT` (or custom `token_env`) with Code (read & write) scope

Draft PRs call the AzDO REST API; the local git remote can be GitHub or AzDO — the remote provider only affects where PRs are created.
