# SPEC: CLI Logging direction parity (inbound / outbound / internal)

**Path:** `/Users/gary/io/ax/docs/specs/cli-logging-direction-parity.md`  
**Tier:** 2 (Logging UX + CLI observability; no money/auth)  
**Spec approval:** autonomous run initially; shipping after Windows Logging still showed 0 in/0 out (ToolTrace was never pushed). Extended to policy index/match/guard (`ax_policy_index` / `ax_rules` / `ax_guard`).

## Goal

Command Center Logging must show **Inbound**, **Outbound**, and **Internal** when the user drives AX via CLI the same way MCP does — not only Internal · CLI / Policy / Memory rows.

## Root cause (verified)

- Logging classifies `inbound tool=` / `outbound tool=` as inbound/outbound.
- MCP writes those lines from `push_inbound` / `push_outbound` (`[ax-mcp]` brand).
- CLI writes domain lines (`cli` / `policy` / `memory`) which map via `traceDirection` to **internal** only.
- Filter chips hide kinds with count 0, so Inbound/Outbound chips never appear on a CLI-only session.
- Gaps: `ax policy skill` / `ax policy skills` write **no** verbose log; `ax node` / `ax query` only get the end-of-command activity dual-write.

## Non-goals

- Do **not** brand CLI traffic as `[ax-mcp]` (cli-mcp-parity: no faking MCP).
- Do not remove existing domain (`cli`/`memory`/`policy`) lines — they remain Internal detail.
- Do not change MCP transport or tool handlers.

## Acceptance

```gherkin
Given a project with `.ax/` and verbose logging on
When `ax explore "logging"` runs
Then the verbose log contains a line matching `inbound tool=ax_explore`
And a line matching `outbound tool=ax_explore`
And an Internal-classified domain or activity line still exists (cli/activity)

Given the same project
When `ax remember "parity note"` and `ax recall "parity"` run
Then inbound/outbound lines exist for `ax_remember` and `ax_recall`
And memory domain lines may still appear as Internal

Given the same project
When `ax policy skill startup` (or any indexed skill) runs
Then inbound tool=ax_skill and outbound tool=ax_skill are written
When `ax policy skills` runs
Then inbound/outbound for skill listing are written (tool=ax_skill)

Given a log line `[ax] inbound tool=ax_explore args={...}`
When Logging parses it
Then kind is inbound (not other/cli) — CLI brand is fine
```

## Setup

- Branch: current working tree (no worktree — need `.ax` + target-dev).
- New deps: none.
- Files: `crates/ax-usage/src/tool_trace.rs` (+ export), CLI command call sites, `mcpTrace.test.ts` classify case, this SPEC/EVIDENCE.
- Git: no checkpoint commits unless user asks.

## Invariants

- Existing MCP inbound/outbound format and brand unchanged.
- `verbose_enabled` / `append_verbose_log` remain the write path.
- Existing domain_log helpers stay.
