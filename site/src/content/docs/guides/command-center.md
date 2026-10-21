---
title: Command Center
description: Local web UI for policy, memory, the code graph, MCP logging, and savings.
---

ax ships a **Command Center** (`ax web`) — a local web UI for policy (rules, skills, memory), the code graph, MCP logging and savings, with CLI quality gates and optional draft PRs (Azure DevOps or GitHub). The API answers the browser that loaded it. Another website cannot read it.

Follow the [page-by-page walkthrough](/guides/command-center-walkthrough/) for every page and Settings group.

## On a phone or tablet

Below 900 pixels, open the hamburger menu for a navigation drawer. Tap **Filters** for page searches and filters, or **Sort and filter** for loaded-row column controls. Sheets retain your selections after **Done**; **Close**, Escape or tapping the backdrop dismisses them. On narrow phones, the header search icon opens **Search**.

Lists stack their metadata and keep row actions visible for touch. Selecting a node, memory, policy item, file, log event or quality entry opens its details across the screen. Close the panel to return to the list. **Graph controls** opens search, kind, community, density and export options; **Graph settings** opens the graph display settings.

Settings shows the current section in a selector. Tap it to open **Settings sections**, filter the list if needed, then choose a section. Forms and status panels use scrollable sheets with reachable footer actions. Appearance and saved column widths remain browser-local; mobile layouts do not overwrite desktop widths.

In **Savings**, **Period** opens the time-range controls. **Review** uses a filter sheet and an **Inspect content and source** sheet for current/incoming comparisons. Full-page Markdown editors offer **Markdown** and **Preview** tabs on mobile so each view has the full screen width; switching tabs preserves the draft.

![Settings on a phone](/screenshots/cc-mobile-settings.png)

![Page filters in a mobile sheet](/screenshots/cc-mobile-filters.png)

![Node details on a phone](/screenshots/cc-mobile-details.png)

![Review comparisons in a sheet](/screenshots/cc-mobile-review-details.png)

![A full-width mobile editor](/screenshots/cc-mobile-editor.png)

These screenshots use the compiled UI and public synthetic data. They demonstrate layout and controls, not a production deployment or measured activity.

Run it after `ax init`; configuration lives in `.ax/ship.toml` (seeded automatically on init when missing).

Page headers and filters share compact spacing so more rows fit on screen. Rules, Skills, Memory and shared lists adapt to the width of their panel: wide rows place names, descriptions and tags together; smaller panels stack readable metadata with labels and wrap instead of forcing horizontal scrolling. Hover a shortened description or tag to see its full value, or open the item for complete content. Focused fields and keyboard controls use a soft accent glow; system high-contrast mode retains a thin focus outline. Savings metrics use equal label rows and aligned numeric values, adapting to smaller screens.

The header and sidebar use the public homepage’s Archivo/lime wordmark. Click either logo to return to the admin homepage. Quality Gates has been removed from the UI and navigation; old `/ship` and `#ship` bookmarks return to **Stats**. CLI quality gates and Settings configuration remain available.

In **Settings → Appearance**, choose **Light** or **Dark**, then **Lime**, **Blue**, **Teal**, **Violet**, **Orange** or **Rose**. Lime is the default and matches the website in Dark. Each color uses a darker shade in Light and a brighter shade in Dark. The browser remembers your choice; header/footer backgrounds and semantic error/warning colors keep their existing meaning.

![Light/Dark and accent color controls](/screenshots/cc-accent-colors.png)

In **Policy → Review**, filter by type and effect, inspect current and incoming content, then select individual or filtered items. **Approve selected**, **Reject selected**, and **Approve filtered type** show a confirmation of the affected items. New items add policy; duplicates match existing content and settings; replacements save a differing version. Comparisons include database-backed policies. Changes since inspection stop the decision and require a refresh. Bulk processing stops on failure and keeps unprocessed items in the queue. When an import did not record its originating peer, the screen says so. This queue reviews policy imports; graph, memory and logging replication runs separately.

![Policy review with change classification and readable content](/screenshots/cc-policy-review-desktop-chrome.png)

![Current and incoming policy content](/screenshots/cc-policy-review-detail.png)

Prefer a native GPU window instead of the browser? Use `ax desktop` — see the [Desktop Client](/guides/desktop-client/) guide (same `/api` surface with an embedded `ax-web` server).

