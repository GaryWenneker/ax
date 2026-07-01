---
name: startup
description: Runs the mandatory ax session-start sequence (ax_preflight with matched rules and skills). Use at the start of every new chat or user message when .ax/policy/ is indexed.
triggers: ["session start", "new message", "preflight", "startup", "turn start"]
tags: ["workflow", "preflight"]
priority: 100
---
# ax Startup Protocol

> **ABSOLUTE — NO EXCEPTIONS.** Every user message starts with preflight when policy is enabled. Skipping preflight is a protocol violation.

## SS-00 — Preflight (unconditional, always first)

> **Run preflight exactly once per turn.** If you already called `ax_preflight` this turn, skip this step. Do not call preflight again after reading this skill.

Run as the **very first MCP tool call** on every new user message, before reading files, searching code, or other work:

```json
ax_preflight({ "prompt": "<full user intent in English>", "files": ["<open or changed paths relative to project root>"] })
```

- Apply all **CRITICAL** rules from `inject` before editing files.
- If a skill matched (including this one), follow its workflow from `inject` — do not re-read `.ax/policy/` on disk.
- `ax_rules` / `ax_skill` are for on-demand loads — preflight already returns full bodies in `inject` on turn start.

## SS-01 — Code context (after preflight)

For structural questions — how code works, call paths, impact, dependencies:

```json
ax_explore({ "query": "<question or symbol names>" })
```

Use `ax_search`, `ax_node`, `ax_callers`, `ax_callees`, `ax_impact` for focused graph queries.

**Policy vs code:** `ax_preflight` = rules/skills. `ax_explore` / `ax_context` = code graph — different tools.

## SS-02 — Pre-write guard

Before Write/StrReplace/Delete on project files when CRITICAL policy rules exist:

```json
ax_guard({ "path": "<relative file path>", "operation": "write" })
```

## SS-03 — Language rule (CRITICAL)

- **All agent responses to the user MUST be in English.**
- Translate non-English user input for your own reasoning; respond in English.

## SS-04 — MCP failure

If preflight fails:

1. Report: `ax MCP unreachable: [error]`
2. State: `Mode: DEGRADED — no policy loaded.`
3. Do not proceed as if policy is active; best-effort only.

## SS-05 — Status line

After successful preflight (one line):

```
ax policy active — N rules, N skills matched
```

Summarise matched counts from the preflight response.
