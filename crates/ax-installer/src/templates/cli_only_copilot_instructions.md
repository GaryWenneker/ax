# AX CLI-only workflow (GitHub Copilot)

Use the AX CLI for project context, sessions, turns, memory, and code intelligence.
Do not require MCP, plugins, IDE hooks, daemons, or network listeners.

## Every task

1. Ensure a session: `ax session start --json` (or resume with `--session <id>`).
2. Begin a turn: `printf '%s' "$PROMPT" | ax turn begin --session <id> --prompt-stdin --json`.
3. Read the returned preflight (rules, skills, memories, working context).
4. Prefer `ax explore`, `ax query`, `ax node`, `ax callers`, `ax impact` over broad filesystem search.

## During the task

- Record findings: `printf '%s' '{"kind":"finding","body":"..."}' | ax turn event --session <id> --turn <turn> --input-stdin`.
- Update working context when facts/decisions change: `ax session add --session <id> --input-stdin`.
- Use `ax policy guard` before risky writes when CRITICAL rules require it; `ax policy capture` / `ax policy skill` for directives and skill bodies.

## Finish

1. Summarize the real outcome (including research-only work with no file changes).
2. `printf '%s' '{"outcome":"...","decisions":[],"memories":[]}' | ax turn finish --session <id> --turn <turn> --outcome-stdin --json`.
3. Verify the receipt (`memory_id`, `idempotent`) before the user-facing reply ends.

Turns without file changes must still be finished when useful work occurred.

**Memory:** Working notes for this chat stay in `ax session`. They do not go into the vault. A durable decision, fix, or convention is `ax remember --trigger <name>`. Before re-deriving, fetch that name with `ax recall --cue <name>`. If the user names a trigger, that fetch comes before explore. Free-text recall is the fallback. Questions about old graph changes, logs, or past turns use `--depth archive` on `ax recall` and `ax history`. Preflight will not contain those hits. A candidate long-term memory is confirmed with `ax remember --confirm <id>`. The old engram stays.
