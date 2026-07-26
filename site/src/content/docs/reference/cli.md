---
title: CLI
description: Complete reference for every ax command, argument, and flag (v4.0.0).
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

Interactive installer — writes MCP config for detected AI agents (Cursor, Claude Code, Codex, opencode, Gemini CLI, Antigravity, Kiro, Hermes, VS Code Copilot, Windsurf, Zed). Does **not** index a project. VS Code, Windsurf, and Zed are MCP-config-only targets (no prompt-hook or stop-hook — those are Claude Code-specific). See [Integrations](/reference/integrations/) for per-agent details.

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

```bash
ax uninstall
```

---

## Project lifecycle

### `ax init [path]`

Initialize a project: create `.ax/` (database, lock, `ship.toml`), index the project, install git hooks, then offer the agent installer.

On **first init**, runs a full index. If `.ax/ax.db` already exists, runs an incremental `ax sync` instead — use `ax index` when you need a full rebuild.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root (default: current directory) |
| `--workspace` | flag | Discover monorepo members (Cargo workspace + nested `.ax/`), write `members` to `ax.json`, and init each member |

```bash
ax init
ax init ./services/api
ax init --workspace
```

### `ax uninit [path]`

Delete the `.ax/` directory and remove git sync hooks. Permanently removes the local index.

| Argument | Type | Description |
|---|---|---|
| `path` | optional | Project root (default: current directory) |

```bash
ax uninit
ax uninit ./old-project
```

### `ax index [path]`

Full re-index from scratch (scan → extract → resolve). Use when the watcher is off or after large git operations.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--force` | flag | Clear the database before indexing |
| `--quiet` | flag | No progress bar or summary |
| `--verbose` | flag | Reserved for extra diagnostics |
| `--all` | flag | Index every workspace member listed in root `ax.json` |

```bash
ax index
ax index ./services/api --force
ax index --quiet
ax index --all
```

### `ax sync [path]`

Incremental update — re-parses only changed files.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--quiet` | flag | No progress bar or summary |
| `--watch` | flag | Watch filesystem and auto-sync until Ctrl+C |
| `--all` | flag | Sync every workspace member listed in root `ax.json` |

Also refreshes git hooks when the memory-capture line is missing (idempotent upgrade path).

```bash
ax sync
ax sync --quiet
ax sync --watch
ax sync --all
```

### `ax watch [path]`

Alias for `ax sync --watch`.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--quiet` | flag | No progress bar or summary |

```bash
ax watch
ax watch --quiet
```

### `ax status [path]`

Index statistics: node/edge/file counts, **document inventory by extension** (markdown, office, PDF, other opaque types), unresolved refs, pending sync, and last indexed time.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--json` | flag | Machine-readable JSON (`stats.docsByExtension`, `pendingFiles`) |

```bash
ax status
ax status --json
```

### `ax unlock [path]`

Remove a stale `.ax/ax.lock` left by a crashed process.

| Argument | Type | Description |
|---|---|---|
| `path` | optional | Project root |

```bash
ax unlock
ax unlock ./repo-with-stale-lock
```

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

```bash
ax query UserService
ax query "handleRequest" --kind function --limit 10
ax query auth --json
```

### `ax explore [query…]`

Natural-language explore — same output shape as the `ax_explore` MCP tool. Optional BYO LLM offload via `ax offload`.

| Argument / flag | Type | Description |
|---|---|---|
| `query` | optional (words) | Question or symbol names (multi-word allowed) |
| `--json` | flag | Structured JSON |

```bash
ax explore "how does auth flow work"
ax explore ax_preflight token savings --json
```

### `ax node [name]`

One symbol's details, or a file with line numbers and dependents — same as `ax_node` MCP.

| Argument | Type | Description |
|---|---|---|
| `name` | optional | Symbol name or file path |

```bash
ax node handleRequest
ax node crates/ax-cli/src/main.rs
```

### `ax files`

List indexed files and detected languages.

| Flag | Type | Description |
|---|---|---|
| `--format` | string | Output format (e.g. `tree` when supported) |
| `--json` | flag | JSON with paths and languages |

```bash
ax files
ax files --format tree --json
```