Embedding Command Center in an IDE webview? Append `?embed=1` to hide the browser titlebar.

Each full page load and hard refresh plays a short green edge glow (Green Vessel) with a startup chime. The overlay is removed when that animation ends, so the dashboard is usable again. Moving between pages inside the app does not replay it. `prefers-reduced-motion: reduce` skips the glow. If the browser blocks the chime until a gesture, the first click or key plays it.

![Compact Rules rows on a wide screen](/screenshots/cc-policy-rules-dark.png)

![Memory metadata wraps on a narrow screen](/screenshots/cc-memory-vault-dark-mobile.png)

![Soft accent focus glow](/screenshots/cc-focus-glow-dark.png)

### Adjust list columns

Drag the divider beside a column heading to change that column's width. Use Left/Right arrow keys when the divider has keyboard focus. Widths are remembered in this browser per page and column layout. Double-click a divider or press Home to return to automatic widths.

Click a heading to cycle through ascending, descending and original order. Open the heading's filter icon to match text in that column; multiple column filters intersect. **Columns** provides the same sort/filter controls on narrow screens and **Reset columns** clears the column settings. These controls sort and filter **loaded rows**, alongside the page's existing server filters and pagination; they do not search unseen pages. Action columns keep their existing controls.

The shared controls apply to Rules, Skills, Memory, code lists, Logging, Review, Savings, Prices, Stats, plugin tables and policy-package previews. Policy groups retain their groups. Single links, graph edges and prose cards remain compact links/cards. Logging keeps the event headline and direction details together, with separate Time and Context columns. Incoming events honor active column sorting/filtering, and keyboard navigation follows the visible order. Opening an event still shows its full payload and call steps.

![Logging event, time and context columns](/screenshots/cc-logging-columns.png)

![Logging cards on a narrow screen](/screenshots/cc-logging-columns-mobile.png)

## Azure Portal appearance

Open **Settings → Appearance** and choose **Light** or **Dark**. The selection is saved in this browser. Both modes apply across pages, editors, Logging, quality panels and dialogs. The header search opens pages or searches code; Settings has its own grouped, filterable navigation.

Action rows have a clear gap from their explanatory text. Single-choice dropdowns use the same theme-colored chevron and reserve space for it; keyboard and touch selection remain native.

![Local MCP controls with consistent spacing and dropdown chevron](/screenshots/cc-settings-remote-dark.png)

![Dark appearance and accent selection](/screenshots/cc-settings-appearance-dark.png)

![Azure Portal Dark — Rules](/screenshots/cc-policy-rules-dark.png)

![Azure Portal Dark — code graph](/screenshots/cc-graph-dark.png)

The screenshots use synthetic public example data, not a real team's activity or measured savings.

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
# MCP traces → `.ax/mcp-verbose-YYYY-MM-DD.log` (always on).
verbose_mcp = true
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

Light and Dark use the full viewport with a fixed sidebar, a themed header, breadcrumbs and a content area that grows with the window. Choose one of six accent colors in either mode; legacy preferences migrate to Dark.

**Mobile (≤899px):** hamburger drawer (aligned with the shell breakpoint), status bar keeps Project / Logging / Activity (Activity and Logging open as full-width sheets above the dock — Logging sheet has kind filters, scroll-to-newest, quality), Logging events stack as cards, with sort/filter controls in the Columns menu, project browser does not autofocus the filter, modals become bottom sheets. Graph is still best on desktop. Agents verify with `.\scripts\web-ui-mobile-smoke.ps1` (Playwright Pixel 5 + screenshots).

The sidebar has four groups, most-used first:

| Group | Pages |
|---|---|
| **Policy** | Rules, Skills, Memory, Sync, Review |
| **Code** | Graph, Search, Nodes, Files |
| **Activity** | Logging, Stats, Savings (hidden when turned off in Settings), Unresolved |
| **System** | Settings, Prices, and the **Reload MCP** action |

The Agent terminal and Quality Gates pages have been removed. Old `/agent` and `/ship` links open **Stats**. SonarQube is available through its `/sonar` route when configured; it is not a main sidebar item.

