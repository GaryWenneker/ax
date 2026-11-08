---
name: ax-cli
description: Shared AX CLI-only agent workflow for session, turn, memory, and observability parity without MCP.
triggers: ["ax cli", "cli-only", "no mcp", "turn begin", "preflight"]
---

# AX CLI skill

## Required loop

```text
ax session start --json
printf '%s' "$PROMPT" | ax turn begin --session $SID --prompt-stdin --json
# use ax explore / ax query / ax node / ax remember as needed
printf '%s' '{"outcome":"..."}' | ax turn finish --session $SID --turn $TID --outcome-stdin --json
```

## Rules

- Research-only turns (no file changes) must still call `ax turn finish`.
- Never invent token or cost numbers; use measured or mark unknown.
- `ax doctor --mode cli --json` diagnoses CLI-only health.
- Optional hooks may call the same CLI commands but are not required.