### `ax context <task>`

Build task-oriented markdown context for agent prompts.

| Argument | Type | Description |
|---|---|---|
| `task` | required | Task description |

```bash
ax context "add rate limiting to the login endpoint"
```

### `ax callers <symbol>`

Find symbols that call the given function/method/class. Output is JSON.

| Argument | Type | Description |
|---|---|---|
| `symbol` | required | Symbol name |

```bash
ax callers handleRequest
```

### `ax callees <symbol>`

Find symbols called by the given function/method. Output is JSON.

| Argument | Type | Description |
|---|---|---|
| `symbol` | required | Symbol name |

```bash
ax callees run_preflight
```

### `ax impact <symbol>`

Blast-radius subgraph — what breaks if you change this symbol. Output is JSON.

| Argument | Type | Description |
|---|---|---|
| `symbol` | required | Symbol name |

```bash
ax impact AuthService
```

---

## Architecture insights

Whole-graph analysis built on the same engine as the `ax_insights` / `ax_report` MCP tools and the Command Center **Graph** page. See [Architecture Insights](/guides/architecture-insights/).

### `ax insights [path]`

Detect subsystems (Leiden community detection), rank **god nodes** (most-connected concepts by in+out degree), and flag **surprising connections** (edges that cross both a community and a top-level module boundary).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--resolution` | number | `1.0` | Cluster granularity — higher yields more, smaller communities |
| `--god-limit` | number | `20` | Max god nodes to show |
| `--surprising-limit` | number | `20` | Max surprising connections to show |
| `--json` | flag | — | Machine-readable JSON |

Community assignments are persisted (`node_communities` table) and only recomputed after `ax index` / `ax sync` or when you re-run with a different `--resolution`.

```bash
ax insights
ax insights --resolution 1.4 --god-limit 30
ax insights --json
```

### `ax report [path]`

Render a full Markdown architecture report: god nodes, communities with member counts, surprising connections, dead code, an unresolved-refs summary, and a set of suggested questions templated from the top god nodes and communities.

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--out` | string | `AX_REPORT.md` | Output file at project root |
| `--resolution` | number | `1.0` | Cluster granularity |
| `--stdout` | flag | — | Print to stdout instead of writing a file |

```bash
ax report
ax report --out docs/ARCHITECTURE.md
ax report --stdout
```

### `ax export graph [path]`

Export the knowledge graph for external tools. Includes Leiden community ids, degree, and god-node flags. Command Center **Graph** can download the same formats (except interactive HTML) via **Export → Download**.

| Argument / flag | Type | Description |
|---|---|---|
| `path` | optional | Project root |
| `--format` | string | `html` \| `json` \| `dot` \| `graphml` \| `gexf` \| `cypher` \| `mermaid` \| `plantuml` (default: `json`) |
| `--out` | path | Output file (default depends on format) |
| `--resolution` | float | Cluster granularity (default: `1.0`) |
| `--limit` | int | Max nodes by degree (default: `3000`) |

```bash
ax export graph --format json
ax export graph --format graphml --out graph.graphml
ax export graph --format cypher --out import.cypher
ax export graph --format mermaid --out graph.mmd
```

### `ax export graph-html [path]`