| Page | Purpose |
|---|---|
| **Graph** | Interactive knowledge graph of the **selected workspace** index only — Leiden communities, god-node tour, suggested questions; optional **Domain** view from `.ax/domain-graph.json`. Other workspaces in `~/.ax/global.db` are not drawn here. |
| **Nodes** | Symbol table with kind/file filters and detail blade |
| **Search** | FTS over indexed symbols |
| **Stats** | Index overview for the active project |
| **Files** | Indexed file tree — folders expand lazily; root-level files (`README.md`, `Cargo.toml`, …) show as files, not folders |
| **Memory** | Browse, search, compose memories (modal composer); capture from git |
| **Savings** | Token and dollar savings from graph queries; activity heatmap, trends, tool audit |
| **Prices** | OpenRouter rates grouped by provider (jump legend), plus curated Cursor Composer rates OpenRouter does not list; chart legend uses distinct input/output colors |
| **Unresolved** | Unresolved graph references with kind filters and a scrolling list |
| **Logging** | Live verbose log for the active project in the app chrome, newest first by default. Event keeps the headline and direction details together; Time and Context show timestamps, kinds and tools. Resize individual columns, sort headings and filter loaded rows. Incoming events honor those settings and keyboard navigation follows the visible order. Kind/tool/date/text filters and Scroll to new remain available. Narrow screens show cards with a Columns menu. Open an event for its full payload and call steps; Full expands the view. |
| **Policy** | View-first rule/skill editors with **Scope** (company → private); master-detail blade (slide-in on open, content reveal when switching rows); resizable list / metadata / source / preview columns (drag grips; double-click resets); Rules/Skills list **label autocomplete** (tag chips, multi-label AND filter, click table tags to toggle); **Rules and Skills grouped** by a shared catalog (collapsible folders with **Collapse all** / **Expand all**; **Groups** multiselect filter; empty groups hidden on the list but still assignable in the editor); Markdown **source** overlay and textarea share font-size / line-height so caret and selection match visible glyphs; tags required on save; layer filters; **Database** column plus teal (this project `ax.db`) vs gold (`~/.ax/global.db`) row bars and badges; right-click **context menu** (move between project and global.db); **History** (hash-on-change revisions, Restore); **Package** / **Restore package** (compose stays large; restore starts compact with drag-and-drop zip, then a near-full-width preview: select all, skill descriptions, one-line compare (red Local newer, orange Package newer, green Identical) plus Reject/Accept or **per-change** hunks labeled **Old** / **New** above the code, blake3 content hashes; re-index skips Cursor-native files in `.agents`); **Sync** (pack export/import + `policySync` hooks); **Review** queue for staged imports |

The Rules and Skills pages have a toolbar with **Graph** (link graph of rules, skills, and memories), **Auto-group**, **Test match**, **Package**, **Restore package**, **Capture**, and **New rule** / **New skill**. The **Default MD / DB** switch picks where new items are stored: Markdown files in the policy folder, or the project database.

### Skill search

**Search more online** sits to the left of Filters on Skills. It opens a panel, searches the public skills catalog, and installs one skill at a time. Each result shows the summary, star count, first-seen date, and security audits that belong to that skill. The installed skill is usable immediately. The receipt prints the source, the pinned commit, and the content hash. Audit stays disabled until that skill is installed. **Report installs** is off until you turn the switch on.

