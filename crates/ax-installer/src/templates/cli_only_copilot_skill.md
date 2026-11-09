---
name: ax-cli
description: AX CLI-only session and turn lifecycle for agents without MCP.
---

# AX CLI skill

Follow the AX CLI-only workflow in `.github/copilot-instructions.md` and the shared `.agents/skills/ax-cli/SKILL.md`.

Always: `ax session` → `ax turn begin` → honor preflight → `ax explore` / work → `ax turn finish`.

CLI lifecycle (no MCP): `ax preflight` or `ax turn begin` → `ax explore` / `ax query` / `ax node` → `ax policy guard <path>` before writes → `ax policy capture` / `ax policy skill` as needed → `ax sync` after edits → `ax remember` / `ax recall` as needed → `ax turn finish`.