Export the graph as a single self-contained, interactive HTML file (inline JSON + a small force-directed renderer) — portable, no server required. Node color = community, node size = degree, docs render as distinct squares.

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--out` | string | `graph.html` | Output file |
| `--resolution` | number | `1.0` | Cluster granularity |
| `--limit` | number | `3000` | Max nodes to include (top by degree) |

```bash
ax export graph-html
ax export graph-html --out graph.html --limit 1500
```

---

## Memory vault

See the [Memory vault guide](/guides/memory/).

### `ax remember <text>`

Store a durable project memory (decision, fix, convention). Flags similar existing memories so contradictions get updated instead of duplicated.

| Argument / flag | Type | Description |
|---|---|---|
| `text` | required | The memory content — what to remember and why |
| `--title` | string | Short title (defaults to first line) |
| `--kind` | string | `decision` \| `bug_fix` \| `architecture` \| `convention` \| `note` |
| `--tag` | string (repeat) | Tag |
| `--file` | string (repeat) | Related file path |
| `--json` | flag | JSON output |

```bash
ax remember "We use tiktoken o200k_base for token counts" --kind decision --tag tokenizer
ax remember "Sonar proxy strips Content-Encoding after decompression" --kind bug_fix --file crates/ax-web/src/sonar_proxy.rs
```

### `ax recall <query>`

Hybrid search (full-text + vector similarity, confidence-decay weighted) over project memories.

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `query` | required | — | Free-text query |
| `--limit` | number | `5` | Max results (≤50) |
| `--json` | flag | — | JSON output |

```bash
ax recall "why tiktoken"
ax recall sonar proxy --limit 10 --json
```

### `ax capture-git`

Mine recent non-merge git commits into memories (the "why" behind changes). Skips trivial messages and already-captured commits; safe to re-run.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--limit` | number | `100` | Number of commits to scan |
| `--quiet` | flag | — | No output (used by git hooks) |
| `--json` | flag | — | JSON output |

Runs automatically from **post-commit** and **post-merge** git hooks after `ax init`. Skips trivial messages and already-captured commits.

```bash
ax capture-git
ax capture-git --limit 50
ax capture-git --limit 1 --quiet    # same as the post-commit hook
```

### `ax memory export`

Export memories that carry a tag (default: `shared`) to `.ax/memory/shared.jsonl` for team git sync.

| Flag | Default | Description |
|---|---|---|
| `--tag` | `shared` | Only export memories with this tag |
| `--out` | `.ax/memory/shared.jsonl` | Output path |
| `--quiet` | off | No summary (for git hooks) |

```bash
ax remember "Adopt GraphQL federation" --kind decision --tag shared
ax memory export
ax memory export --tag team --out ./shared-memories.jsonl
```

### `ax memory import`

Import JSONL memories (upsert by id). Used after `git pull` or via the post-merge hook when `"memorySync": true` is set in `ax.json`.

```bash
ax memory import
ax memory import --path ./shared-memories.jsonl
```

---

## Git & testing

### `ax diff`

Symbol-level blast radius for the current git diff vs a base branch.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--base` | string | `main` | Base branch or ref |
| `--json` | flag | — | JSON output |

```bash
ax diff --base main
ax diff --base origin/main --json
```

### `ax test-impact`

Test-function impact analysis (git diff + TIA graph).

| Flag | Type | Default | Description |
|---|---|---|---|
| `--base` | string | `main` | Base branch or ref |
| `--json` | flag | — | JSON output |

```bash
ax test-impact --base main
ax test-impact --base develop --json
```

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
| `--ci` | flag | — | Headless CI: evaluate, print JSON on stdout, summary on stderr, **exit 1** if the gate failed |
| `--draft` | flag | — | Create draft PR after quality gate |
| `--title` | string | — | PR title |
| `--port` | number | `7070` | Dashboard port |
| `--open` | flag | — | Open browser |
| `--auto-commit` | flag | — | Force-enable Aider-style checkpoint commit before this evaluation, overriding `.ax/ship.toml` `[auto_commit]` for this run only |
| `--revert-on-fail` | flag | — | With `--auto-commit`, `git reset --mixed` the checkpoint if the quality gate fails (file contents stay on disk, uncommitted) |

```bash
ax ship --watch --open
ax ship --evaluate
ax ship --ci                     # CI / GitHub Actions
ax ship --evaluate --auto-commit --revert-on-fail
ax ship --draft --title "feat: memory vault"
ax web --open                    # Command Center without git watch
```

Example workflows:

- [GitHub Actions](https://github.com/GaryWenneker/ax/blob/main/docs/examples/github-actions-ship.yml)
- [GitLab CI](https://github.com/GaryWenneker/ax/blob/main/docs/examples/gitlab-ci-ship.yml)
- [Azure Pipelines](https://github.com/GaryWenneker/ax/blob/main/docs/examples/azure-pipelines-ship.yml)

---

### `ax share [path]`

Share Command Center on the LAN with a bearer token (read-only session).

| Argument / flag | Type | Default | Description |
|---|---|---|---|
| `path` | optional | cwd | Project root |
| `--port` | number | `7070` | Listen port |
| `--bind` | string | `0.0.0.0` | Bind address |
| `--open` | flag | — | Open browser with token URL |
| `--token` | string | random | Share token |

```bash
ax share --open
```

### `ax lsp status` / `ax lsp enrich`

Optional Language Server enrichment. `enrich` calls `textDocument/definition` on
unresolved refs and writes edges with confidence `exact`.

```bash
ax lsp status
ax lsp enrich --limit 100
```

---

### `ax stop-hook`

Claude Code `Stop` / `SubagentStop` hook target — internal command wired automatically by `ax install` for Claude Code, not meant to be run manually. Reads Claude's JSON payload on stdin, runs `ax_guard`-equivalent checks against every uncommitted file (`git status --porcelain`), and on a CRITICAL violation prints `{"decision": "block", "reason": "..."}` so Claude fixes the issue before the turn actually ends. Honors `stop_hook_active` to avoid infinite loops and no-ops when there's no indexed policy.

| Env var | Effect |
|---|---|
| `AX_NO_STOP_HOOK=1` | Disable — hook exits immediately without checking |

```bash
# Wired automatically:
ax install                       # adds Stop + SubagentStop hooks for Claude Code
```

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

```bash
ax version
ax --version
```

### `ax upgrade [version]`

Self-update from [getax.wenneker.io/releases/latest.txt](https://getax.wenneker.io/releases/latest.txt) (GitHub fallback). Non-interactive — no confirmation prompt.

| Argument / flag | Type | Description |
|---|---|---|
| `version` | optional | Pin a release tag (e.g. `v3.0.0`) |
| `--check` | flag | Check for updates without installing |
| `--local [archive]` | flag / path | Install from local `dist/ax-<platform>.zip` (maintainers); optional explicit archive path |

```bash
ax upgrade
ax upgrade v3.0.0
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

