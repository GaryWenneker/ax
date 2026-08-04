---
title: Takumi 匠
description: Code-OSS fork with native Ax UI — lifecycle, theme-matched panels, menubar, and MCP.
---

**Takumi 匠** is a full [Code-OSS](https://github.com/microsoft/vscode) fork with **Ax fused into the IDE chrome** — not an optional marketplace plugin. The top-level **Ax** menu, activity bar, and status bar are first-class UI. Native panels call the local `ax web` **API** and use **only Takumi / VS Code theme tokens** (no forced Mint theme, no brand accent colors).

User-facing labels always use **Ax** (capital A). Technical IDs and CLI remain lowercase (`ax`, `ax.exe`, command ids).

## Repository layout

| Path | Role |
|------|------|
| `c:\gary\takumi` | Code-OSS fork (product identity + native Ax host) |
| `c:\gary\ax` | Ax brain — graph, policy, MCP, web API |
| `takumi.code-workspace` | Multi-root workspace linking both trees |

Upstream remote points at `microsoft/vscode`. Product branch: `feature/takumi-vscode-fork`.

## Lifecycle (automatic)

On trusted folder open, the built-in `extensions/ax` host:

1. Ensures the **ax binary** (`ax version`; installs via getax or `ax upgrade` if missing)
2. Runs **`ax install --yes --target takumi --path <folder>`** (writes `.vscode/mcp.json`)
3. Runs **`ax init`** when `.ax/` is missing (or after ax major/minor bumps)
4. Runs **`ax sync --quiet`**, then starts **`ax sync --watch`** (5-minute interval fallback)

Settings: `ax.lifecycle.enabled` (default on), `ax.lifecycle.syncWatch`, `ax.lifecycle.syncIntervalMinutes` (5).

## Native Ax UI

| Surface | What it does |
|---------|----------------|
| **Ax** menubar | Stats, Graph, Ship, Sonar, Agent, Policy suite, Memory, Search, Settings, logging/files/nodes, Sync, Ship Evaluate, lifecycle refresh, Wire MCP, Start MCP |
| Activity bar **Ax** | Same pages as the menubar (native panels) + project switcher |
| Status bar | Index health chips; the **project** chip opens the native **Open Ax project** switcher |
| Editor context | **Show in Ax Graph** |
| Host (`extensions/ax`) | Lifecycle, starts `ax web` as API, hosts native page webviews, wires `.vscode/mcp.json` |

Each menu item opens its **own native panel** (stats, nodes, graph, files, search, memory, ship, sonar, agent, settings, logging, savings, prices, unresolved, policy-*). Panels speak `/api/*` on the local `ax web` process — the browser SPA is not the product UI.

Policy rule and skill editors open as modal dialogs over their respective list panels. Opening an existing item fetches the full policy document first, so every frontmatter field and the Markdown body are loaded before it can be saved.

- **Level** is always a combo (`CRITICAL`, `WARNING`, `INFO`).
- **Tags** and **triggers** use autocomplete chip badges (suggestions from existing policy; type and Enter to add custom values).
- **Match** and **Review** open as modals from the Policy Rules toolbar (not separate editor tabs).
- An indeterminate progress bar appears in the panel header while API calls or page loads run.
- MCP wiring (`.vscode/mcp.json`) uses atomic writes with retries so lifecycle sync is not blocked by transient Windows file locks.

Memory create and edit open as modal dialogs. Click a title or **Edit** to load the full record (title, kind, body, tags) before saving via `PUT /api/memory/{id}`.

Markdown body fields (policy rules, skills, memory) use a larger editor with **Plain**, **WYSIWYG**, and **View** mode tabs.

All native list tables (policy rules/skills, memory, files, nodes, search, unresolved, stats languages, review) are **sortable** — click a column header to sort ascending/descending.

JSON, logs, and code blocks (policy match results, ship/agent output, markdown View mode) use a **source viewer** with line numbers and VS Code–token syntax colors.

The **Logging** page matches ax-web: verbose toggle, live MCP trace table (kind/tool filters, sortable columns), quality modal, and call inspector modal with syntax-highlighted JSON.

Optional troubleshooting: `ax.legacyEmbedUi` embeds the SPA in an iframe (default **false**).

**Preferences** (Ax menu) edits `.ax/ship.toml` UI settings: verbose MCP logging, savings visibility, and Logging timezone (combo picker — same zones as ax-web).

### Open Ax project

Click the status-bar project name (or **Ax → Open Ax project**). Takumi opens a native panel: recent indexed projects, folder browse, **Ax projects only**, create folder, initialize (`ax init`), and **Switch to this project**.

On Windows, project paths are normalized before display and when opening folders (extended-length `\\?\` prefixes from `canonicalize()` are stripped), so Welcome → Recent shows `C:\gary\…` instead of `\\?\C:\gary`.

## Install MCP for a project

```bash
ax install --yes --target takumi --path .
```

Or rely on lifecycle / **Ax → Wire MCP** / setting `ax.autoWireMcp` (default on).

Wiring alone is not enough for Bonzai Agent: Takumi also **starts** the `ax` MCP stdio server on activation (`ax.autoStartMcp`, default on). If the agent still reports Mode: DEGRADED, run **Ax → Start MCP** (or **MCP: List Servers → Start ax** and approve trust), then ask again.

`--path` sets the project root explicitly so Takumi can wire MCP even when the process cwd is not the workspace folder.

## Theme

Native panels use `var(--vscode-*)` only (foreground, editor background, buttons, inputs, lists, `--vscode-charts-*` for the graph). Takumi does **not** force **ax Mint Dark**.

## Bonzai SSO

Takumi can sign in to [Bonzai](https://bonzai.iodigital.com) with the same SAML SSO as the website (**iO Employees & Clients Login**) and then call Bonzai `/api/*` with a Bearer token.

| Piece | Detail |
|-------|--------|
| Extension | `extensions/bonzai-authentication` (auth provider id `bonzai`) |
| Accounts menu | **Sign in to Bonzai...** (same Accounts icon as Copilot) |
| Sign-in | Accounts menu, status-bar chip, or Command Palette **Bonzai: Sign in with SSO** |
| IdP | `https://bonzai.iodigital.com/oauth/saml` → `auth.hosted-tools.com` |
| Synced data | `/api/user`, `/api/projects/accessible`, `/api/balance`, `/api/models` |
| Project scope | `projectId` query (default `-1`); **Bonzai: Select Project** |
| Token storage | VS Code `SecretStorage` (never logged) |

### How sign-in works (automated)

Bonzai’s SPA uses an HttpOnly refresh cookie plus a Bearer access token from `POST /api/auth/refresh`. The system browser’s cookies are not visible to Electron, so Takumi automates SSO in the **integrated browser**:

1. Accounts → **Sign in to Bonzai...**
2. Takumi opens a normal **Integrated Browser** tab at `/oauth/saml` (status-bar progress with a command — never a notification overlay)
3. Click through SAML at `auth.hosted-tools.com` in that tab
4. After redirect back to `bonzai.iodigital.com`, Takumi runs `POST /api/auth/refresh` in that page (cookie jar) and stores the Bearer token
5. The Accounts menu shows your Bonzai account; APIs use `Authorization: Bearer …`

No token paste is required on desktop. Takumi does **not** open the system browser for SSO — Bonzai’s HttpOnly cookies only exist in the browser that completed login, so an external Chrome/Edge session cannot be reused inside Takumi.

When the stored Bearer expires, Bonzai’s `POST /api/auth/refresh` often returns HTTP 200 with plain text `Refresh token not provided` (cookie missing in Electron). Accounts → **Bonzai: Account...** then asks you to **Sign in** again instead of showing a JSON parse error.

**Note:** Workbench notifications pause the integrated browser (`Paused due to Notification`). SSO progress must stay in the status bar only so login links remain clickable.

### Settings

| Setting | Default | Purpose |
|---------|---------|---------|
| `ax.port` | `7070` | Local ax web API port |
| `ax.bind` | `127.0.0.1` | Bind address for ax web |
| `ax.autoStartWeb` | `true` | Start ax web API with Takumi |
| `ax.autoWireMcp` | `true` | Write workspace MCP on activate |
| `ax.autoStartMcp` | `true` | Start ax MCP stdio on activate (live tools for agents) |
| `ax.lifecycle.enabled` | `true` | Binary + install + init + sync |
| `ax.lifecycle.syncWatch` | `true` | Prefer `ax sync --watch` |
| `ax.lifecycle.syncIntervalMinutes` | `5` | Fallback sync interval |
| `ax.legacyEmbedUi` | `false` | SPA iframe (troubleshoot only) |
| `ax.cliPath` | _(empty)_ | Override path to `ax` / `ax.exe` |

## Related

- [CLI reference](/reference/cli/) — `ax install`, `ax init`, `ax sync`
- [MCP server](/reference/mcp-server/) — tools exposed to agents
- [Integrations](/reference/integrations/) — per-agent MCP wiring
