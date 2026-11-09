---
name: subagents
description: Mandatory ax MCP workflow for Cursor Task and background subagents. Use when delegated via Task tool — preflight is required, not optional.
triggers: ["Task tool", "subagent", "background agent", "run_in_background", "explore agent"]
tags: ["subagents", "preflight"]
priority: 95
seedVersion: 6
group: agent-orchestration
---
# ax Subagent Protocol

> **MANDATORY ax WORKFLOW** — IDE-agnostic policy via preflight. Prefer MCP when connected; use CLI when MCP is unavailable or the project is CLI-only.

## Turn order

```text
1. ax_preflight(prompt, files)     [once per turn — inject has full rule/skill bodies]
2. Work — CRITICAL rules binding; `ax_guard` (`path` + `operation`) before writes
3. Code questions — ax_explore (not policy files on disk)
4. Session ops — MCP tools (`ax_sync`, `ax_lsp`, `ax_ship`, `ax_policy_index`, …), never shell `ax …` while MCP is up
```

**CLI-only / MCP unavailable:**

```text
1. ax preflight / ax turn begin
2. ax policy guard <path> before writes when CRITICAL rules require it
3. ax explore / ax query / ax node for structural questions
4. ax sync / ax remember / ax session … for ops
```

## Conversation cache

A subagent has its own conversation, so it does not see the parent's `<ax_session_context>`. The parent pastes the relevant cache ids into the Task prompt; the subagent calls `ax_expand` with them instead of re-running those queries. Inside the subagent's own conversation the usual rule holds: A repeated graph call in this conversation returns a short `[ax cache hit]` reference; read `<ax_session_context>` before searching again, and pass `fresh: true` to force a new query.

Record a durable fact, file, symbol, decision, or open question with `ax_session` (actions `add`, `update`, `compact`, `clear`, `fork`, `handoff`). Preflight shows `<ax_working_context>`; pass its hash as `known_context` and an unchanged snapshot comes back as one line. Pass the `session` id from `<ax_chat>` to every ax call in this chat; a preflight without it starts a new chat. A changed index marks it stale; `compact` confirms the notes against the current index. fork copies these notes to a new session. handoff starts a new session from the note you send. The old session stays readable, and the graph cache is not copied. `ax_durable` stores this chat's transcript, documents, and checkpointed tasks.

## First action (subagent)

You are a subagent if you received a delegated Task prompt. Your **first tool call** must be:

```json
ax_preflight({ "prompt": "<verbatim user intent in English>", "files": [] })
```

Then follow matched CRITICAL rules and any skill workflows from `inject`.

## Parent agent checklist

Before every `Task` invocation, paste into the Task `prompt`:

> Load the `subagents` skill via `ax_skill({ name: "subagents" })` (or `ax policy skill subagents` when CLI-only) and follow it exactly as your very first action. ax workflow is mandatory — MCP when connected, CLI when not.

Include `## User prompt (verbatim)` with the user's full message.

## MCP failure / CLI-only

Report `ax MCP unreachable`, state `Mode: DEGRADED` (skip unreachable wording for intentional CLI-only), then continue with the CLI turn order above — not Grep-first.
