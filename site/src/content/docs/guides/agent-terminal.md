---
title: Agent Terminal
description: Chat with the built-in ax agent or external CLIs from the Command Center web UI.
---

![Agent terminal — chat with AI agents, switch projects, profiles, and agents from the toolbar](/screenshots/cc-agent-terminal.png)

**ax v2.1.5+** adds an **Agent Terminal** to `ax web` — a chat panel for talking to AI agents without leaving the Command Center.

## Quick start

```bash
ax init
ax web                          # opens http://localhost:7070
# Navigate to Agent (#agent) in the sidebar
```

The Agent page includes:

- **Built-in ax agent** — runs preflight, explore/search/status MCP tools in-process, then synthesizes a reply via your configured LLM offload endpoint.
- **External agents** — Cursor and Claude Code CLI (when installed), with per-profile auth directories.
- **Workspace picker** — switch between indexed projects without restarting the server.
- **Maximize** — full-screen chat on desktop and mobile.

## Settings

Open **Settings → AI Agents** to:

| Action | Description |
|---|---|
| Install CLI | Install Claude Code, Codex, Cursor (winget), Gemini, OpenCode, etc. |
| Wire ax MCP | Write ax MCP config for selected editors/CLIs |
| Add profile | Create an isolated config directory (`~/.ax/agent-profiles/…`) |
| Authenticate | Stream auth/login for Claude or Cursor profiles |
| Terminal mode | `auto`, `builtin`, or `external` |

Agent preferences are stored in `~/.ax/config.json` under `agents` and `workspace.recent`.

## Built-in agent tool loop

Each chat turn runs:

1. `ax_preflight` — inject matched policy rules/skills
2. `ax_explore`, `ax_search`, or `ax_status` (heuristic from your prompt)
3. LLM synthesis with tool output as context

The UI shows tool start/end markers in the chat stream (`tool_start` / `tool_end` SSE events).

## External agents

When **Terminal mode** is `external` (or `auto` with a detected CLI):

- **Claude Code** — `claude -p …` with `CLAUDE_CONFIG_DIR` from the active profile
- **Cursor** — `cursor agent --print …` with `--user-data-dir` from the profile

If the external CLI fails, the built-in agent is used as fallback.

## Workspace hot-switch

`POST /api/workspace/switch` swaps the in-memory graph, policy, and ship daemon to another indexed project. The UI refreshes data in place — no page reload or server restart.

Browse roots include home, common dev folders, recent projects, and the current project parent.

## API endpoints

| Endpoint | Purpose |
|---|---|
| `GET /api/agent/status` | Installed targets + CLI/MCP status |
| `POST /api/agent/cli/install/stream` | SSE install agent CLI(s) |
| `POST /api/agent/install/stream` | SSE wire ax MCP |
| `POST /api/agent/chat/stream` | SSE chat (tokens, tools, done) |
| `GET /api/workspace/current` | Active project + recent list |
| `POST /api/workspace/switch` | Hot-switch workspace |
| `POST /api/workspace/init/stream` | Run `ax init` with streamed output |

## Hide the Agent nav item

In `.ax/ship.toml`:

```toml
[ui]
show_agent_terminal = false
```

Rebuild or restart `ax web` after changing ship config.

## See also

- [Command Center](/guides/command-center/) — quality gates and ship dashboard
- [Policy Engine](/guides/policy-engine/) — rules injected via preflight
- [MCP Server](/reference/mcp-server/) — tools the built-in agent calls
