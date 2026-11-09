# AX CLI-only mode

Use AX without MCP, plugins, IDE hooks, daemons, or network listeners. The CLI talks to the same shared engines MCP uses (preflight, sessions, durable context, memory, activity, cache).

## Install

```bash
# All distinct IDE instruction surfaces + shared AGENTS skill
ax install --mode cli-only

# Or pick targets
ax install --mode cli-only --target vscode
ax install --mode cli-only --target cursor
ax install --mode cli-only --target windsurf
ax install --mode cli-only --target continue
ax install --mode cli-only --target claude
ax install --mode cli-only --target gemini
ax install --mode cli-only --target cline
ax install --mode cli-only --target zed   # shared AGENTS.md only
```

This writes IDE instruction files and the shared `.agents/skills/ax-cli` skill. It does **not** write MCP configs or install hooks.

| Target | Files |
|---|---|
| cursor | `.cursor/rules/ax-cli.mdc` |
| vscode / copilot | `.github/copilot-instructions.md`, `.github/skills/ax-cli/SKILL.md` |
| windsurf | `.windsurfrules` (AX_CLI markers) |
| continue | `.continue/rules/ax-cli.md` (no `mcpServers/ax.json`) |
| claude | `.claude/rules/ax-cli.md`, `.claude/CLAUDE.md` |
| gemini | `GEMINI.md` |
| cline / antigravity | `.clinerules` |
| zed / jetbrains / kiro / codex / hermes / opencode | `AGENTS.md` + `.agents/skills/ax-cli/SKILL.md` |
| always | `AGENTS.md`, `.agents/skills/ax-cli/SKILL.md` |


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