From the terminal, skill search is `ax skill search react`. Add one hit with `ax skill add <source> --skill <name>`. See [ax skill](/reference/cli/#ax-skill).

### Policy search

Policy search looks through the rules and skills already in this project. It does not search the public catalog.

On Rules and on Skills, **Filters** narrow the list you already have. Type a label to see suggestions, add tag chips (every selected tag must match), and click a tag in the table to toggle it. **Groups** filters by catalog folder.

**Test match** on the toolbar is the other policy search. Type a prompt and see which rules and skills would be injected, the same matcher `ax_preflight` uses. The CLI is `ax policy match "prompt text"`. `ax policy rules` and `ax policy skills` list what is indexed. See [Policy search](/guides/policy-engine/#policy-search).

**Audit** on an installed skill, or on a rule, stores a quality report and leaves the item enabled. **Copy report** on the green result copies an agent brief: the file to edit, the findings, and the edit instructions. A path such as `skills/<name>/SKILL.md` is present when that skill file exists in a policy folder or the skill is stored in the project or global database. Placeholder paths and directories are not reported as missing files. Clicking an action keeps a dark mark on that button and a darker wash on its row until you click elsewhere.

The same checks are available from the CLI: `ax skill audit`, `ax skill brief`, and `ax policy quality <id>`.

![Review — staged policy imports waiting for Accept or Reject](/screenshots/cc-policy-review-dark.png)

![Prices — provider groups, jump legend, and input/output rate colors](/screenshots/cc-prices-dark.png)

![MCP Logging — event, time and context columns with readable direction metadata](/screenshots/cc-logging-columns.png)

![MCP Quality — correlation, enrichment, tool mix, findings, and Copy fixpack](/screenshots/cc-mcp-quality-dark.png)

Full walkthrough: [MCP Logging & Quality](/guides/mcp-quality/).

Open with `ax web --open` or `ax ship --watch --open`. If a newer GitHub release exists, `ax web` / `ax desktop` print an update notice on stderr **before** the UI stays running (`ax upgrade` to install).

If pages render but stay empty / forever loading, close extra Command Center tabs and hard-refresh (`Ctrl+Shift+R`). Multiple long-lived SSE streams used to exhaust the browser’s ~6 HTTP/1.1 sockets per host; the UI now shares one EventSource per URL per tab. You can also hit `http://127.0.0.1:PORT/api/reset-client-cache` once if a stale service worker is involved.

Logging uses one heading with the project, connection state and event count. **Quality** opens the metrics panel; the project label’s tooltip shows the source log path. Kind filters are quiet text chips; on narrow screens they scroll horizontally. Date, tool and search stay together below them. The full event payload remains in the side inspector.

```bash
ax web --open
ax mcp audit                  # same quality engine as the Q chip
ax savings hook install       # Cursor sessionStart → model + session tags
```

### Live updates

Pages refresh on their own when the data behind them changes; there is no need to press F5. The server checks the index, memory vault, policy, and usage databases once per second and sends a short change event over `GET /api/changes` (Server-Sent Events). A page reloads only the area that changed:

| Change | Pages that refresh |
|---|---|
| Index (`graph`) | Graph, Search, Nodes, Files, Stats, Unresolved, status bar |
| Memory (`memory`) | Memory |
| Rules / skills | Rules, Skills, Sync, Review |
| Usage and prices (`usage`) | Savings, Prices, status bar |

New rows keep their title color. A narrow light moves once through the title and subtitle, the same kind of chase Cursor uses on streaming text. The row background stays still. New graph nodes get a two-ring spark; a node outside the current view gets an arrow at the canvas edge for three seconds, and more than 20 new nodes show one "N new nodes" notice instead. The graph never moves the camera or resets the layout on a live update. With reduced motion enabled, the title stays still and the spark does not grow. Logging and Ship already stream on their own; new Logging entries and new items in the activity feed chase too, while the history loaded on open or by scrolling does not. The light takes the color of the record type.

Cache status lines (`ax_cache_status`, and a line when a context-cache body is stored) share a colored node and, when they sit next to each other, a rail. The same color means the same session group. A line on its own is not joined.

The graph shows the top nodes by degree (the **Density** slider), but newly indexed nodes are always added to the view on a live update, with their links, so you see each one arrive. They stay until you reload the page or change Density.

### Project browser

![Project browser — browse your disk for indexed ax projects, filter, initialize, and switch workspace](/screenshots/cc-project-browser-dark.png)

The workspace picker in the Command Center status bar opens a **project browser** modal. From there you can:

- See **recent ax projects** with quick-switch buttons
- Browse your file system (drive roots on Windows, `/` on Unix) — **all folders**, including empty ones
- Filter by name or show ax-initialized projects only
- Create new folders
- **Initialize** a folder with `ax init` directly from the browser
- Switch the active workspace without restarting the server

Browse is limited to configured `browse_roots` plus home, common project dirs, and filesystem roots. Extra roots can be added in workspace config.

### Modal-based forms

![New memory modal — centered modal with blurred backdrop, title, kind selector, body field, and save action](/screenshots/cc-modal-composer-dark.png)

Create and edit flows in the Command Center (new memory, new agent profile, folder picker, project purge) open as **centered modals with a blurred backdrop** — never as inline page sections. The shared `ModalShell` component handles Escape-to-close, backdrop click, and scroll lock.

## Quality gate pipeline

When you run `ax ship --evaluate` (or when git hooks trigger evaluation), ax runs:

1. **index** — incremental sync
2. **diff** — changed files and dirty symbols vs `target_branch`
3. **tia** — test-impact analysis via `Covers` edges in the graph
4. **tests** — runs impacted tests (when any are found)
5. **sonar** — optional SonarQube scan + quality gate (when enabled). If SonarQube is offline but a Podman container already exists, the gate **always tries to start it** (up to **3** `podman start` retries, then waits for the API) before failing.
6. **policy** — business-rule and breaking-change warnings

CLI quality-gate output remains available through `ax ship`; the shared event API is retained.

## Admin homepage and CLI gates

The admin homepage shows project statistics. Quality Gates is no longer a web page; use `ax ship` for the pipeline.

The header project switcher lists every initialized ax project on this machine, not only the ones opened from Command Center. It keeps recently opened projects first, then adds any folder under your home directory (four levels deep) that contains `.ax/ax.db`. Hidden directories and build folders such as `node_modules` and `target` are skipped.

The CLI pipeline reports:

- **Overview** — branch, changed files, impacted tests
- **Quality gate** summary and pipeline step progress
- Changed files, test impact, breaking changes, and business rules after a run
- Draft PR action (when remote is configured)

SonarQube install, setup, and scan buttons are no longer in the dashboard. The `sonar` quality-gate step still runs when `[sonar] enabled = true` in `.ax/ship.toml`.

Default port: `7070` (override with `--port` or `[ship].web_port`).

The same `ax web` UI includes **Memory**, **Savings**, and **Prices**. Skills and Rules support single- and multi-select (Shift/Ctrl or Cmd, header checkbox) with the context menu applying to the selection. Package/Restore can list global.db copies and optionally copy restored items into global.db. Clicking outside an open rule/skill editor returns to the list. See [`ax savings`](/reference/cli/#ax-savings), [`ax pricing`](/reference/cli/#ax-pricing), and [Token savings](/guides/token-savings/).

### Editing a rule or skill body

The body card has three views: **Markdown** (the source), **WYSIWYG** (rich text), and **Preview**. Your choice is remembered in the browser.

- **WYSIWYG** has a formatting toolbar (bold, italic, strikethrough, code, headings, lists, task lists, quotes, code blocks, links) and the shortcuts ⌘B, ⌘I and ⌘K. Edits are saved as Markdown. Opening the view does not change the body; only a real edit does. `[[links]]`, HTML comments, tables, fenced code and task lists survive the round trip. Tables may come back with padded columns.
- **Link picker.** Type `[[` in the Markdown or WYSIWYG view to link a rule, skill or memory, from this project or global. In WYSIWYG, the toolbar's link button and ⌘K open the same picker with its own search box; with text selected, that text becomes the link label (`[[azdo-pr-review|the review skill]]`). Paste an `http(s)://` address there to make a normal web link instead.
  - An empty search lists every kind, grouped under Rules, Skills and Memory with a count each. The chips All · Rules · Skills · Memory narrow the list. At most 100 rows show; the rest are counted below the list.
  - Quick search: plain words match the name, title and tags, and every word must match (`azure review`). `#tag` searches tags only, by exact tag or prefix (`#az` finds `azure` and `azdo`). `rule:`, `skill:`, `memory:` (or `r:`, `s:`, `m:`) set the kind from the keyboard, so `[[s:#azure` lists skills tagged azure. Exact name matches come first, then names that start with the word, then the rest. Up to three tags show on each row, with the matched tag highlighted.
  - Use the arrow keys to move, Enter (or Tab after `[[`) to insert, and Esc to close. The picker writes the shortest link that still resolves: a bare name when it is unique, otherwise the folder path (`global/rules/review`); memories always use `memories/<title>`. The list comes from `GET /api/links/graph`, whose nodes carry that link text in `target` and their tags in `tags`.

![Settings — IDEs and coding agents found on this machine, with Connect and Disconnect](/screenshots/cc-settings-dark.png)

Settings also describes **MCP Output: Smart Output**. Output representation is chosen internally; there is no manual XML/JSON/Markdown selector. Cache status and measured savings are not claimed by this static description.

Open **Settings** in the sidebar (or from Command Center) to manage `.ax/ship.toml`:

- **IDEs & agents** — connect ax to the IDEs and coding agents found on this machine (writes the ax MCP server and hooks into their config). **Connect** also puts the Command Center inside the IDE — see [Command Center inside your IDE](#command-center-inside-your-ide)
- **Command Center** — target branch, git repositories, dashboard port, test runner, index scope, **Review comment language**, and the PR provider (Azure DevOps organization / project / repository ID / token env var, or GitHub)
- **Appearance** — choose Light/Dark and an accent; the browser remembers the selection. Header/footer backgrounds retain their theme; Dark is neutral and Light uses a blue header. Show Savings and log Timezone live here. Empty/local timezone uses browser-local display and host-local file rotation; line timestamps stay UTC. Verbose logging is always enabled.
- **Sharing** — policy sync, live share status card (badge, port, copy URL), How to share, Enable PWA / Install
- **Vault connection** — mount rules, skills, and memories as a WebDAV drive for Obsidian or any editor — see [Obsidian Vault](/guides/obsidian-vault/). With login on, the browser vault uses that session and a desktop mount needs the share token.
- **Open Knowledge Format (OKF)** — **Generate OKF bundle** / **Validate** (and optional wiki publish) via `/api/okf/*`; writes Markdown under `okf.outDir` from `ax.json` — see [OKF](/guides/okf/)
- **Plugins** — live extractor table from `GET /api/plugins` (name, mode, extensions, entry) with Refresh
- **Embeddings** — memory embed backend (`hash` / `onnx` / …), onnx Cargo feature, model + tokenizer paths from `GET /api/memory/embed-status` with Re-probe
- **Remote policy share** — pull team rules, skills, and optional memory from a git host or OneDrive for this project — see [Remote Policy Share](/guides/policy-sharing/)

## Command Center inside your IDE

**Settings → IDEs & agents → Connect** installs the Command Center into the IDE, next to the MCP server. The ax binary carries the extension and plugin, so nothing is downloaded. Connect installs that IDE's extension when its own extension list does not already contain the bundled version, including after **Disconnect**. A leftover folder under the extensions directory does not count as installed.

| IDE | What you get |
|---|---|
| Cursor, VS Code, Windsurf, Antigravity, Kiro | Extension **ax Command Center** is a local UI extension, so the command is available in every window of that editor, including SSH remotes. Click **ax** in the status bar, or run **ax: Open Command Center**. That switches Command Center to this window's project, even when localhost already has another project selected, then opens a pop-out. **Open Command Center** loads the panel. Another row switches to that project and then loads the panel. Esc keeps the workspace switch and leaves the panel closed. Quit and reopen the editor after Connect. If `ax web` is not running, the click starts it for this project. |
| JetBrains IDEs (IntelliJ IDEA, Rider, WebStorm, PyCharm, GoLand, …) | Plugin with an **ax** tool window on the right, installed in every JetBrains IDE found. Restart the IDE after Connect. |
| Zed | Zed has no web panels: the task **ax: Open Command Center** (`task: spawn`) opens it in the browser. |

The panel shows `http://127.0.0.1:7070/?embed=1`. The status-bar click selects the project with `POST /api/workspace/switch` before the panel opens. If `ax web` is not running, the extension or plugin starts it for the open project. Change the port with the `ax.webPort` setting (VS Code family). Each IDE row shows a **Panel** / **No panel** badge; **Disconnect** removes only the ax extension, plugin, or task.

Set `AX_NO_IDE_PANEL=1` to connect MCP only, without the extension, plugin, or task.

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


## Recreate documentation screenshots

Build the UI and capture its production pages with Puppeteer and synthetic API responses. This mode does not require a Rust, backend or Docker build:

```bash
cd crates/ax-web/web-ui
npm ci
npm run build
cd ../../../..
AX_SCREENSHOT_DEMO=1 AX_THEME=azure-light node site/scripts/capture-screenshots.mjs
AX_SCREENSHOT_DEMO=1 AX_THEME=azure-dark AX_SHOTS=cc-policy-rules.png,cc-graph.png node site/scripts/capture-screenshots.mjs
```

Dark captures receive a `-dark` suffix. Set `AX_CHROME_PATH` to an installed Chromium browser when needed. Capturing a live environment still requires a redaction terms file and a visual privacy check.
