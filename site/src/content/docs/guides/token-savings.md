---
title: Token savings
description: How ax estimates context-token savings from MCP graph queries vs reading full files.
---

![Context savings — tokens saved, cost reduction, graph call metrics, highlights, and daily activity heatmap](/screenshots/cc-savings-dashboard.png)

ax replaces blind file crawls with targeted graph queries. Each MCP graph call logs how much context it returned versus how many tokens Read/Grep without ax would have cost.

## Formula

```text
saved(call) = max( counterfactual_tokens − response_tokens, 0 )
```

| Symbol | Meaning |
|---|---|
| **counterfactual_tokens** | Sum over unique files the graph response referenced — whole-file BPE by default, or symbol line span when configured |
| **response_tokens** | Measured o200k BPE count of the MCP response text |

Policy tools (`ax_preflight`, `ax_rules`, …) are logged but excluded from savings totals — they are not Read substitutes.

## File discovery (counterfactual)

ax walks the **structured MCP JSON** (not just text) and collects:

- Nodes with `filePath` + `startLine` / `endLine` (explore, node, callers, …)
- `codeBlocks` from `ax_context` (including inline `content` when files are unreadable)
- Path-only arrays: `relatedFiles`, `files`, `affected`, …
- Paths without line numbers still count (whole-file Read baseline)

Previously, file paths without `endLine` were skipped — those are included now.

## What is measured vs estimated

| Metric | Source |
|---|---|
| Graph response tokens | **Measured** — o200k BPE over MCP `content[0].text` |
| Counterfactual (readable file) | **Measured** — BPE over whole file or line range (mode-dependent) |
| Counterfactual (unreadable file) | Heuristic — line span × 9, inline `content`, or 3500-token average |
| Tokens saved | Per-call `max(0, counterfactual − response)`, then summed |

## Counterfactual mode

Set `AX_SAVINGS_CF_MODE` to choose the Read baseline per file:

| Mode | Baseline |
|---|---|
| `full` (default) | Whole file BPE — matches Cursor **Read** without offset |
| `range` | Symbol line span BPE when start+end are known |
| `max` | Per file: max(whole file, line span) |

## CLI

```bash
ax savings                          # month-to-date summary
ax savings --period week --json
ax savings import --all             # import Cursor + Claude Code logs
```

Data is stored in `~/.ax/usage.db` (local only — no query strings or response bodies).

## Command Center

The sidebar **Savings** tab (toggle visibility in Settings) shows hero stats, heatmap, daily trends (saved / reduction / compare / weekday / table), tool audit, by-project, recent calls, agent sessions, and methodology.

## Agent log import

Import local session logs to correlate tool-call patterns:

| Agent | Path |
|---|---|
| Claude Code | `~/.claude/projects/*/*.jsonl` |
| Cursor | `~/.cursor/projects/*/agent-transcripts/*.jsonl` |

## Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `AX_SAVINGS_CF_MODE` | `full` | Counterfactual baseline: `full`, `range`, or `max` |
| `AX_SAVINGS_CHARS_PER_TOKEN` | 4 | Fallback chars/token when BPE unavailable |
| `AX_SAVINGS_TOKENS_PER_LINE` | 9 | Tokens per line for unreadable files |
| `AX_SAVINGS_AVG_FILE_TOKENS` | 3500 | Fallback when no line count or path-only ref |

## Related

- [MCP server reference](/reference/mcp-server/) — graph tools that generate savings
- [`ax savings` CLI](/reference/cli/#ax-savings)