```bash
ax telemetry status
ax telemetry on
ax telemetry off
```

---

## Context savings

### `ax savings`

Estimated context-token savings from MCP graph queries. Stored locally in `~/.ax/usage.db`. Each `ax_explore` / graph MCP call logs response size and a counterfactual file-read estimate.

| Flag | Type | Default | Description |
|---|---|---|---|
| `--period` | string | `month_to_date` | `week` \| `month_to_date` \| `month` \| `year` \| `custom` |
| `--from` | `YYYY-MM-DD` | — | Start date (required for `custom`) |
| `--to` | `YYYY-MM-DD` | — | End date (optional for `custom`) |
| `--json` | flag | — | JSON summary with per-tool and daily breakdown |

```bash
ax savings
ax savings --period week --json
ax savings import --all
ax savings import --claude --cursor
ax savings tag-session --session-id <uuid> --model composer-2.5-fast
ax savings hook install
```

| Subcommand | Description |
|---|---|
| `import --all` | Import Cursor + Claude Code session transcripts |
| `tag-session` | Manually record model name for a session id |
| `hook install` | Install Cursor sessionStart hook into `~/.cursor/hooks/` |

The **Savings** page in `ax web` (enable in Settings) exposes the same filters. See [Token savings](/guides/token-savings/).

### `ax mcp audit`

Correlate `<project>/.ax/mcp-verbose-*.log` (daily files) with a Cursor agent transcript and score MCP quality (preflight, enrichment, explore-before-grep, correlation). Same engine powers the Command Center **Quality** status-bar chip and slide-out. Persists a snapshot to `.ax/audit/latest.json`. Exit code `2` when critical findings are present.

Scoring notes:

