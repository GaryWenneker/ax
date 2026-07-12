---
title: Token savings
description: How ax estimates context-token savings from MCP graph queries vs reading full files.
---

ax replaces blind file crawls with targeted graph queries. Each MCP graph call logs how much context it returned versus how many tokens a full-file read would have cost.

## Formula

```text
tokens_saved ≈ (F × T_file) − T_graph
```

| Symbol | Meaning |
|---|---|
| **F** | Unique files referenced in the graph response (entry points, callers, callees) |
| **T_file** | Estimated tokens per file — `max(end_line) × 9` from indexed nodes, or `AX_SAVINGS_AVG_FILE_TOKENS` (default 3500) |
| **T_graph** | Graph response size — `response_chars / AX_SAVINGS_CHARS_PER_TOKEN` (default 4 chars per token) |

Policy tools (`ax_preflight`, `ax_rules`, …) are logged but excluded from savings totals — they are not Read substitutes.

## What is measured vs estimated

| Metric | Source |
|---|---|
| MCP response chars | Exact (from MCP `content[0].text` length) |
| Graph response tokens | Estimated (chars ÷ 4) |
| Counterfactual file count | From unique `filePath` values in structured MCP JSON |
| Counterfactual file tokens | Heuristic (lines × 9 or fallback average) |
| Tokens saved | `max(0, counterfactual − response)` |

## CLI

```bash
ax savings                          # month-to-date summary
ax savings --period week --json
ax savings import --all             # import Cursor + Claude Code logs
```

Data is stored in `~/.ax/usage.db` (local only — no query strings or response bodies).

## Command Center

Enable **Show Savings page** in Settings (beta). The sidebar **Savings** tab shows:

- Estimated tokens saved
- MCP call counts and file reads avoided
- Per-tool breakdown
- Daily rollup
- Imported agent sessions (Read/Grep vs ax ratio)

## Agent log import

Import local session logs to correlate tool-call patterns:

| Agent | Path |
|---|---|
| Claude Code | `~/.claude/projects/*/*.jsonl` |
| Cursor | `~/.cursor/projects/*/agent-transcripts/*.jsonl` |

Claude sessions include input token totals. Cursor transcripts report tool-call ratios only (no per-turn token usage in the log format).

## Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `AX_SAVINGS_CHARS_PER_TOKEN` | 4 | Chars per token for response size estimate |
| `AX_SAVINGS_TOKENS_PER_LINE` | 9 | Tokens per line for per-file estimate |
| `AX_SAVINGS_AVG_FILE_TOKENS` | 3500 | Fallback when line count is unavailable |

## Related

- [MCP server reference](/reference/mcp-server/) — graph tools that generate savings
- [`ax savings` CLI](/reference/cli/#ax-savings)
