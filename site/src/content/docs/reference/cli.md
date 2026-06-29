---
title: CLI
description: Every ax command and the flags it accepts.
---

```bash
ax                         # Run interactive installer
ax install                 # Run installer (explicit)
ax uninstall               # Remove ax from your agents (inverse of install)
ax init [path]             # Initialize a project + build its graph (one step)
ax uninit [path]           # Remove ax from a project (--force to skip prompt)
ax index [path]            # Full re-index from scratch (--force, --quiet, --verbose)
ax sync [path]             # Incremental update (--quiet)
ax status [path]           # Show statistics (--json)
ax unlock [path]           # Remove a stale lock file that's blocking indexing
ax query <search>          # Search symbols (--kind, --limit, --json)
ax explore <query>         # Relevant symbols' source + call paths in one shot (same output as the ax_explore MCP tool)
ax node <symbol|file>      # One symbol's source + callers, or read a file with line numbers (same output as ax_node)
ax files [path]            # Show file structure (--format, --filter, --pattern, --max-depth, --json)
ax callers <symbol>        # Find what calls a function/method (--limit, --json)
ax callees <symbol>        # Find what a function/method calls (--limit, --json)
ax impact <symbol>         # Analyze what code is affected by changing a symbol (--depth, --json)
ax affected [files...]     # Find test files affected by changes (see below)
ax daemon                  # Manage background daemons — pick one to stop (alias: daemons)
ax telemetry [on|off]      # Show or change anonymous usage telemetry
ax upgrade [version]       # Update to the latest release (--check, --force)
ax version                 # Print the installed version (also -v, --version)
ax help [command]          # Show help, optionally for one command
```

The MCP server (`ax serve --mcp`) is launched automatically by your agent — you don't run it by hand. See [MCP Server](/ax/reference/mcp-server/).

## init, index, and sync

`ax init` creates the local `.ax/` directory **and** builds the full graph in one step. (The old `-i`/`--index` flag is now a no-op, accepted only so existing scripts don't break.) After that the file watcher keeps the graph current automatically — `index` (a full rebuild from scratch) and `sync` (an incremental update) are only needed when the watcher is disabled or you're scripting against the index outside an agent session.

## Query commands

`query`, `callers`, `callees`, and `impact` all accept `--json` for machine-readable output.

```bash
ax query UserService --kind class --limit 10
ax callers handleRequest --json
ax impact AuthMiddleware --depth 3
```

`explore` and `node` are the CLI faces of the `ax_explore` and `ax_node` MCP tools — same output — so subagents and non-MCP harnesses can reach the graph from a shell.

## affected

Traces import dependencies transitively to find which test files are affected by changed source files. See [Affected Tests in CI](/ax/guides/affected-tests/) for options and a CI example.
