---
title: Integrations
description: Supported agents, and manual MCP setup.
---

The interactive installer auto-detects supported agents and wires the ax MCP server. For agents that use an instructions file, it also writes a short marker-fenced ax section so subagents learn the `ax explore` workflow; `ax uninstall` removes it.

## Supported agents

- **Claude Code**
- **Cursor**
- **Codex CLI**
- **opencode**
- **Hermes Agent**
- **Gemini CLI**
- **Antigravity IDE**
- **Kiro**

Run `npx @garywenneker/ax` or `ax install` — see [Installation](/getting-started/installation/) for non-interactive flags.

## Manual setup

Install the CLI globally (any method from [Installation](/getting-started/installation/)), then add the MCP server to `~/.claude.json`:

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

Optionally auto-allow ax tools in `~/.claude/settings.json`:

```json
{
  "permissions": {
    "allow": [
      "mcp__ax__*"
    ]
  }
}
```

:::tip
Cursor launches MCP subprocesses with the wrong working directory. The installer injects `--path` for you; if you configure Cursor manually, pass the project path explicitly.
:::
