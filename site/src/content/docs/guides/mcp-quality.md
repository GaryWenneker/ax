---
title: MCP Logging & Quality
description: End-to-end guide to verbose MCP and CLI traces, ax mcp audit, quality checks, session hooks, and the Logging page in ax-admin.
---

ax ships an **observability loop** for both MCP and CLI: record what agents send and receive, score how well they use ax tools, and turn the findings into fixes for policy and habits.

## What the loop is

```text
Agent turns write `<project>/.ax/mcp-verbose-YYYY-MM-DD.log` (always on; one file per calendar day)
        ↓
Optional: Cursor sessionStart hook tags session= + model
        ↓
ax mcp audit  (score, findings, tokens at risk; exit 2 on critical findings)
        ↓
ax-admin Logging page / ax_logs  (MCP calls handled for the project; admin grant)
        ↓
Paste the findings into agent chat to improve policy / habits
```

Traces **never** alter agent-facing tool responses (`content.text` / `structuredContent`). Logging is side-channel only.

## Directions: Inbound, Outbound, Internal

Each line in the verbose log has one of three directions:

| Direction | Log shape | Typical source |
|-----------|-----------|----------------|
| **Inbound** | `inbound tool=ax_… args=…` | MCP tool request, or CLI `ToolTrace` for the same tool id |
| **Outbound** | `outbound tool=ax_…` (+ optional preview) | MCP tool response, or CLI `ToolTrace` finish |
| **Internal** | domain / activity lines (`cli`, `policy`, `memory`, …) | Lifecycle detail that is not a request/response pair |

MCP brands those lines `[ax-mcp]`. CLI brands the same body with `[ax]` so provenance stays honest. The direction is the same.

**CLI commands that emit inbound + outbound** (same tool ids agents use over MCP):

| CLI | Tool id on the log line |
|-----|-------------------------|
| `ax explore` | `ax_explore` |
| `ax query` | `ax_search` |
| `ax node` | `ax_node` |
| `ax remember` / `ax recall` | `ax_remember` / `ax_recall` |
| `ax preflight`, `ax session …`, turn helpers via shared engine | `ax_preflight`, `ax_session`, … |
| `ax policy index` | `ax_policy_index` |
| `ax policy match` | `ax_rules` |
| `ax policy guard` | `ax_guard` |
| `ax policy skill` / `ax policy skills` | `ax_skill` |

Domain lines (`cli`, `policy`, `memory`) still appear for the same runs. A CLI-only session can therefore write all three directions after a single `ax explore` or `ax policy match`.

