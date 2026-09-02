# SPEC: Verbose MCP logging lives on Settings

**Tier:** 2 (UI placement; same `saveShipConfig` / `[ui].verbose_mcp` contract)
**Isolation:** none (landing tree; no new dependencies)
**spec approval:** not obtained (autonomous run — user request: enable verbose logging in Settings, not Command Center / Logging)

## Goal

The on/off switch for Verbose MCP logging is **Settings → Interface**. The **Logging** page is a viewer only (no switch). **Command Center** (`/ship`) stays without this toggle.

## Scenarios

### S1 — Settings has the switch

Given `crates/ax-web/web-ui/src/pages/Settings.tsx`
When inspecting the Interface section
Then there is a `SettingRow` titled `Verbose MCP logging`
And a `Toggle` with `aria-label="Verbose MCP logging"`
And toggling calls `saveShipConfig` with `ui.verbose_mcp` (same pattern as Show Savings)

### S2 — Logging has no switch

Given `crates/ax-web/web-ui/src/pages/Logging.tsx`
Then there is no `role="switch"` / `aria-label="Verbose MCP logging"`
And when verbose is off, copy points the user to Settings (button/link using `navigateRoute({ page: 'settings' })`)

### S3 — Docs

Given README and site guides
Then they say **Settings → Interface → Verbose MCP logging**, not **Logging → Verbose MCP logging**

## Invariants

- Existing API: `GET`/`PUT` ship config `ui.verbose_mcp` unchanged
- `AX_MCP_VERBOSE` still overrides as today
- Logging still tails `.ax/mcp-verbose-*.log` and still reads `verboseEnabled` for empty-state copy
- No new npm/cargo dependencies

## Setup

- Files: Settings/Logging/McpTraceLive/McpQualitySlideout, site docs, README, this spec, `tools/gauntlet-verbose-mcp-settings.sh`
- Git: no checkpoint commits unless the user asks
