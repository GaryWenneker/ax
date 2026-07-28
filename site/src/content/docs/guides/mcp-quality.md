---
title: MCP Logging & Quality
description: End-to-end guide to verbose MCP traces, the Logging page, Quality slide-out, session hooks, and ax mcp audit.
---

**ax v3.0.0+** ships an **MCP observability loop**: record what agents send and receive, score how well they use ax tools, and turn waste into a fixpack you can paste back into chat.

![MCP Logging — live verbose stream with kind filters and Call Inspector](/screenshots/cc-logging.png)

![MCP Quality — correlation, enrichment, tool mix, findings, and Copy fixpack](/screenshots/cc-mcp-quality.png)

## What the loop is

```text
Enable Verbose MCP logging
        ↓
Agent turns write `<project>/.ax/mcp-verbose-YYYY-MM-DD.log` (one file per calendar day; boundary = Settings timezone)
        ↓
Command Center Logging page (live SSE table + Call Inspector)
        ↓
Q chip / Quality slide-out (score, findings, tokens at risk)
        ↓
Optional: Cursor sessionStart hook tags session= + model
        ↓
ax mcp audit  (same engine; exit 2 on critical findings)
        ↓
Copy fixpack → paste into agent chat to improve policy / habits
```

Traces **never** alter agent-facing tool responses (`content.text` / `structuredContent`). Logging is side-channel only.

## v4 domain events

When Verbose MCP logging is on, the same daily log also receives domain lines for:

| Prefix | Source |
|--------|--------|
| `plugin` | Extractor plugins during index/sync |
| `lsp` | `ax lsp enrich` / Unresolved → Enrich with LSP |
| `ship-ci` | `ax ship --ci` |
| `ship` | `ax ship --evaluate` / `--draft` / `--watch` |
| `share` | `ax share` / token gate |
| `workspace` | `ax index` / `ax sync` (incl. `--all`) / workspace switch |
| `memory` | `ax remember` / `ax recall` / memory export·import / capture-git |
| `policy` | `ax policy index|import|match|guard|…` |
| `cli` | Graph readouts: `explore` / `impact` / `insights` / `report` / `context` / `status` |
| `embed` | memory embedding backend |
| `action` | Command Center live activity bus |

Domain lines are written only when Verbose MCP logging is on (`[ui].verbose_mcp` or `AX_MCP_VERBOSE`).

Filter these on the Logging page kind chips (also via `/logging?kind=lsp`). Quality / `ax mcp audit` can raise:

| Check | When |
|-------|------|
| `ShipCiFailed` | `ship-ci status=failed` in window |
| `PluginExtractErrors` | `plugin … fail` lines |
| `LspAvailableUnused` | Runnable language server on PATH (`--version` ok), no `lsp enrich` while MCP is active |
| `ShareReadonlyWrite` | Mutating API blocked in share/read-only |
| `EmbedBackend` | Informational: `embed backend=…` line |

Activity StatusBar rows deep-link to Logging with the matching kind filter.

## 1. Enable verbose logging

Pick one:

| Method | Effect |
|---|---|
| **Logging → Verbose MCP logging** | Writes `[ui] verbose_mcp = true` to `.ax/ship.toml` for the **active** project |
| `AX_MCP_VERBOSE=1` | Env on the MCP server process (Cursor MCP config) |

Then **restart / reconnect** the ax MCP server in your agent.

Lines go to:

- `<project>/.ax/mcp-verbose-YYYY-MM-DD.log` (authoritative for Logging + audit; legacy `mcp-verbose.log` is migrated on first access)
- Optionally Cursor **Output → MCP** (stderr)

When a Cursor session id is known, lines include `session=<uuid>` for tighter correlation.

## 2. Logging page (`ax web` → Logging)

Open via the status-bar **Logging** chip, or navigate to `/logging`.

| Control | Purpose |
|---|---|
| **Kind chips** | Filter Inbound / Outbound / Preview / Error / Internal / Enrich / plugin / lsp / ship / share / workspace / memory / policy / cli / embed / action |
| **Has text chip** | Show only events with a human-readable text payload (`prompt`, `query`, `text`, `message`, or `q` — including nested JSON paths and flat `key=value` fields). Matching rows get a blue left edge + a key badge (`prompt` / `query` / …) in Summary; the Call Inspector opens with a **Prompt / text** hero section. URL alias: `?hasQuery=1` still works. |
| **Date dropdown** | Filter by calendar day (`YYYY-MM-DD` in the configured timezone); Time column shows date + clock |
| **Timezone** | Set under **Settings → Interface → Timezone** (IANA, e.g. `Europe/Amsterdam`). Controls Logging Date/time display **and** daily log file rotation (midnight boundary). Timestamps inside log lines stay UTC. Default: browser local. |
| **Tool dropdown** | Filter by tool name (`ax_preflight`, `ax_explore`, …). Domain/CLI lines also fill TOOL: `memory`, `policy`, `workspace`, `lsp`, `ship-ci`, `cli:explore`, … (from `tool=` on the log line or inferred from `[ax] …` / `cli cmd=`). |
| **Text search** | Free-text over summary / meta |
| **Project switcher** | Switch to another recent workspace’s verbose log |
| **Newest / Scroll to new** | Table is **newest-first**. Stay pinned to the top for live updates, or use **Scroll to new** (toolbar + floating chip) after scrolling into history |
| **Older days** | Scroll toward the bottom to load previous dated log files |
| **Status bar** | Live in / out / prev / err / event counts; muted danger tint when offline |
| **Header waves** | Azure CSS waves under the title bar on every page (same language as the site nav) |
| **Call Inspector** | Tap a row (or Enter) for pretty-printed JSON/XML and `key=value` fields |