If the log holds no inbound or outbound lines and only domain lines while you use the CLI, the build is older than **ax 8.4.4**. Older builds wrote domain/activity lines only. Install a current build: the headless build is upgraded by running its installer again (see [Headless Linux build](/guides/headless-linux/#install)).

## Domain events

The same daily log also receives domain lines for:

| Prefix | Source |
|--------|--------|
| `plugin` | Extractor plugins during index/sync |
| `lsp` | `ax lsp enrich` |
| `ship-ci` | `ax ship --ci` |
| `ship` | `ax ship --evaluate` / `--draft` (and `--watch`, full ax only) |
| `share` | `ax share` / token gate (full ax only) |
| `workspace` | `ax index` / `ax sync` (incl. `--all`) / workspace switch |
| `memory` | `ax remember` / `ax recall` / memory export·import / capture-git |
| `policy` | `ax policy index|import|match|guard|…` |
| `cli` | Graph readouts: `explore` / `impact` / `insights` / `report` / `context` / `status` |
| `embed` | memory embedding backend |
| `action` | Command Center live activity bus (full ax only) |

Domain lines are always written to the project’s dated verbose log.

`ax mcp audit` can raise these findings from domain lines:

| Check | When |
|-------|------|
| `ShipCiFailed` | `ship-ci status=failed` in window |
| `PluginExtractErrors` | `plugin … fail` lines |
| `LspAvailableUnused` | Runnable language server on PATH (`--version` succeeds within 2 seconds), no `lsp enrich` while MCP is active. The PATH check runs only when the window has MCP calls and no `lsp enrich`, and its result is reused for 10 minutes |
| `ShareReadonlyWrite` | Mutating API blocked in share/read-only (full ax only) |
| `EmbedBackend` | Informational: `embed backend=…` line |

## 1. Confirm the project log

Verbose MCP logging is always enabled. Reconnect the MCP server after changing its project path so activity belongs to the intended project. `AX_MCP_VERBOSE=1` remains compatible with existing client configurations.

Lines go to:

- `<project>/.ax/mcp-verbose-YYYY-MM-DD.log` (authoritative for the audit; legacy `mcp-verbose.log` is migrated on first access)
- Optionally Cursor **Output → MCP** (stderr)

A new file starts at midnight in the timezone set as `timezone` under `[ui]` in `<project>/.ax/ship.toml` (an IANA name such as `Europe/Amsterdam`). When it is empty or `local`, the host timezone is used. Timestamps inside lines stay UTC.

When a Cursor session id is known, lines include `session=<uuid>` for tighter correlation.

CLI tool lines (`[ax] inbound`, `outbound preview`, `error`) are redacted before they are written: string arguments under keys such as `password`, `token`, `secret` or `api_key` become `[redacted]`, and `key=value` secrets, the value after `password:`, `token:`, `api-key:`, `Authorization:` or `Bearer`, `sk-…` and `AKIA…` keys and long key-like runs inside text are replaced the same way. UUIDs and 40- or 64-digit hex commit ids are kept, and so are plain numbers after a marker (`tokens: 1200`). Activity summaries and turn prompts and outcomes use the same rules.

The audit reads only the dated files that can overlap what it checks: for a rolling window, the files from the day before the window start onward; for `--session`, the files from the day before the transcript's first timed event (all files when the transcript has no timestamps). Older files stay on disk by default. To delete them automatically, set `verbose_log_days` or `verbose_log_max_bytes` under `[retention]` in `.ax/ship.toml` (see [History retention](/guides/command-center/#history-retention-retention)); otherwise remove them by hand if you do not want to keep the prompts they contain.

## 2. Logging page in ax-admin

The **Logging** page in [ax-admin](/guides/remote-admin/) lists the MCP calls the environment handled for the project, newest first: time, tool, result, duration, and size. It shows what the environment was asked, never the content: there are no arguments and no result bodies.

![The MCP call log of an environment](/screenshots/ax-admin-logging.png)

Logging is a management page. It appears only when the connection has the admin grant; see [Give a key the admin grant](/guides/remote-admin/#give-a-key-the-admin-grant).

The page reads the `ax_logs` MCP tool, which a credential with the admin grant can also call directly. `limit` (default 100, maximum 500), `before` (the previous page's `nextBefore`), and `tool` (only that tool) are optional:

```text
ax_logs({ "limit": 50, "tool": "ax_explore" })
```

For the full lines, with arguments, previews, and domain events, read the dated log file on the machine that runs ax:

```bash
grep "inbound tool=ax_explore" .ax/mcp-verbose-*.log
```

The full ax, which is being phased out, also has a live Logging page with kind filters, a Call Inspector, and a Quality slide-out in its Command Center; see [Command Center](/guides/command-center/).

## 3. Audit with `ax mcp audit`

`ax mcp audit` correlates the verbose log with a Cursor agent transcript and scores the window:

| Part of the report | Meaning |
|---|---|
| **Score and grade** | Starts at 100 and drops per finding (see [Quality checks](#5-quality-checks-scoring)) |
| **Correlation** | How well verbose lines match the Cursor transcript window |
| **Enrichment** | Inject size p50/p95, empty enrich rate, matched-rules rate, preflight count |
| **Tool mix** | Preflight / Explore / Guard / Graph / Read / Grep counts |
| **Tokens at risk** | Estimated tokens at risk this window |
| **Findings** | Ranked checks with severity and waste hints |

```bash
ax mcp audit
ax mcp audit --window-minutes 60
ax mcp audit --session <uuid>
ax mcp audit --session path/to/transcript.jsonl --json
```

Exit code **2** when critical findings are present (CI-friendly). Snapshots land in `.ax/audit/latest.json`.

Without `--session`, the audit reads transcripts from `~/.cursor/projects/<slug>/agent-transcripts/`, where `<slug>` is the project path in lower case with each run of other characters replaced by one `-`. A folder whose name differs only in case or punctuation also counts; a parent or sibling workspace's folder never does. Subagent chats (in `subagents/` folders) are skipped.

## 4. Cursor sessionStart hook (correlation + model tags)

Cursor Composer transcripts often omit the picker model. Install once:

```bash
ax savings hook install
```

This installs hooks under `~/.cursor/hooks/` and merges `sessionStart` into `~/.cursor/hooks.json`. Each new Composer chat runs `ax session-hook` (hidden) so:

- `~/.ax/active-cursor-session` (one line, the chat id) is written **even when Cursor omits the model** (required for `session=` on verbose lines)
- When the hook knows the window and project, `~/.ax/cursor-sessions/<hash>.json` binds that window to the chat; each window has its own file
- Verbose lines can carry `session=<uuid>`
- `ax savings import --all` keeps the tagged model for by-model rollups (when model is present)
- `ax mcp audit` prefers the ax-heaviest transcript and aligns windows more accurately

If correlation stays near 0%: confirm the hook ran (`cat ~/.ax/active-cursor-session`, or `type %USERPROFILE%\.ax\active-cursor-session` on Windows), reconnect ax MCP with the intended project path, and pass `--session <uuid>` when auditing a specific chat.

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

## 6. Act on findings

Each finding says what failed and why it wastes tokens. Paste the `ax mcp audit` report into an agent chat so the agent can update rules, strengthen Explore-before-Grep, or fix guard coverage. `--json` gives the same findings in machine-readable form.

## 7. End-to-end playbook

```bash
# One-time
ax savings hook install
# Logging is always enabled; reconnect MCP to confirm the project path
# Restart ax MCP in Cursor

# During / after a session
ax mcp audit                     # CLI report; exit 2 if critical
ax mcp audit --json              # machine-readable
# ax-admin → Logging             # MCP calls handled for the project (admin grant)
```

Related surfaces:

- [Token savings](/guides/token-savings/) — savings formula, import, pricing
- [ax-admin](/guides/remote-admin/) — Logging page and the admin grant
- [MCP Server](/reference/mcp-server/) — lean responses + logging and stderr options
- [CLI](/reference/cli/#ax-mcp-audit) — `ax mcp audit` flags
- [Policy Engine](/guides/policy-engine/) — rules that raise Explore-before-Grep / guard scores

## Troubleshooting

| Symptom | Fix |
|---|---|
| Log empty | Reconnect MCP with the intended project path; confirm today's `<project>/.ax/mcp-verbose-YYYY-MM-DD.log` grows. CLI-only: run `ax explore "…"` or `ax policy match "…"` in that project |
| No inbound or outbound lines, only domain lines from the CLI | Need **ax 8.4.4+** for CLI `ToolTrace`. Install a current build, then run a graph or policy command |
| Score does not change | Need inbound traffic + verbose file; trigger a preflight (MCP) or `ax preflight` / `ax explore` (CLI), then run `ax mcp audit` again |
| ExploreBeforeGrep false alarm | Install session hook; prefer timed transcripts; re-run audit after a real ax-heavy chat; quiet/DEGRADED MCP should not flag this |
| Correlation near 0% | Wrong project; MCP disconnected; missing `session=` (reinstall hook / restart MCP); or audit without matching transcript — pass `--session` |
| UncorrelatedTool (medium) despite a project log | Usually ax MCP is offline after a reinstall — reconnect MCP; a rolling-window audit with no MCP calls in the window ignores the transcript calls, because verbose logging is always on |
| LspAvailableUnused | Run `ax lsp enrich` while MCP is active; only `[ax] lsp enrich …` log lines count, not prompts that mention it |
| No findings | Nothing fired in the window — widen `--window-minutes` or run a fuller agent turn first |
| ax-admin shows no Logging page | The connection lacks the admin grant; see [Give a key the admin grant](/guides/remote-admin/#give-a-key-the-admin-grant) |
