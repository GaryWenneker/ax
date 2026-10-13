---
title: MCP Logging & Quality
description: End-to-end guide to verbose MCP traces, the Logging page, Quality slide-out, session hooks, and ax mcp audit.
---

ax ships an **MCP observability loop**: record what agents send and receive, score how well they use ax tools, and turn waste into a fixpack you can paste back into chat.

![MCP Logging — live verbose stream with kind filters and Call Inspector](/screenshots/cc-logging-dark.png)

![MCP Quality — correlation, enrichment, tool mix, findings, and Copy fixpack](/screenshots/cc-mcp-quality-dark.png)

## What the loop is

```text
Agent turns write `<project>/.ax/mcp-verbose-YYYY-MM-DD.log` (always on; one file per calendar day; boundary = Settings timezone)
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

## Domain events

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

Domain lines are always written to the project’s dated verbose log.

Filter these on the Logging page kind chips (also via `/logging?kind=lsp`). Quality / `ax mcp audit` can raise:

| Check | When |
|-------|------|
| `ShipCiFailed` | `ship-ci status=failed` in window |
| `PluginExtractErrors` | `plugin … fail` lines |
| `LspAvailableUnused` | Runnable language server on PATH (`--version` ok), no `lsp enrich` while MCP is active |
| `ShareReadonlyWrite` | Mutating API blocked in share/read-only |
| `EmbedBackend` | Informational: `embed backend=…` line |

Activity StatusBar rows deep-link to Logging with the matching kind filter.

## 1. Confirm the project log

Verbose MCP logging is always enabled. Reconnect the MCP server after changing its project path so activity belongs to the intended project. `AX_MCP_VERBOSE=1` remains compatible with existing client configurations.

Lines go to:

- `<project>/.ax/mcp-verbose-YYYY-MM-DD.log` (authoritative for Logging + audit; legacy `mcp-verbose.log` is migrated on first access)
- Optionally Cursor **Output → MCP** (stderr)

When a Cursor session id is known, lines include `session=<uuid>` for tighter correlation.

## 2. Logging page (`ax web` → Logging)

Open via the status-bar **Logging** chip, or navigate to `/logging`.

Rows summarize routine activity as readable actions: **Workspace sync completed** with **19 files · 162 ms**, or **Git history captured** with **20 scanned · 2 captured**. A small marker sits in its own gutter. Only related call groups are connected; unrelated events have no dotted timeline. The Context column keeps kind/tool filters and avoids repeating an identical domain/tool label.

The original entry is preserved. Search the complete message or filter Event by a raw field such as `duration_ms=162`; select the row and expand **Raw line** to inspect or copy exactly what Ax logged. Unrecognized or failed domain messages retain their text.

| Control | Purpose |
|---|---|
| **Kind chips** | Filter Inbound / Outbound / Preview / Error / Internal / Enrich / plugin / lsp / ship / share / workspace / memory / policy / cli / embed / action |
| **Has text chip** | Show only events with a human-readable text payload (`prompt`, `query`, `text`, `message`, or `q` — including nested JSON paths and flat `key=value` fields). Matching rows show direction markers and a key badge (`prompt` / `query` / …) in Context; the Call Inspector opens with a **Prompt / text** hero section. URL alias: `?hasQuery=1` still works. |
| **Date dropdown** | Filter by calendar day (`YYYY-MM-DD` in the configured timezone); Time column shows date + clock |
| **Timezone** | Set under **Settings → Appearance → Timezone** (IANA, e.g. `Europe/Amsterdam`). Controls Logging display and the configured daily rotation zone. Timestamps inside lines stay UTC. With an empty/local setting, display uses the browser timezone and file rotation uses the host timezone; these can differ when accessed remotely. |
| **Tool dropdown** | Filter by tool name (`ax_preflight`, `ax_explore`, …). Domain/CLI lines also fill TOOL: `memory`, `policy`, `workspace`, `lsp`, `ship-ci`, `cli:explore`, … (from `tool=` on the log line or inferred from `[ax] …` / `cli cmd=`). |
| **Text search** | Free-text over summary / meta |
| **Project switcher** | Use the app header or status bar to switch workspace and its verbose log |
| **Newest / Scroll to new** | Table is **newest-first**. Stay pinned to the top for live updates, or use **Scroll to new** (toolbar + floating chip) after scrolling into history |
| **Older days** | Scroll toward the bottom to load previous dated log files |
| **Status bar** | Live in / out / prev / err / event counts; muted danger tint when offline |
| **Quality** | Opens the quality panel; the button includes the current score |
| **Call Inspector** | Tap a row (or Enter) for pretty-printed JSON/XML and `key=value` fields |

![Logging inspector with readable call steps and the original raw line](/screenshots/cc-logging-inspector-dark.png)

Keyboard: `↑↓` / `j` `k` to move (down = older), Home = newest, End = oldest, Enter to inspect, Esc / `b` to back.

Offline / reconnecting: existing log rows remain readable while Ax reconnects. The live indicator and status bar show connection state.

## 3. Quality slide-out (Q chip)

The status-bar **Q** chip shows a live score (and a critical badge when needed). Click it (or the Logging **Quality** button) for:

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

If correlation stays near 0%: confirm the hook ran (`type %USERPROFILE%\.ax\active-cursor-session`), reconnect ax MCP with the intended project path, and pass `--session <uuid>` when auditing a specific chat.

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
| **GuardBeforeWrite** | low | Write/StrReplace/Delete/EditNotebook ran with no `ax_guard` (read-only windows are never flagged) |
| **UncorrelatedTool** | high / medium / info | Transcript ax tools that do not line up with verbose clusters; **info** (no score hit) when verbose logging is enabled but MCP wrote nothing in-window |
| **VerboseGap** | high / medium | Weak correlation when MCP clusters exist but don't match transcript calls |

Auditor softens false positives when possible: recovered MCP errors (same-tool success within 60s), untimed whole-session Read/Grep tails while verbose is idle, enrich side-channels attached to preflight clusters, domain `[ax] workspace|lsp|…` lines (not counted as MCP tool clusters), preferring the transcript with the most ax activity (not the newest empty chat), and skipping ExploreBeforeGrep / LspAvailableUnused when there is no MCP inbound in the window.

## 6. Copy fixpack

**Copy fixpack** builds a Markdown brief from the current findings (what failed, why it wastes tokens, suggested policy / habit fixes). Paste it into an agent chat so the agent can update rules, strengthen Explore-before-Grep, or fix guard coverage.

## 7. End-to-end playbook

```bash
# One-time
ax savings hook install
# Logging is always enabled; reconnect MCP to confirm the project path
# Restart ax MCP in Cursor

