## AX CLI-only

When MCP is not available, use the AX CLI for preflight, sessions, turns, memory, and codegraph:

- `ax preflight --prompt-stdin --session <id> --json`
- `ax session start|get|add|compact|…`
- `ax turn begin|event|finish|fail|status`
- `ax durable …`, `ax cache …`, `ax doctor --mode cli`

See `.agents/skills/ax-cli/SKILL.md`.
