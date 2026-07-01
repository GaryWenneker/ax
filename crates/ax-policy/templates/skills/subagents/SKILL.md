---
name: subagents
description: Mandatory ax MCP workflow for Cursor Task and background subagents. Use when delegated via Task tool — preflight is required, not optional.
triggers: ["Task tool", "subagent", "background agent", "run_in_background", "explore agent"]
tags: ["subagents", "preflight"]
priority: 95
---
# ax Subagent Protocol

> **MANDATORY ax MCP WORKFLOW** — IDE-agnostic policy via `.ax/policy/` + MCP preflight.

## Turn order

```text
1. ax_preflight(prompt, files)     [once per turn — inject has full rule/skill bodies]
2. Work — CRITICAL rules binding; ax_guard before writes
3. Code questions — ax_explore (not policy files on disk)
```

## First action (subagent)

You are a subagent if you received a delegated Task prompt. Your **first tool call** must be:

```json
ax_preflight({ "prompt": "<verbatim user intent in English>", "files": [] })
```

Then follow matched CRITICAL rules and any skill workflows from `inject`.

## Parent agent checklist

Before every `Task` invocation, paste into the Task `prompt`:

> Read the `subagents` skill via `ax_skill({ name: "subagents" })` and follow it exactly as your very first action. ax MCP is mandatory.

Include `## User prompt (verbatim)` with the user's full message.

## MCP failure

Report `ax MCP unreachable`, state `Mode: DEGRADED`, continue best-effort only.
