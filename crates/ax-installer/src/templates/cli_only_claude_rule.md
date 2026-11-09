# AX CLI-only

When AX MCP is unavailable, use the AX CLI exclusively.

1. `ax session start --json` or resume `--session`.
2. `ax turn begin --session <id> --prompt-stdin --json` (or `ax preflight --prompt-stdin`).
3. Prefer `ax explore` / `ax query` / `ax node` for structural questions.
4. Before Write/Delete with CRITICAL rules: `ax policy guard <path>`.
6. Durable directives: `ax policy capture` (propose) then `--yes` only after confirm; load skills with `ax policy skill <name>`.
5. After edits: `ax sync`. Memory: `ax remember` / `ax recall`.
6. Finish with `ax turn finish` even when no files change.
