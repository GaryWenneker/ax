# AX CLI-only (Cline / Antigravity)

When AX MCP is unavailable or the project is CLI-only, use the AX CLI exclusively.

1. `ax session start --json` or resume `--session`.
2. `printf '%s' "$PROMPT" | ax turn begin --session <id> --prompt-stdin --json`.
3. Honor returned preflight rules/skills/memories.
4. Prefer `ax explore` / `ax query` / `ax node` over Grep/Read for structural questions.
5. Record decisions with `ax turn event` and finish with `ax turn finish` even when no files change.

Do not install or start MCP, plugins, daemons, or network listeners for this workflow.
See `.agents/skills/ax-cli/SKILL.md`.
