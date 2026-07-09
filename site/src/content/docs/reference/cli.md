---
title: CLI
description: Complete reference for every ax command, argument, and flag (v2.1.4).
---

Run `ax <command> --help` for the same information from the installed binary. Global help: `ax --help`.

The MCP server (`ax serve --mcp`) is started by your agent — you do not run it manually. See [MCP Server](/reference/mcp-server/).

## Global options

| Flag | Description |
|---|---|
| `-h`, `--help` | Print help for `ax` or a subcommand |
| `-V`, `--version` | Print installed version (same as `ax version`) |

Running `ax` with **no subcommand** starts the interactive installer (same as `ax install`).

## Environment variables

| Variable | Effect |
|---|---|
| `AX_FORCE_COLOR=1` | Force ANSI colors |
| `AX_UNICODE=1` | Force Unicode spinners/glyphs on Windows |
| `AX_ASCII=1` | Force ASCII glyphs everywhere |
| `NO_COLOR` | Disable ANSI colors |
| `AX_NO_UPDATE_CHECK=1` | Skip background upgrade notices after commands |
| `AX_NO_POLICY=1` | Skip policy injection in prompt-hook |
| `AX_NO_POLICY_CAPTURE=1` | Skip directive capture hints in prompt-hook |
| `AX_POLICY_MAX_CHARS` | Cap policy inject size (default `16000`) |
| `AX_TELEMETRY=0` / `DO_NOT_TRACK=1` | Disable anonymous telemetry |
| `AX_OFFLOAD_URL`, `AX_OFFLOAD_KEY`, … | Override explore offload settings |

## Configuration files

| File | Scope | Contents |
|---|---|---|
| `~/.ax/config.json` | Global | `index`, `offload`, `policy.storage` |
| `<project>/ax.json` | Per-project | Same keys — project wins on conflict |
| `<project>/.ax/` | Per-project | `ax.db`, lock file, optional `policy/` |

See [Configuration](/getting-started/configuration/) for the full schema.

---

## Install & uninstall

### `ax` / `ax install`

Interactive installer — writes MCP config for detected AI agents (Cursor, Claude Code, Codex, opencode, Gemini, Kiro, etc.). Does **not** index a project.

| Argument / flag | Type | Description |
|---|---|---|
| `--yes` | flag | Non-interactive: skip prompts, install detected agents |
| `--all` | flag | Configure every supported agent, not only detected ones |

```bash
ax install
ax install --yes
ax install --yes --all
```

### `ax uninstall`

Remove ax entries from agent MCP configuration files. Does not delete `~/.ax` or project `.ax/` indexes.

No flags.

---

## Project lifecycle

### `ax init [path]`

Initialize a project: create `.ax/` (database, lock, `ship.toml`), run a full index, install git hooks, then offer the agent installer.

| Argument | Type | Description |
|---|---|---|
| `path` | optional | Project root (default: current directory) |

```bash
ax init
ax init ./services/api
```

### `ax uninit [path]`

Delete the `.ax/` directory and remove git sync hooks. Permanently removes the local index.

| Argument | Type | Description |
|---|---|---|
| `path` | optional | Project root (default: current directory) |

### `ax index [path]`

Full re-index from scratch (scan → extract → resolve). Use when the watcher is off or after large git operations.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--force` | flag | Clear the database before indexing |
| `--quiet` | flag | No progress bar or summary |
| `--verbose` | flag | Reserved for extra diagnostics |

### `ax sync [path]`

Incremental update — re-parses only changed files.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--quiet` | flag | No progress bar or summary |
| `--watch` | flag | Watch filesystem and auto-sync until Ctrl+C |

### `ax watch [path]`

Alias for `ax sync --watch`.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--quiet` | flag | No progress bar or summary |

### `ax status [path]`

Index statistics: node/edge/file counts, unresolved refs, last indexed time.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--json` | flag | Machine-readable JSON |

### `ax unlock [path]`

Remove a stale `.ax/ax.lock` left by a crashed process.

| Argument | Type | Description |
|---|---|---|
| `path` | optional | Project root |

---

## Graph query

### `ax query <text>`

Full-text symbol search (FTS5). For natural-language questions use `ax explore`.

| Argument / flag | Type | Description |
|---|---|---|
| `text` | required | Search string |
| `--kind` | string | Filter by node kind (`function`, `class`, `file`, …) |
| `--limit` | number | Max results |
| `--json` | flag | JSON array output |

### `ax explore [query…]`

Natural-language explore — same output shape as the `ax_explore` MCP tool. Optional BYO LLM offload via `ax offload`.

| Argument / flag | Type | Description |
|---|---|---|
| `query` | optional (words) | Question or symbol names (multi-word allowed) |
| `--json` | flag | Structured JSON |

### `ax node [name]`

One symbol's details, or a file with line numbers and dependents — same as `ax_node` MCP.

| Argument | Type | Description |
|---|---|---|
| `name` | optional | Symbol name or file path |

### `ax files`

List indexed files and detected languages.

