---
title: CLI-only mode
description: Run the same ax preflight, session, turn, and graph engines without MCP, plugins, IDE hooks, or daemons.
---

Use ax when your agent cannot (or must not) load an MCP server. The CLI talks to the **same shared engines** MCP uses: preflight, sessions, turns, memory, graph explore, and activity logging.

## Install instruction surfaces (no MCP configs)

```bash
# Every distinct IDE instruction surface + shared AGENTS skill
ax install --mode cli-only

# Or pick targets
ax install --mode cli-only --target cursor
ax install --mode cli-only --target vscode
ax install --mode cli-only --target windsurf
ax install --mode cli-only --target continue
ax install --mode cli-only --target claude
ax install --mode cli-only --target gemini
ax install --mode cli-only --target cline
ax install --mode cli-only --target zed   # shared AGENTS.md only
```

`--mode cli-only` never writes MCP server configs, plugins, daemons, or IDE hooks.

| Target | Files written |
|---|---|
| cursor | `.cursor/rules/ax-cli.mdc` |
| vscode / copilot | `.github/copilot-instructions.md`, `.github/skills/ax-cli/SKILL.md` |
| windsurf | `.windsurfrules` (`AX_CLI` markers) |
| continue | `.continue/rules/ax-cli.md` (no `mcpServers/ax.json`) |
| claude | `.claude/rules/ax-cli.md`, `.claude/CLAUDE.md` |
| gemini | `GEMINI.md` |
| cline / antigravity | `.clinerules` |
| zed / jetbrains / kiro / codex / hermes / opencode | `AGENTS.md` + `.agents/skills/ax-cli/SKILL.md` |
| always | `AGENTS.md`, `.agents/skills/ax-cli/SKILL.md` |

## Agent loop

```bash
SESSION=$(ax session start --json | jq -r .session)
TURN=$(printf '%s' "$PROMPT" | ax turn begin --session "$SESSION" --prompt-stdin --json | jq -r .turnId)
# … ax explore / ax query / ax node …
printf '%s' '{"outcome":"…","decisions":[]}' \
  | ax turn finish --session "$SESSION" --turn "$TURN" --outcome-stdin --json
```

Research turns with no file changes still finish and write a Memory row with `source=cli-turn` (Command Center shows a **cli** badge).

| Need | CLI |
|---|---|
| Preflight / policy | `ax preflight` or `ax turn begin` |
| Code graph | `ax explore`, `ax query`, `ax node` |
| Guard | `ax policy guard <path>` |
| Capture directive | `ax policy capture` |
| Load skill | `ax policy skill <name>` |
| Re-index / memory | `ax sync`, `ax remember` / `ax recall` |
| Cache expand | `ax cache expand` |

## Dual-path with MCP

Prefer MCP tools when the ax MCP server is connected. When MCP is down or the project is CLI-only, use the table above — do not skip graph/policy work or fall back to Grep-first structural searches.

Team rules (`prefer-mcp-ops`, `explore-before-grep`, `startup`, `subagents`) and IDE bootstrap templates document both paths.

## Logging parity with MCP

CLI graph and policy commands write the **same inbound / outbound shapes** MCP uses (branded `[ax]`, not `[ax-mcp]`), plus Internal domain lines (`cli` / `policy` / `memory`). After `ax explore`, `ax remember` / `ax recall`, `ax preflight`, or `ax policy match|index|guard|skill`, open Command Center **Logging** for the project — you should see non-zero **in** and **out** counts as well as Internal rows.

Requires **ax 8.4.4+**. Details: [MCP Logging & Quality — Directions](/guides/mcp-quality/#directions-inbound-outbound-internal).

## Diagnostics

```bash
ax doctor --mode cli --json
```

## See also

- [CLI reference — sessions & turns](/reference/cli/#sessions-turns--cli-only-parity)
- [Policy Engine](/guides/policy-engine/)
- [MCP Logging & Quality](/guides/mcp-quality/)
- Repo guide: [`docs/CLI_ONLY.md`](https://github.com/GaryWenneker/ax/blob/main/docs/CLI_ONLY.md)