- Enrichment attaches `enrich` / `enrich done` lines (including `final_inject_chars`) to the open `ax_preflight` cluster so empty-inject false positives are avoided.
- Cursor transcripts often lack JSON timestamps; the auditor parses embedded `<timestamp>…</timestamp>` markers and carries them onto following tool calls so the rolling window matches verbose activity.
- Untimed transcripts (no embed/JSON ts) are capped and capacity-matched against the verbose window; when verbose is quiet in-window, the whole-session transcript tail is dropped so idle windows do not false-flag `UncorrelatedTool` / `VerboseGap`.
- Tool mix prefers the richer of verbose vs transcript counts for explore/graph/preflight/guard (`CallDynamicTool` included).
- Default session pick prefers the recent transcript with the most ax tool calls (avoids linking an empty newest chat).
- Verbose lines may include `session=<uuid>` (from the Cursor sessionStart hook / `AX_CURSOR_SESSION_ID`) for tighter transcript↔log correlation — run `ax savings hook install` once per machine.
- MCP error clusters that recover with a successful same-tool retry within 60s do not penalize the score.
- When verbose log files exist but have no clusters in the window, `UncorrelatedTool` is medium (widen `--window-minutes` or pass `--session`) instead of critical "enable verbose".

| Flag | Type | Default | Description |
|---|---|---|---|
| `--session` | uuid \| path | latest transcript | Cursor session id or `.jsonl` path |
| `--window-minutes` | number | `30` | Rolling window when no `--session` |
| `--json` | flag | — | Machine-readable `QualitySnapshot` |

```bash
ax mcp audit
ax mcp audit --window-minutes 60
ax mcp audit --session <uuid>
ax mcp audit --session path/to/transcript.jsonl --json
```

---

## Cursor auth switching

### `ax cursor auth`

Save and restore Cursor IDE auth sessions for fast subscription switching. Snapshots `cursorAuth/*` keys from `%APPDATA%\\Cursor\\User\\globalStorage\\state.vscdb` plus `auth.json` into `~/.ax/cursor-auth/`.

| Subcommand | Description |
|---|---|
| `status` | Show live plan, email, login method |
| `list` | List saved profiles |
| `save <name>` | Snapshot current Cursor auth |
| `use <name>` | Apply a saved profile (restart Cursor after) |
| `show <name>` | Inspect a saved profile without applying |

| Flag | Applies to | Description |
|---|---|---|
| `--label` | `save` | Human-readable profile label |
| `--from-auth-json` | `save` | Bootstrap from stale `auth.json` only |
| `--email`, `--membership`, `--subscription-status`, `--sign-up-type` | `save` | Override metadata when bootstrapping |
| `--force` | `use` | Apply while Cursor is running (still restart after) |
| `--json` | all | JSON output |

Close Cursor before `use`, then restart Cursor after switching.

```bash
ax cursor auth status
ax cursor auth save enterprise --label "Work"
ax cursor auth save personal --from-auth-json --email you@gmail.com --membership pro_plus
ax cursor auth list
ax cursor auth use personal
ax cursor auth use enterprise
```

---

## Explore offload

### `ax offload`

Configure optional LLM offload for `ax explore` (BYO OpenAI-compatible API). Stored in `~/.ax/config.json`.

#### `ax offload status`

Show current endpoint and key env var name.

```bash
ax offload status
```

#### `ax offload set-endpoint <url>`

| Argument / flag | Type | Description |
|---|---|---|
| `url` | required | Base URL (must end with `/v1`) |
| `--key-env` | string | Environment variable holding the API key |

```bash
ax offload set-endpoint https://api.openai.com/v1 --key-env OPENAI_API_KEY
```

#### `ax offload clear`

Remove offload configuration.

```bash
ax offload clear
```

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

```bash
ax web
ax web ./my-repo --port 8080 --open
```

---

## Policy

Policy commands manage `.ax/policy/` rules and skills. See [Policy Engine](/guides/policy-engine/).

Most policy subcommands accept an optional `[path]` project root (default: current directory).

### `ax policy index [path]`

Index `.ax/policy/` into SQLite. In **database** mode without `--force`, shows DB counts only.

| Flag | Description |
|---|---|
| `--force` | Re-import from disk (database mode) or full replace (files mode) |

```bash
ax policy index
ax policy index --force
```

### `ax policy import [path]`

Import `.mdc` / `SKILL.md` from disk into database (merge — keeps DB-only rows).

```bash
ax policy import
ax policy import ./my-project
```

### `ax policy pull <git-url> [path]`

Clone a remote git policy registry into `.ax/policy/vendored/<name>/`, copy `rules/` and `skills/` into the project policy tree, and re-index.