| Flag | Type | Description |
|---|---|---|
| `--format` | string | Output format (e.g. `tree` when supported) |
| `--json` | flag | JSON with paths and languages |

### `ax context <task>`

Build task-oriented markdown context for agent prompts.

| Argument | Type | Description |
|---|---|---|
| `task` | required | Task description |

### `ax callers <symbol>`

Find symbols that call the given function/method/class. Output is JSON.

| Argument | Type | Description |
|---|---|---|
| `symbol` | required | Symbol name |

### `ax callees <symbol>`

Find symbols called by the given function/method. Output is JSON.

| Argument | Type | Description |
|---|---|---|
| `symbol` | required | Symbol name |

### `ax impact <symbol>`

Blast-radius subgraph — what breaks if you change this symbol. Output is JSON.

| Argument | Type | Description |
|---|---|---|
| `symbol` | required | Symbol name |

---

## Git & testing

### `ax diff`

Symbol-level blast radius for the current git diff vs a base branch.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--base` | string | `main` | Base branch or ref |
| `--json` | flag | — | JSON output |

### `ax test-impact`

Test-function impact analysis (git diff + TIA graph).

| Flag | Type | Default | Description |
|---|---|---|---|
| `--base` | string | `main` | Base branch or ref |
| `--json` | flag | — | JSON output |

### `ax affected [files…]`

Reverse impact: find **test files** affected by changes. See [Affected Tests in CI](/guides/affected-tests/).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `files` | optional (repeat) | — | Changed source file paths |
| `--stdin` | flag | — | Read changed paths from stdin |
| `--base` | string | `main` | Base branch when no files given |
| `-d`, `--depth` | number | `5` | Max dependency traversal depth |
| `-f`, `--filter` | string | — | Glob filter for test files |
| `-j`, `--json` | flag | — | JSON output |
| `-q`, `--quiet` | flag | — | File paths only |

```bash
ax affected src/auth/login.ts
git diff --name-only main | ax affected --stdin
```

---

## Command Center

### `ax ship [path]`

Git-aware quality gates, SSE dashboard, draft PRs. See [Command Center](/guides/command-center/).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--watch` | flag | — | Watch git events and open dashboard |
| `--evaluate` | flag | — | Run one quality-gate evaluation |
| `--draft` | flag | — | Create draft PR after quality gate |
| `--title` | string | — | PR title |
| `--port` | number | `7070` | Dashboard port |
| `--open` | flag | — | Open browser |

---

## Daemon

### `ax daemon [path] [status|stop]`

MCP background daemon control (shared index connection per project).

| Argument / subcommand | Description |
|---|---|
| `path` | Optional project root (before subcommand) |
| `status` | Show pid, port, socket from `.ax/daemon.json` (default) |
| `stop` | Stop the running daemon |

```bash
ax daemon status
ax daemon stop
ax daemon ./repo status
```

---

## Version & upgrade

### `ax version`

Print installed version. Aliases: `ax -V`, `ax --version`.

### `ax upgrade [version]`

