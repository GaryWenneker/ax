## AX CLI-only

When MCP is not available, use the AX CLI for preflight, sessions, turns, memory, and codegraph:

- `ax preflight --prompt-stdin --session <id> --json`
- `ax session start|get|add|compact|…`
- `ax turn begin|event|finish|fail|status`
- `ax durable …`, `ax cache …`, `ax doctor --mode cli`

See `.agents/skills/ax-cli/SKILL.md`.

CLI lifecycle (no MCP): `ax preflight` or `ax turn begin` → `ax explore` / `ax query` / `ax node` → `ax policy guard <path>` before writes → `ax policy capture` / `ax policy skill` as needed → `ax sync` after edits → `ax remember` / `ax recall` as needed → `ax turn finish`.

**Memory:** Working notes for this chat stay in `ax session`. They do not go into the vault. A durable decision, fix, or convention is `ax remember --trigger <name>`. Before re-deriving, fetch that name with `ax recall --cue <name>`. If the user names a trigger, that fetch comes before explore. Free-text recall is the fallback. Questions about old graph changes, logs, or past turns use `--depth archive` on `ax recall` and `ax history`. Preflight will not contain those hits. A candidate long-term memory is confirmed with `ax remember --confirm <id>`. The old engram stays.
