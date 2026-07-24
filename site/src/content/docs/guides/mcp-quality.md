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

## 1. Enable verbose logging

Pick one:

| Method | Effect |
|---|---|
| **Settings → Interface → Verbose MCP logging** | Writes `[ui] verbose_mcp = true` to `.ax/ship.toml` for the **active** project |
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
| **Kind chips** | Filter Inbound / Outbound / Preview / Error / Internal / Enrich |
| **Has query chip** | Show only events whose JSON payload has a top-level `query` property (e.g. `ax_search` / `ax_explore` args). Matching rows get a blue left edge + a **query** badge in Summary; Meta shows `json · query`. Click the badge or the chip to filter. |
| **Date dropdown** | Filter by calendar day (`YYYY-MM-DD` in the configured timezone); Time column shows date + clock |
| **Timezone** | Set under **Settings → Interface → Timezone** (IANA, e.g. `Europe/Amsterdam`). Controls Logging Date/time display **and** daily log file rotation (midnight boundary). Timestamps inside log lines stay UTC. Default: browser local. |
| **Tool dropdown** | Filter by tool name (`ax_preflight`, `ax_explore`, …) |
| **Text search** | Free-text over summary / meta |
| **Project switcher** | Tail another recent workspace’s verbose log |
| **Status bar** | Live in / out / prev / err / event counts; muted danger tint when offline |
| **Header waves** | Azure CSS waves under the title bar on every page (same language as the site nav) |
| **Call Inspector** | Tap a row (or Enter) for pretty-printed JSON/XML and `key=value` fields |

Keyboard: `↑↓` / `j` `k` to move, Enter to inspect, Esc / `b` to back.

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

- Verbose lines can carry `session=<uuid>`
- `ax savings import --all` keeps the tagged model for by-model rollups
- `ax mcp audit` prefers the ax-heaviest transcript and aligns windows more accurately

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
| **ExploreBeforeGrep** | high / medium | Read/Grep without graph tools, or heavy Read/Grep vs `ax_explore` |
| **GuardBeforeWrite** | low | Busy ax traffic with no `ax_guard` |
| **UncorrelatedTool** | high / medium | Transcript ax tools that do not line up with verbose clusters |
| **VerboseGap** | high / medium | Weak correlation or missing verbose coverage for transcript activity |

Auditor softens false positives when possible: recovered MCP errors (same-tool success within 60s), untimed whole-session Read/Grep tails while verbose is idle, enrich side-channels attached to preflight clusters, and preferring the transcript with the most ax activity (not the newest empty chat).

## 6. Copy fixpack

**Copy fixpack** builds a Markdown brief from the current findings (what failed, why it wastes tokens, suggested policy / habit fixes). Paste it into an agent chat so the agent can update rules, strengthen Explore-before-Grep, or fix guard coverage.

## 7. End-to-end playbook

```bash
# One-time
ax savings hook install
# Command Center → Settings → Verbose MCP logging = on
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
| ExploreBeforeGrep false alarm | Install session hook; prefer timed transcripts; re-run audit after a real ax-heavy chat |
| Correlation near 0% | Wrong project selected; verbose off; or audit without matching transcript — pass `--session` |
| Fixpack empty | No findings in window — widen `--window-minutes` or run a fuller agent turn first |
