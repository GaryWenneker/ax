---
name: ax-cli
description: AX CLI-only session and turn lifecycle for agents without MCP.
---

# AX CLI skill

Follow the AX CLI-only workflow in `.github/copilot-instructions.md` and the shared `.agents/skills/ax-cli/SKILL.md`.

Always: `ax session` → `ax turn begin` → honor preflight → `ax explore` / work → `ax turn finish`.

CLI lifecycle (no MCP): `ax preflight` or `ax turn begin` → `ax explore` / `ax query` / `ax node` → `ax policy guard <path>` before writes → `ax policy capture` / `ax policy skill` as needed → `ax sync` after edits → `ax remember` / `ax recall` as needed → `ax turn finish`.

**Memory:** Working notes for this chat stay in `ax session`. They do not go into the vault. A durable decision, fix, or convention is `ax remember --trigger <name>`. Before re-deriving, fetch that name with `ax recall --cue <name>`. If the user names a trigger, that fetch comes before explore. Free-text recall is the fallback. Questions about old graph changes, logs, or past turns use `--depth archive` on `ax recall` and `ax history`. Preflight will not contain those hits. A candidate long-term memory is confirmed with `ax remember --confirm <id>`. The old engram stays.
