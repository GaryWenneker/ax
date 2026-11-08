# AX CLI-only mode

Use AX without MCP, plugins, IDE hooks, daemons, or network listeners. The CLI talks to the same shared engines MCP uses (preflight, sessions, durable context, memory, activity, cache).

## Install

```bash
ax install --mode cli-only --target vscode
ax install --mode cli-only --target cursor
```

This writes Copilot/Cursor instructions and the shared `.agents/skills/ax-cli` skill. It does **not** write MCP configs or install hooks.

## Agent loop

```bash
SESSION=$(ax session start --json | jq -r .session)
ax preflight --prompt "$PROMPT" --json >/dev/null
TURN=$(printf '%s' "$PROMPT" | ax turn begin --session "$SESSION" --prompt-stdin --json | jq -r .turnId)
# … ax explore / ax query / ax node …
printf '%s' '{"outcome":"…","decisions":[]}' \
  | ax turn finish --session "$SESSION" --turn "$TURN" --outcome-stdin --json
```

Research turns with no file changes still produce history and a Memory row with `source=cli-turn` (Command Center shows a **cli** badge).

Optional durable transcript / handoff:

```bash
ax durable append --session "$SESSION" --kind user --body-stdin <<<'Continue auth'
ax durable fork --session "$SESSION" --json
ax cache status
```

## Diagnostics

```bash
ax doctor --mode cli --json
```

## Windows

See `docs/specs/cli-mcp-parity/examples/cli-only-turn.ps1`.

## Command Center

Logging shows CLI activity under the `CLI` / `Hook` / `Agent` filters alongside MCP events. Provenance is never rewritten to look like MCP.