| Argument / flag | Type | Description |
|---|---|---|
| `git-url` | required | HTTPS or SSH git URL |
| `--name` | string | Vendor subdirectory name (default: repo name) |

```bash
ax policy pull https://github.com/acme/ax-org-policy.git
ax policy pull git@github.com:acme/ax-org-policy.git --name org
```

### `ax policy export [path]`

Export database policy to files.

| Flag | Default | Description |
|---|---|---|
| `--out` | `.ax/policy/export` | Output directory |

```bash
ax policy export
ax policy export --out ./policy-backup
```

### `ax policy match <prompt> [path]`

Test which rules/skills match a prompt.

| Argument / flag | Type | Description |
|---|---|---|
| `prompt` | required | Prompt text to match |
| `--file` | string (repeat) | Open/changed file paths for glob matching |
| `--json` | flag | JSON output |

```bash
ax policy match "deploy to production" --file src/ship.rs
ax policy match "utf8 bom" --json
```

### `ax policy rules [path]`

List indexed rules.

| Flag | Description |
|---|---|
| `--json` | JSON output |

```bash
ax policy rules
ax policy rules --json
```

### `ax policy skills [path]`

List indexed skills.

| Flag | Description |
|---|---|
| `--json` | JSON output |

```bash
ax policy skills
ax policy skill startup
```

### `ax policy skill <name> [path]`

Print one skill body (markdown).

| Argument | Description |
|---|---|
| `name` | Skill name |

```bash
ax policy skill release
```

### `ax policy guard <file>`

Pre-write CRITICAL guard check. Built-in: UTF-8/BOM encoding, secrets paths. Generic: any CRITICAL rule can add a `guard: forbid-path: "<glob>"`, `guard: forbid-content: "<substring or /regex/>"`, or `guard: require-content: "<substring or /regex/>"` directive line to its body to opt into this check without code changes — no separate flag needed, directives are picked up automatically from indexed policy.

| Argument / flag | Type | Description |
|---|---|---|
| `file` | required | Path relative to project root |
| `-p`, `--path` | string | Project root (default: cwd) |
| `--delete` | flag | Run delete guard instead of write (default: write) |
| `--json` | flag | JSON output |

```bash
ax policy guard README.md
ax policy guard .env --json
ax policy guard old-file.txt --delete
```

### `ax policy test [path]`

Smoke tests: match, guard, bootstrap, subagents.

| Flag | Description |
|---|---|
| `--json` | JSON output |

```bash
ax policy test
ax policy test --json
```

### `ax policy sync [path]`

Verify managed policy instruction files (`.ax/policy/skills/startup/SKILL.md`, …) and IDE bootstrap files (`.cursor/rules/ax.mdc`, `AGENTS.md`, etc.). Required managed files must match the embedded init templates; optional team copies only fail the basic preflight checks.

| Flag | Description |
|---|---|
| `--fix` | Restore missing or drifted managed files from embedded init templates |

```bash
ax policy sync
ax policy sync --fix
```

After `--fix` restores `.ax/policy/` files in database mode, run `ax policy import` so `ax.db` picks up the updated skill bodies.
### `ax policy capture <prompt> [path]`

Propose or save a team rule from directive language (`always`, `you must`, `@rule`, …).

| Argument / flag | Type | Description |
|---|---|---|
| `prompt` | required | User prompt containing a directive |
| `--file` | string (repeat) | Open files (for glob inference) |
| `--yes` | flag | Save with defaults (skip interview) |
| `--json` | flag | JSON proposal or result |

```bash
ax policy capture "always update docs when adding a feature"
ax policy capture "never commit secrets" --file .env --json
```

### `ax policy storage`

Show or set policy storage mode (`files` vs `database`).

#### `ax policy storage status [path]`

Show effective storage mode, config paths, and project/global values.

| Flag | Description |
|---|---|
| `--json` | JSON output |

```bash
ax policy storage status
ax policy storage status --json
```

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

```bash
ax policy storage files --migrate
ax policy storage files --global
```

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

# Architecture insights
ax insights --json
ax report --out AX_REPORT.md
ax export graph-html --out graph.html

# Memory
ax remember "we picked X because Y" --kind decision
ax recall "why X"
ax capture-git

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