Self-update from [getax.wenneker.io/releases/latest.txt](https://getax.wenneker.io/releases/latest.txt) (GitHub fallback). Non-interactive — no confirmation prompt.

| Argument / flag | Type | Description |
|---|---|---|
| `version` | optional | Pin a release tag (e.g. `v2.1.4`) |
| `--check` | flag | Check for updates without installing |
| `--local [archive]` | flag / path | Install from local `dist/ax-<platform>.zip` (maintainers); optional explicit archive path |

```bash
ax upgrade
ax upgrade v2.1.4
ax upgrade --check
ax upgrade --local
ax upgrade --local dist/ax-win32-x64.zip
```

On Windows the process exits immediately; a detached helper finishes the binary swap. Open a new terminal and run `ax version` to verify.

---

## Telemetry

### `ax telemetry [on|off|status]`

Anonymous usage telemetry (command names and coarse buckets — never source code or paths).

| Argument | Description |
|---|---|
| `on` | Enable telemetry |
| `off` | Disable telemetry |
| `status` | Show current setting (default when omitted) |

---

## Token usage

### `ax tokens`

Per-model LLM token usage from explore offload. Stored locally in `~/.ax/usage.db` (global across projects). Counts are recorded when offload runs via `ax explore` or `ax_explore` MCP.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--period` | string | `month_to_date` | `week` \| `month_to_date` \| `month` \| `year` \| `custom` |
| `--from` | `YYYY-MM-DD` | — | Start date (required for `custom`) |
| `--to` | `YYYY-MM-DD` | — | End date (optional for `custom`) |
| `--json` | flag | — | JSON summary with per-model and daily breakdown |

```bash
ax tokens
ax tokens --period week
ax tokens --period year --json
ax tokens --period custom --from 2026-01-01 --to 2026-03-31
```

The **Tokens** page in `ax web` exposes the same filters and shows totals, per-model tables, and daily usage.

---

## Explore offload

### `ax offload`

Configure optional LLM offload for `ax explore` (BYO OpenAI-compatible API). Stored in `~/.ax/config.json`.

#### `ax offload status`

Show current endpoint and key env var name.

#### `ax offload set-endpoint <url>`

| Argument / flag | Type | Description |
|---|---|---|
| `url` | required | Base URL (must end with `/v1`) |
| `--key-env` | string | Environment variable holding the API key |

#### `ax offload clear`

Remove offload configuration.

---

## Web UI

### `ax web [path]`

Local web UI — graph browser, policy editor, token usage dashboard, Command Center tab.

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--port` | number | `7070` | Listen port |
| `--open` | flag | — | Open browser after start |

Set `AX_WEB_READONLY=1` for browse-only mode.

---

## Policy

Policy commands manage `.ax/policy/` rules and skills. See [Policy Engine](/guides/policy-engine/).

Most policy subcommands accept an optional `[path]` project root (default: current directory).

### `ax policy index [path]`

Index `.ax/policy/` into SQLite. In **database** mode without `--force`, shows DB counts only.

| Flag | Description |
|---|---|
| `--force` | Re-import from disk (database mode) or full replace (files mode) |

### `ax policy import [path]`

Import `.mdc` / `SKILL.md` from disk into database (merge — keeps DB-only rows).

### `ax policy export [path]`

Export database policy to files.

| Flag | Default | Description |
|---|---|---|
| `--out` | `.ax/policy/export` | Output directory |

### `ax policy match <prompt> [path]`

Test which rules/skills match a prompt.

| Argument / flag | Type | Description |
|---|---|---|
| `prompt` | required | Prompt text to match |
| `--file` | string (repeat) | Open/changed file paths for glob matching |
| `--json` | flag | JSON output |

### `ax policy rules [path]`

List indexed rules.

| Flag | Description |
|---|---|
| `--json` | JSON output |

### `ax policy skills [path]`

List indexed skills.

| Flag | Description |
|---|---|
| `--json` | JSON output |

### `ax policy skill <name> [path]`

Print one skill body (markdown).

| Argument | Description |
|---|---|
| `name` | Skill name |

### `ax policy guard <file>`

Pre-write CRITICAL guard check (UTF-8, secrets paths, etc.).

| Argument / flag | Type | Description |
|---|---|---|
| `file` | required | Path relative to project root |
| `-p`, `--path` | string | Project root (default: cwd) |
| `--delete` | flag | Run delete guard instead of write (default: write) |
| `--json` | flag | JSON output |

### `ax policy test [path]`

Smoke tests: match, guard, bootstrap, subagents.

| Flag | Description |
|---|---|
| `--json` | JSON output |

### `ax policy sync [path]`

Verify IDE bootstrap files (`.cursor/rules/ax.mdc`, `AGENTS.md`, etc.).

| Flag | Description |
|---|---|
| `--fix` | Restore missing or drifted managed files from embedded templates |

### `ax policy capture <prompt> [path]`

Propose or save a team rule from directive language (`always`, `you must`, `@rule`, …).

| Argument / flag | Type | Description |
|---|---|---|
| `prompt` | required | User prompt containing a directive |
| `--file` | string (repeat) | Open files (for glob inference) |
| `--yes` | flag | Save with defaults (skip interview) |
| `--json` | flag | JSON proposal or result |

### `ax policy storage`

Show or set policy storage mode (`files` vs `database`).

#### `ax policy storage status [path]`

Show effective storage mode, config paths, and project/global values.

| Flag | Description |
|---|---|
| `--json` | JSON output |

#### `ax policy storage database [path]`

Set **database** as source of truth (`ax.db`).

| Flag | Description |
|---|---|
| `--global` | Write to `~/.ax/config.json` instead of project `ax.json` |
| `--migrate` | Scan repo for rules/skills; propose or import |
| `--yes` | With `--migrate`: import all candidates with parsed defaults |
| `--json` | JSON output |

```bash
ax policy storage database --migrate          # propose (interview questions)
ax policy storage database --migrate --yes    # apply import
```

#### `ax policy storage files [path]`

Set **files** as source of truth (`.ax/policy/` on disk).

| Flag | Description |
|---|---|
| `--global` | Write to `~/.ax/config.json` |
| `--migrate` | Export database policy to `.ax/policy/` files |
| `--json` | JSON output |

---

## Hidden / internal commands

Not for daily use — invoked by agents, installers, or upgrade helpers.

| Command | Purpose |
|---|---|
| `ax serve --mcp` | Stdio MCP server (agent-launched) |
| `ax serve --mcp --daemon` | Background MCP daemon |
| `ax prompt-hook` | Claude `UserPromptSubmit` hook (stdin JSON) |
| `ax watchdog-child` | MCP liveness watchdog child |
| `ax upgrade-apply` | Windows upgrade swap helper |

---

## Quick reference

```bash
# Lifecycle
ax init [path]
ax sync --watch
ax status --json

# Graph
ax explore "auth flow" --json
ax query UserService --kind class
ax callers handleRequest

# Git / ship
ax diff --base main --json
ax test-impact --base main
ax ship --watch --open

# Policy
ax policy storage status
ax policy match "deploy" --file src/app.ts
ax policy capture "always validate input"

# Upgrade
ax upgrade
ax upgrade --local
```
