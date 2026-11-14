---
name: ax CLI-only
alwaysApply: true
description: AX CLI-only parity workflow (no MCP required)
---
# AX CLI-only

When AX MCP is unavailable or the project is CLI-only, use the AX CLI exclusively.

1. `ax session start --json` or resume `--session`.
2. `printf '%s' "$PROMPT" | ax turn begin --session <id> --prompt-stdin --json`.
3. Honor returned preflight rules/skills/memories.
4. Prefer `ax explore` / `ax query` / `ax node` over Grep/Read for structural questions.
5. Record decisions with `ax turn event` and finish with `ax turn finish` even when no files change.

Do not install or start MCP, plugins, daemons, or network listeners for this workflow.
See `.agents/skills/ax-cli/SKILL.md`.

CLI lifecycle (no MCP): `ax preflight` or `ax turn begin` → `ax explore` / `ax query` / `ax node` → `ax policy guard <path>` before writes → `ax policy capture` / `ax policy skill` as needed → `ax sync` after edits → `ax remember` / `ax recall` as needed → `ax turn finish`.

**Memory:** Working notes for this chat stay in `ax session`. They do not go into the vault. A durable decision, fix, or convention is `ax remember --trigger <name>`. Before re-deriving, fetch that name with `ax recall --cue <name>`. If the user names a trigger, that fetch comes before explore. Free-text recall is the fallback. Questions about old graph changes, logs, or past turns use `--depth archive` on `ax recall` and `ax history`. Preflight will not contain those hits. A candidate long-term memory is confirmed with `ax remember --confirm <id>`. The old engram stays.
