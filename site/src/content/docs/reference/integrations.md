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
- **VS Code (Copilot Chat)** — writes the workspace-local `.vscode/mcp.json` (root key `servers`, per VS Code's native MCP config format).
- **Windsurf (Cascade)** — writes `~/.codeium/windsurf/mcp_config.json` (global only — Windsurf has no project-level MCP config).
- **Zed** — writes `context_servers` in Zed's `settings.json` (`%APPDATA%\Zed\settings.json` on Windows, `~/.config/zed/settings.json` on macOS/Linux).

Run `npx @garywenneker/ax` or `ax install` — see [Installation](/getting-started/installation/) for non-interactive flags.

VS Code, Windsurf, and Zed are MCP-only integrations — there is no headless CLI to run these as background agents (unlike Claude Code, Cursor, Codex, opencode, or Gemini CLI), so `ax install` wires the MCP server but the "Agent terminal" in the Command Center will not offer them as a runnable target.

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
