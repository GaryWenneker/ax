---
name: ax-cli
description: Shared AX CLI-only agent workflow for session, turn, graph, policy guard/capture/skill, memory, and observability without MCP.
triggers: ["ax cli", "cli-only", "no mcp", "turn begin", "preflight", "policy guard"]
---

# AX CLI skill

## Required loop

```text
ax session start --json
printf '%s' "$PROMPT" | ax turn begin --session $SID --prompt-stdin --json
# or: printf '%s' "$PROMPT" | ax preflight --prompt-stdin --session $SID --json
# graph: ax explore / ax query / ax node
# before writes: ax policy guard <path>
# after edits: ax sync ; durable notes: ax remember / ax recall
printf '%s' '{"outcome":"..."}' | ax turn finish --session $SID --turn $TID --outcome-stdin --json
```

## Rules

- Research-only turns (no file changes) must still call `ax turn finish`.
- Prefer `ax explore` over Grep/Read for structural questions.
- Call `ax policy guard <path>` before Write/Delete when CRITICAL rules exist.
- Durable directives: `ax policy capture` then `--yes` only after confirm; load skills with `ax policy skill <name>`.
- Never invent token or cost numbers; use measured or mark unknown.
- `ax doctor --mode cli --json` diagnoses CLI-only health.
- Optional hooks may call the same CLI commands but are not required.