Keyboard: `↑↓` / `j` `k` to move (down = older), Home = newest, End = oldest, Enter to inspect, Esc / `b` to back.

Offline / reconnecting: log text is **blurred** until the SSE stream is live again.

## 3. Quality slide-out (Q chip)

The status-bar **Q** chip shows a live score (and a critical badge when needed). Click it (or the Logging quality strip) for:

| Section | Meaning |
|---|---|
| **Correlation** | How well verbose lines match the Cursor transcript window |
| **Enrichment** | Inject size p50/p95, empty enrich rate, matched-rules rate, preflight count |
| **Tool mix** | Preflight / Explore / Guard / Graph / Read / Grep counts |
| **Token waste** | Estimated tokens at risk this window |
| **Findings** | Ranked checks with severity and waste hints |
| **Actions** | **Copy fixpack**, Run full session audit, Open Logging, Refresh |

CLI equivalent:

```bash
ax mcp audit
ax mcp audit --window-minutes 60
ax mcp audit --session <uuid>
ax mcp audit --session path/to/transcript.jsonl --json
```

Exit code **2** when critical findings are present (CI-friendly). Snapshots land in `.ax/audit/latest.json`.

## 4. Cursor sessionStart hook (correlation + model tags)

Cursor Composer transcripts often omit the picker model. Install once:

```bash
ax savings hook install
```

This installs hooks under `~/.cursor/hooks/` and merges `sessionStart` into `~/.cursor/hooks.json`. Each new Composer chat runs `ax session-hook` (hidden) so:

- `~/.ax/active-cursor-session` is written **even when Cursor omits the model** (required for `session=` on verbose lines)
- Verbose lines can carry `session=<uuid>`
- `ax savings import --all` keeps the tagged model for by-model rollups (when model is present)
- `ax mcp audit` prefers the ax-heaviest transcript and aligns windows more accurately

If correlation stays near 0%: confirm the hook ran (`type %USERPROFILE%\.ax\active-cursor-session`), reconnect ax MCP after enabling verbose, and pass `--session <uuid>` when auditing a specific chat.

Manual debug tag:

```bash
ax savings tag-session --session-id <uuid> --model composer-2.5-fast
```

## 5. Quality checks (scoring)

Score starts at **100** and drops when findings fire. Main checks:

| Check | Severity | When it fires |
|---|---|---|
| **PreflightOnce** | critical / low | No `ax_preflight` in a window with MCP traffic; or preflight spam |
| **EnrichPresent** | high | Preflight/enrich clusters with empty or missing inject |
| **RulesInjected** | medium | Low `matched_rules` rate on preflight enrichments |
| **ExploreBeforeGrep** | high / medium | Read/Grep without graph tools while MCP inbound is active (skipped in DEGRADED / quiet MCP) |
| **GuardBeforeWrite** | low | Busy ax traffic with no `ax_guard` |
| **UncorrelatedTool** | high / medium / info | Transcript ax tools that do not line up with verbose clusters; **info** (no score hit) when verbose logging is enabled but MCP wrote nothing in-window |
| **VerboseGap** | high / medium | Weak correlation when MCP clusters exist but don't match transcript calls |

Auditor softens false positives when possible: recovered MCP errors (same-tool success within 60s), untimed whole-session Read/Grep tails while verbose is idle, enrich side-channels attached to preflight clusters, domain `[ax] workspace|lsp|…` lines (not counted as MCP tool clusters), preferring the transcript with the most ax activity (not the newest empty chat), and skipping ExploreBeforeGrep / LspAvailableUnused when there is no MCP inbound in the window.

## 6. Copy fixpack

**Copy fixpack** builds a Markdown brief from the current findings (what failed, why it wastes tokens, suggested policy / habit fixes). Paste it into an agent chat so the agent can update rules, strengthen Explore-before-Grep, or fix guard coverage.

## 7. End-to-end playbook

```bash
# One-time
ax savings hook install
# Command Center → Logging → Verbose MCP logging = on
# Restart ax MCP in Cursor

# During / after a session
ax web --open                    # Logging page + Q chip
ax mcp audit                     # CLI report; exit 2 if critical
ax mcp audit --json              # machine-readable
```

Related surfaces:

- [Token savings](/guides/token-savings/) — heatmap, TokenViz path graph, import, pricing
- [Command Center](/guides/command-center/) — Logging page in the page table
- [MCP Server](/reference/mcp-server/) — lean responses + verbose env vars
- [CLI](/reference/cli/#ax-mcp-audit) — `ax mcp audit` flags
- [Policy Engine](/guides/policy-engine/) — rules that raise Explore-before-Grep / guard scores

## Troubleshooting

| Symptom | Fix |
|---|---|
| Logging empty | Enable verbose logging; reconnect MCP; confirm today's `<project>/.ax/mcp-verbose-YYYY-MM-DD.log` grows |
| Q score stuck / muted | Need inbound traffic + verbose file; open Logging and trigger a preflight |
| ExploreBeforeGrep false alarm | Install session hook; prefer timed transcripts; re-run audit after a real ax-heavy chat; quiet/DEGRADED MCP should not flag this |
| Correlation near 0% | Wrong project; verbose off; MCP disconnected; missing `session=` (reinstall hook / restart MCP); or audit without matching transcript — pass `--session` |
| UncorrelatedTool (medium) while verbose is on | Usually ax MCP is offline after a reinstall — reconnect MCP; live rolling audits no longer score-penalize this when `verbose_mcp=true` |
| LspAvailableUnused | Run `ax lsp enrich` (or Unresolved → Enrich with LSP) while MCP is active; domain-only windows no longer false-flag |
| Fixpack empty | No findings in window — widen `--window-minutes` or run a fuller agent turn first |
