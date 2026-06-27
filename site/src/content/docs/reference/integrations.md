---
title: Integrations
description: Supported agents, and manual MCP setup.
---

The interactive installer auto-detects and configures each supported agent — wiring the ax MCP server into each. For the agents that use an instructions file, it also writes a short marker-fenced ax section (`CLAUDE.md`, `AGENTS.md`, or `GEMINI.md`) so subagents and non-MCP harnesses learn the `ax explore` command; `ax uninstall` removes it.

## Supported agents

- **Claude Code**
- **Cursor**
- **Codex CLI**
- **opencode**
- **Hermes Agent**
- **Gemini CLI**
- **Antigravity IDE**
- **Kiro**

Run `npx @colbymchenry/ax` and pick your agent(s); see [Installation](/ax/getting-started/installation/) for the non-interactive flags.

## Manual setup

If you'd rather wire it up yourself, install globally:

```bash
npm install -g @colbymchenry/ax
```

Add the MCP server to `~/.claude.json`:

```json
{
  "mcpServers": {
    "ax": {
      "type": "stdio",
      "command": "ax",
      "args": ["serve", "--mcp"]
    }
  }
}
```

Optionally auto-allow ax's tools in `~/.claude/settings.json`:

```json
{
  "permissions": {
    "allow": [
      "mcp__ax__*"
    ]
  }
}
```

One wildcard auto-approves every ax tool. The server lists a single tool by default — `ax_explore` — but if you re-enable others via the `ax_MCP_TOOLS` environment variable, they're already permitted with no prompt.

:::tip
Cursor launches MCP subprocesses with the wrong working directory. The installer handles this for you by injecting a `--path` argument; if you wire Cursor up by hand, pass the project path explicitly.
:::