# During / after a session
ax web --open                    # Logging page + Q chip
ax mcp audit                     # CLI report; exit 2 if critical
ax mcp audit --json              # machine-readable
```

Related surfaces:

- [Token savings](/guides/token-savings/) — heatmap, TokenViz path graph, import, pricing
- [Command Center](/guides/command-center/) — Logging page in the page table
- [MCP Server](/reference/mcp-server/) — lean responses + logging and stderr options
- [CLI](/reference/cli/#ax-mcp-audit) — `ax mcp audit` flags
- [Policy Engine](/guides/policy-engine/) — rules that raise Explore-before-Grep / guard scores

## Troubleshooting

| Symptom | Fix |
|---|---|
| Logging empty | Reconnect MCP with the intended project path; confirm today's `<project>/.ax/mcp-verbose-YYYY-MM-DD.log` grows |
| Q score stuck / muted | Need inbound traffic + verbose file; open Logging and trigger a preflight |
| ExploreBeforeGrep false alarm | Install session hook; prefer timed transcripts; re-run audit after a real ax-heavy chat; quiet/DEGRADED MCP should not flag this |
| Correlation near 0% | Wrong project; MCP disconnected; missing `session=` (reinstall hook / restart MCP); or audit without matching transcript — pass `--session` |
| UncorrelatedTool (medium) despite a project log | Usually ax MCP is offline after a reinstall — reconnect MCP; live rolling audits no longer score-penalize this when `verbose_mcp=true` |
| LspAvailableUnused | Run `ax lsp enrich` (or Unresolved → Enrich with LSP) while MCP is active; domain-only windows no longer false-flag |
| Fixpack empty | No findings in window — widen `--window-minutes` or run a fuller agent turn first |
